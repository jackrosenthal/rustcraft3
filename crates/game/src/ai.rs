//! Computer players: each runs its race script (with common.ai) in its own JASS VM, and the
//! engine carries out what the script asks for through the AI natives (production,
//! harvesting, towns, and the attack and defense "captains").

use std::collections::{HashMap, VecDeque};

use w3_jass::{FuncId, NativeResult, Outcome, Thread, Value, Vm};

use crate::natives::{Args, boolean, handle, int, none, reg};
use crate::objects::{Id, id_from_int, id_from_str, id_to_int};
use crate::types::*;
use crate::world::{NativeFn, World};

/// The AI script's virtual machine and its threads.  Kept apart from [`AiState`] so the VM
/// can be borrowed mutably while natives change the AI state in the world.
pub struct AiVm {
    pub vm: Vm,
    pub sleepers: Vec<(f32, Thread)>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Captain {
    pub home: (f32, f32),
    pub units: Vec<u32>,
    pub goal: Option<(f32, f32)>,
    pub target: u32,
    pub retreating: bool,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct AiState {
    pub player: u8,
    /// The script running, to compile again when a saved game loads.
    pub script: String,
    #[serde(skip)]
    pub natives: Vec<Option<NativeFn>>,
    #[serde(skip)]
    pub native_names: Vec<String>,
    /// Threads started with StartThread, run on the next AI update.
    pub spawned: Vec<Thread>,
    pub melee: bool,
    pub flags: HashMap<String, bool>,
    /// Harvest requests in priority order: (gold?, town, workers).
    pub harvest: Vec<(bool, i32, i32)>,
    pub attack: Captain,
    pub defense: Captain,
    pub assault: Vec<(Id, i32)>,
    pub defenders: Vec<(Id, i32)>,
    pub hero_levels: Option<FuncId>,
    pub hero_context: (i32, i32),
    pub commands: VecDeque<(i32, i32)>,
    pub enemy_base: u32,
    pub stage: Option<(f32, f32)>,
    pub harvest_timer: f32,
    pub guard_posts: Vec<(Id, f32, f32)>,
}

/// Distance within which a town hall and a gold mine belong to the same town.
const TOWN_MINE_RADIUS: f32 = 1500.0;

impl World {
    /// Starts a computer player's AI script (StartMeleeAI / StartCampaignAI).
    /// Compiles an AI script; returns its machine and its natives' functions and names.
    pub(crate) fn compile_ai(&self, script: &str) -> Option<(Vm, Vec<Option<NativeFn>>, Vec<String>)> {
        let read = |n: &str| self.chain.read_string(n).ok();
        // Melee scripts are named without their folder ("orc.ai").
        let src = read(script).or_else(|| read(&format!("Scripts\\{script}")));
        let (Some(common_j), Some(common_ai), Some(src)) = (read("Scripts\\common.j"), read("Scripts\\common.ai"), src) else {
            log::warn!("AI script {script} not found");
            return None;
        };
        let module = match w3_jass::compile([("common.j", &*common_j), ("common.ai", &*common_ai), (script, &*src)]) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("AI script {script}: {e}");
                return None;
            }
        };
        let vm = Vm::new(module);
        let mut table = crate::natives::table();
        for (k, v) in table_ai() {
            table.insert(k, v);
        }
        let natives = vm.module.natives.iter().map(|n| table.get(&*n.name).copied()).collect();
        let names = vm.module.natives.iter().map(|n| n.name.to_string()).collect();
        Some((vm, natives, names))
    }

    pub fn start_ai(&mut self, player: u8, script: &str, melee: bool) {
        let Some((mut vm, natives, native_names)) = self.compile_ai(script) else { return };
        let state = AiState {
            player,
            script: script.to_string(),
            natives,
            native_names,
            melee,
            attack: Captain { home: self.start_of(player), ..Default::default() },
            defense: Captain { home: self.start_of(player), ..Default::default() },
            ..Default::default()
        };
        self.ai.insert(player, state);
        self.ai_current = Some(player);
        if let Err(e) = vm.init_globals(self) {
            log::warn!("AI {script} globals: {e}");
        }
        let main = vm.module.function("main");
        self.ai_current = None;
        let mut aivm = AiVm { vm, sleepers: Vec::new() };
        if let Some(f) = main {
            let t = aivm.vm.new_thread(f, &[], true);
            aivm.sleepers.push((self.time, t));
        }
        self.ai_vms.insert(player, aivm);
        log::info!("player {player} runs AI script {script}");
    }

    pub fn start_of(&self, player: u8) -> (f32, f32) {
        let s = self.players.get(player as usize).map_or(-1, |p| p.start_location);
        self.start_locations.get(s.max(0) as usize).map_or((0.0, 0.0), |l| (l.x, l.y))
    }

    /// Runs every computer player's due threads and engine-side AI.
    pub fn update_ai(&mut self, dt: f32) {
        let players: Vec<u8> = self.ai_vms.keys().copied().collect();
        for p in players {
            let Some(mut aivm) = self.ai_vms.remove(&p) else { continue };
            self.ai_current = Some(p);
            let now = self.time;
            let due: Vec<Thread> = {
                let mut due = Vec::new();
                let mut i = 0;
                while i < aivm.sleepers.len() {
                    if aivm.sleepers[i].0 <= now {
                        due.push(aivm.sleepers.swap_remove(i).1);
                    } else {
                        i += 1;
                    }
                }
                due
            };
            for mut t in due {
                match aivm.vm.run(self, &mut t) {
                    Outcome::Sleeping(s) => aivm.sleepers.push((now + s.max(0.0), t)),
                    Outcome::Finished(_) => {}
                    Outcome::Aborted(e) => log::debug!("AI thread for player {p} ended: {e}"),
                }
                let spawned = self.ai.get_mut(&p).map(|a| std::mem::take(&mut a.spawned)).unwrap_or_default();
                for t in spawned {
                    aivm.sleepers.push((now, t));
                }
            }
            self.ai_hero_skills(p, &mut aivm);
            self.ai_current = None;
            self.ai_vms.insert(p, aivm);
            self.ai_engine(p, dt);
        }
    }

    /// Spends AI heroes' skill points on what the script's SetHeroLevels callback picks
    /// (it reads GetHeroId and GetHeroLevelAI and returns an ability id).
    fn ai_hero_skills(&mut self, p: u8, aivm: &mut AiVm) {
        let Some(f) = self.ai.get(&p).and_then(|s| s.hero_levels) else { return };
        let heroes: Vec<u32> = self.ai_units(p).into_iter().filter(|&h| self.unit(h).is_some_and(|u| u.is_hero && u.skill_points > 0)).collect();
        for h in heroes {
            // Guard against a callback that picks something unlearnable.
            for _ in 0..4 {
                let Some(u) = self.unit(h) else { break };
                if u.skill_points <= 0 {
                    break;
                }
                let learned: i32 = u.abilities.iter().filter(|(a, _)| self.objects.abilities.get(*a).is_some_and(|o| o.int("hero") != 0)).map(|a| a.1).sum();
                let ctx = (id_to_int(u.type_id), learned + 1);
                if let Some(s) = self.ai.get_mut(&p) {
                    s.hero_context = ctx;
                }
                let mut t = aivm.vm.new_thread(f, &[], false);
                let Outcome::Finished(Value::Int(a)) = aivm.vm.run(self, &mut t) else { break };
                if !self.learn_skill(h, id_from_int(a)) {
                    break;
                }
            }
        }
    }

    fn ai_state(&mut self) -> Option<&mut AiState> {
        let p = self.ai_current?;
        self.ai.get_mut(&p)
    }

    fn ai_player(&self) -> u8 {
        self.ai_current.unwrap_or(0)
    }

    /// Living units of the AI player, optionally of one type.
    fn ai_units(&self, p: u8) -> Vec<u32> {
        self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.owner == p && !u.dead)).collect()
    }

    /// Town halls of a player in creation order; town 0 is the first.
    pub fn towns(&self, p: u8) -> Vec<(f32, f32, u32)> {
        let mut out: Vec<(f32, f32, u32)> = self
            .ai_units(p)
            .into_iter()
            .filter(|&h| self.ability_with_code(h, "Artn").is_some_and(|(a, l)| self.objects.abilities.get(a).is_some_and(|o| o.level_real("DataA", l) != 0.0)))
            .filter_map(|h| self.unit(h).map(|u| (u.x, u.y, h)))
            .collect();
        if out.is_empty() {
            let s = self.start_of(p);
            out.push((s.0, s.1, 0));
        }
        out
    }

    fn town_pos(&self, p: u8, town: i32) -> (f32, f32) {
        let t = self.towns(p);
        let t = t.get(town.max(0) as usize).or(t.first()).copied().unwrap_or_default();
        (t.0, t.1)
    }

    fn mine_near(&self, x: f32, y: f32) -> Option<u32> {
        self.unit_order
            .iter()
            .copied()
            .filter(|&h| self.ability_with_code(h, "Agld").is_some() && self.unit(h).is_some_and(|m| !m.dead))
            .filter_map(|h| self.unit(h).map(|m| ((m.x - x).hypot(m.y - y), h)))
            .filter(|(d, _)| *d < TOWN_MINE_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }

    /// Units of a type the player has or is producing.
    pub fn unit_count(&self, p: u8, id: Id, done: bool, near: Option<(f32, f32)>) -> i32 {
        let mut n = 0;
        for h in self.ai_units(p) {
            let u = self.unit(h).unwrap();
            if let Some((x, y)) = near {
                if (u.x - x).hypot(u.y - y) > TOWN_MINE_RADIUS {
                    continue;
                }
            }
            if self.counts_as(u.type_id, id) && (!done || u.constructing.is_none() && !u.queue.contains(&u.type_id)) {
                n += 1;
            }
            if !done {
                n += u.queue.iter().filter(|q| **q == id).count() as i32;
                // Workers on their way to build count as the structure.
                if matches!(u.action, Action::Build { id: b, .. } if b == id) {
                    n += 1;
                }
            }
        }
        n
    }

    /// Whether a unit of type `have` counts as `want`: the same type, or a structure
    /// upgraded from it (a Stronghold counts as a Great Hall).
    fn counts_as(&self, have: Id, want: Id) -> bool {
        let mut cur = vec![want];
        for _ in 0..4 {
            if cur.contains(&have) {
                return true;
            }
            cur = cur
                .iter()
                .filter_map(|t| self.objects.units.get(*t))
                .flat_map(|o| o.list("Upgrade").iter().map(|s| id_from_str(s.trim())).collect::<Vec<_>>())
                .collect();
            if cur.is_empty() {
                return false;
            }
        }
        false
    }

    fn workers(&self, p: u8) -> Vec<u32> {
        self.ai_units(p).into_iter().filter(|&h| self.ability_with_code(h, "Ahar").is_some() || self.ability_with_code(h, "Ahrl").is_some()).collect()
    }

    /// Finds a spot for a structure near a town, keeping clear of the path to the mine.
    fn find_build_site(&mut self, p: u8, id: Id, town: i32) -> Option<(f32, f32)> {
        let (cx, cy) = self.town_pos(p, town);
        let mine = self.mine_near(cx, cy).and_then(|m| self.unit(m).map(|u| (u.x, u.y)));
        for ring in 3..24 {
            let r = ring as f32 * 64.0;
            let steps = ring * 8;
            for k in 0..steps {
                let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                let (x, y) = self.snap_placement(cx + r * a.cos(), cy + r * a.sin());
                if let Some((mx, my)) = mine {
                    // Stay off the line between the hall and its mine.
                    let (dx, dy) = (mx - cx, my - cy);
                    let len2 = (dx * dx + dy * dy).max(1.0);
                    let t = (((x - cx) * dx + (y - cy) * dy) / len2).clamp(0.0, 1.0);
                    let (px, py) = (cx + dx * t, cy + dy * t);
                    if (x - px).hypot(y - py) < 384.0 {
                        continue;
                    }
                }
                if self.can_place(id, x, y) && self.can_place_with_margin(id, x, y) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn can_place_with_margin(&mut self, id: Id, x: f32, y: f32) -> bool {
        // Leave a path around the structure.
        [(64.0, 0.0), (-64.0, 0.0), (0.0, 64.0), (0.0, -64.0)].iter().all(|&(dx, dy)| self.can_place(id, x + dx, y + dy))
    }

    /// Starts producing one unit, structure, or hero; returns whether it started.
    pub fn ai_produce(&mut self, p: u8, id: Id, town: i32) -> bool {
        let Some(o) = self.objects.units.get(id) else { return false };
        // Town hall tiers and other structure upgrades transform an existing structure.
        let id_s = crate::objects::id_str(id);
        let upgradable = self.ai_units(p).into_iter().find(|&h| {
            self.unit(h).is_some_and(|u| u.is_building && u.constructing.is_none() && u.queue.is_empty())
                && self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|bo| bo.list("Upgrade").iter().any(|x| x.trim() == id_s))
        });
        if let Some(b) = upgradable {
            return self.upgrade_structure(b, id).is_ok();
        }
        if o.bool("isbldg") {
            let (g, l, _) = self.cost(id);
            let pl = &self.players[p as usize];
            if pl.get_state(player_state::RESOURCE_GOLD) < g || pl.get_state(player_state::RESOURCE_LUMBER) < l {
                return false;
            }
            // A worker that can build it, preferring one not carrying resources.
            let id_s = crate::objects::id_str(id);
            let mut builders: Vec<u32> = self
                .workers(p)
                .into_iter()
                .filter(|&h| self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|wo| wo.list("Builds").iter().any(|b| b.trim() == id_s)))
                .filter(|&h| self.unit(h).is_some_and(|u| !matches!(u.action, Action::Build { .. } | Action::Construct { .. })))
                .collect();
            builders.sort_by_key(|&h| self.unit(h).map_or(0, |u| u.carry_gold + u.carry_lumber));
            let Some(&b) = builders.first() else { return false };
            let Some((x, y)) = self.find_build_site(p, id, town) else { return false };
            self.leave_mine_pub(b);
            if let Some(u) = self.unit_mut(b) {
                u.action = Action::Build { id, x, y };
                u.path.clear();
            }
            return true;
        }
        // Units and heroes train at the least busy structure that offers them.
        let id_s = crate::objects::id_str(id);
        let mut producers: Vec<u32> = self
            .ai_units(p)
            .into_iter()
            .filter(|&h| self.unit(h).is_some_and(|u| u.is_building && u.constructing.is_none()))
            .filter(|&h| self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|bo| bo.list("Trains").iter().any(|t| t.trim() == id_s)))
            .collect();
        producers.sort_by_key(|&h| self.unit(h).map_or(0, |u| u.queue.len()));
        let Some(&b) = producers.first() else { return false };
        if self.unit(b).is_some_and(|u| !u.queue.is_empty()) {
            return false;
        }
        self.train(b, id).is_ok()
    }

    /// Engine-side behavior: keeping workers on gold and wood, and moving captains.
    fn ai_engine(&mut self, p: u8, dt: f32) {
        let Some(st) = self.ai.get_mut(&p) else { return };
        st.harvest_timer -= dt;
        if st.harvest_timer > 0.0 {
            return;
        }
        st.harvest_timer = 1.0;
        let requests = st.harvest.clone();
        let workers = self.workers(p);
        let busy = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Build { .. } | Action::Construct { .. } | Action::Attack { .. } | Action::Move { .. }));
        let on_gold = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Harvest { target } if w.ability_with_code(target, "Agld").is_some()) || (matches!(u.action, Action::ReturnResources) && u.carry_gold > 0));
        let on_wood = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Harvest { target } if w.ability_with_code(target, "Agld").is_none()) || (matches!(u.action, Action::ReturnResources) && u.carry_lumber > 0));
        // Split the available workers among the requests in priority order.
        let available = workers.iter().filter(|&&h| !busy(self, h)).count() as i32;
        let (mut gold_target, mut wood_target, mut left) = (0, 0, available);
        let mut gold_town = 0;
        for (gold, town, n) in &requests {
            let take = (*n).min(left).max(0);
            left -= take;
            if *gold {
                gold_target += take;
                gold_town = *town;
            } else {
                wood_target += take;
            }
        }
        let mut gold_now: Vec<u32> = workers.iter().copied().filter(|&h| on_gold(self, h)).collect();
        let mut wood_now: Vec<u32> = workers.iter().copied().filter(|&h| on_wood(self, h)).collect();
        let mut idle: Vec<u32> = workers.iter().copied().filter(|&h| !busy(self, h) && !on_gold(self, h) && !on_wood(self, h)).collect();
        // Release extras, empty-handed workers first.
        while gold_now.len() as i32 > gold_target {
            idle.push(gold_now.pop().unwrap());
        }
        while wood_now.len() as i32 > wood_target {
            idle.push(wood_now.pop().unwrap());
        }
        let harvest = crate::natives::basic::order_id("harvest");
        let (tx, ty) = self.town_pos(p, gold_town);
        if let Some(mine) = self.mine_near(tx, ty) {
            while (gold_now.len() as i32) < gold_target {
                let Some(h) = idle.pop() else { break };
                self.order_unit(h, harvest, 0.0, 0.0, mine);
                gold_now.push(h);
            }
        }
        let (tx, ty) = self.town_pos(p, 0);
        while (wood_now.len() as i32) < wood_target {
            let Some(h) = idle.pop() else { break };
            let tree = self
                .destructable_order
                .iter()
                .copied()
                .filter(|&d| self.can_harvest(h, d))
                .filter_map(|d| self.widget_pos(d).map(|q| ((q.0 - tx).hypot(q.1 - ty), d)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|x| x.1);
            let Some(t) = tree else { break };
            self.order_unit(h, harvest, 0.0, 0.0, t);
            wood_now.push(h);
        }
        // Captains keep their units moving toward the goal.
        for which in 0..2 {
            let Some(st) = self.ai.get_mut(&p) else { return };
            let cap = if which == 0 { &mut st.attack } else { &mut st.defense };
            cap.units.retain(|_| true);
            let units = cap.units.clone();
            let goal = cap.goal;
            let target = cap.target;
            let units: Vec<u32> = units.into_iter().filter(|&h| self.unit(h).is_some_and(|u| !u.dead)).collect();
            if let Some(st) = self.ai.get_mut(&p) {
                let cap = if which == 0 { &mut st.attack } else { &mut st.defense };
                cap.units = units.clone();
            }
            let attack = crate::natives::basic::order_id("attack");
            for h in units {
                let idle = self.unit(h).is_some_and(|u| matches!(u.action, Action::Idle));
                if !idle {
                    continue;
                }
                if target != 0 && self.unit(target).is_some_and(|t| !t.dead) {
                    self.order_unit(h, attack, 0.0, 0.0, target);
                } else if let Some((x, y)) = goal {
                    if self.unit(h).is_some_and(|u| (u.x - x).hypot(u.y - y) > 300.0) {
                        self.order_unit(h, attack, x, y, 0);
                    }
                }
            }
        }
    }

    pub fn leave_mine_pub(&mut self, h: u32) {
        let mine = self.unit(h).map_or(0, |u| u.inside);
        if mine == 0 {
            return;
        }
        if let Some(m) = self.unit_mut(mine) {
            m.inside = m.inside.saturating_sub(1);
        }
        if let Some(u) = self.unit_mut(h) {
            u.inside = 0;
            u.hidden = false;
        }
    }

    fn captain(&mut self, attack: bool) -> Option<&mut Captain> {
        let st = self.ai_state()?;
        Some(if attack { &mut st.attack } else { &mut st.defense })
    }

    /// Adds units of a type to a captain until it has `qty` of them.
    fn fill_captain(&mut self, attack: bool, id: Id, qty: i32) -> bool {
        let p = self.ai_player();
        let have: Vec<u32> = self.captain(attack).map(|c| c.units.clone()).unwrap_or_default();
        let mut count = have.iter().filter(|&&h| self.unit(h).is_some_and(|u| u.type_id == id && !u.dead)).count() as i32;
        let others: Vec<u32> = self.captain(!attack).map(|c| c.units.clone()).unwrap_or_default();
        for h in self.ai_units(p) {
            if count >= qty {
                break;
            }
            let Some(u) = self.unit(h) else { continue };
            if u.type_id != id || u.constructing.is_some() || have.contains(&h) || others.contains(&h) {
                continue;
            }
            if let Some(c) = self.captain(attack) {
                c.units.push(h);
            }
            count += 1;
        }
        count >= qty
    }

    fn captain_units(&mut self, attack: bool) -> Vec<u32> {
        self.captain(attack).map(|c| c.units.clone()).unwrap_or_default()
    }

    fn enemy_town_hall(&self, p: u8) -> Option<u32> {
        let (sx, sy) = self.start_of(p);
        self.unit_order
            .iter()
            .copied()
            .filter_map(|h| self.unit(h).map(|u| (h, u)))
            .filter(|(_, u)| !u.dead && u.owner < PLAYER_NEUTRAL_AGGRESSIVE && !crate::natives::players::is_ally(self, p, u.owner))
            .filter(|(h, _)| self.ability_with_code(*h, "Artn").is_some())
            .map(|(h, u)| ((u.x - sx).hypot(u.y - sy), h))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }
}

fn captain_flag(attack: bool) -> bool {
    attack
}

fn upgrade_cost(w: &World, id: Id, gold: bool) -> i32 {
    let p = w.ai_player();
    let level = w.players[p as usize].tech_researched.get(&id).copied().unwrap_or(0);
    w.objects.upgrades.get(id).map_or(0, |o| {
        let (base, step) = if gold { ("goldbase", "goldmod") } else { ("lumberbase", "lumbermod") };
        o.int(base) + o.int(step) * level
    })
}

fn set_flag(w: &mut World, key: &'static str, v: bool) -> NativeResult {
    if let Some(s) = w.ai_state() {
        s.flags.insert(key.to_string(), v);
    }
    none()
}

pub fn table_ai() -> HashMap<&'static str, NativeFn> {
    let mut t: HashMap<&'static str, NativeFn> = HashMap::new();
    reg!(t,
        DebugS => |_, _, a| { log::trace!("AI: {}", a.s(0)); none() },
        DebugFI => |_, _, a| { log::trace!("AI: {} {}", a.s(0), a.i(1)); none() },
        DebugUnitID => |_, _, a| { log::trace!("AI: {} {}", a.s(0), crate::objects::id_str(id_from_int(a.i(1)))); none() },
        DisplayText => |_, _, a| { log::trace!("AI: {}", a.s(1)); none() },
        DisplayTextI => |_, _, a| { log::trace!("AI: {} {}", a.s(1), a.i(2)); none() },
        DisplayTextII => |_, _, a| { log::trace!("AI: {} {} {}", a.s(1), a.i(2), a.i(3)); none() },
        DisplayTextIII => |_, _, a| { log::trace!("AI: {} {} {} {}", a.s(1), a.i(2), a.i(3), a.i(4)); none() },
        DoAiScriptDebug => |_, _, _| boolean(false),
        DebugBreak => |_, _, _| none(),
        GetAiPlayer => |w, _, _| int(w.ai_player() as i32),
        GetHeroId => |w, _, _| int(w.ai_state().map_or(0, |s| s.hero_context.0)),
        GetHeroLevelAI => |w, _, _| int(w.ai_state().map_or(0, |s| s.hero_context.1)),
        GetUnitCount => |w, _, a| { let p = w.ai_player(); int(w.unit_count(p, id_from_int(a.i(0)), false, None)) },
        GetUnitCountDone => |w, _, a| { let p = w.ai_player(); int(w.unit_count(p, id_from_int(a.i(0)), true, None)) },
        GetPlayerUnitTypeCount => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return int(0) };
            int(w.unit_count(p, id_from_int(a.i(1)), false, None))
        },
        GetTownUnitCount => |w, _, a| {
            let p = w.ai_player();
            let pos = w.town_pos(p, a.i(1));
            int(w.unit_count(p, id_from_int(a.i(0)), a.b(2), Some(pos)))
        },
        GetUnitGoldCost => |w, _, a| { let id = id_from_int(a.i(0)); int(w.objects.units.get(id).map_or(0, |o| o.int("goldcost"))) },
        GetUnitWoodCost => |w, _, a| { let id = id_from_int(a.i(0)); int(w.objects.units.get(id).map_or(0, |o| o.int("lumbercost"))) },
        GetUnitBuildTime => |w, _, a| { let id = id_from_int(a.i(0)); int(w.objects.units.get(id).map_or(0, |o| o.int("bldtm"))) },
        GetMinesOwned => |w, _, _| {
            let p = w.ai_player();
            let towns = w.towns(p);
            int(towns.iter().filter(|t| t.2 != 0 && w.mine_near(t.0, t.1).is_some()).count() as i32)
        },
        GetGoldOwned => |w, _, _| {
            let p = w.ai_player();
            let towns = w.towns(p);
            int(towns.iter().filter(|t| t.2 != 0).filter_map(|t| w.mine_near(t.0, t.1)).filter_map(|m| w.unit(m)).map(|m| m.resource).sum())
        },
        TownWithMine => |w, _, _| {
            let p = w.ai_player();
            let towns = w.towns(p);
            int(towns.iter().position(|t| t.2 != 0 && w.mine_near(t.0, t.1).is_some()).map_or(-1, |i| i as i32))
        },
        TownHasMine => |w, _, a| {
            let p = w.ai_player();
            let t = w.towns(p);
            boolean(t.get(a.i(0).max(0) as usize).is_some_and(|t| w.mine_near(t.0, t.1).is_some()))
        },
        TownHasHall => |w, _, a| {
            let p = w.ai_player();
            let t = w.towns(p);
            boolean(t.get(a.i(0).max(0) as usize).is_some_and(|t| t.2 != 0))
        },
        GetUpgradeLevel => |w, _, a| {
            let p = w.ai_player();
            int(w.players[p as usize].tech_researched.get(&id_from_int(a.i(0))).copied().unwrap_or(0))
        },
        GetUpgradeGoldCost => |w, _, a| int(upgrade_cost(w, id_from_int(a.i(0)), true)),
        GetUpgradeWoodCost => |w, _, a| int(upgrade_cost(w, id_from_int(a.i(0)), false)),
        GetNextExpansion => |_, _, _| int(-1),
        GetMegaTarget => |_, _, _| none(),
        GetBuilding => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            handle(w.unit_order.iter().copied().find(|&h| w.unit(h).is_some_and(|u| u.owner == p && u.is_building && !u.dead)).unwrap_or(0))
        },
        GetEnemyPower => |_, _, _| int(0),
        SetAllianceTarget => |_, _, _| none(),
        GetAllianceTarget => |_, _, _| none(),
        SetProduce => |w, _, a| {
            let p = w.ai_player();
            let id = id_from_int(a.i(1));
            let mut ok = true;
            for _ in 0..a.i(0).max(0) {
                if !w.ai_produce(p, id, a.i(2)) {
                    ok = false;
                    break;
                }
            }
            boolean(ok)
        },
        Unsummon => |_, _, _| none(),
        SetExpansion => |_, _, _| boolean(false),
        SetUpgrade => |w, _, a| {
            let p = w.ai_player();
            let id = id_from_int(a.i(0));
            boolean(w.research(p, id))
        },
        SetHeroLevels => |w, _, a| { let f = a.code(0); if let Some(s) = w.ai_state() { s.hero_levels = f; } none() },
        SetNewHeroes => |w, _, a| set_flag(w, "new_heroes", a.b(0)),
        PurchaseZeppelin => |_, _, _| none(),
        MergeUnits => |_, _, _| boolean(false),
        ConvertUnits => |_, _, _| boolean(false),
        SetCampaignAI => |w, _, _| { if let Some(s) = w.ai_state() { s.melee = false; } none() },
        SetMeleeAI => |w, _, _| { if let Some(s) = w.ai_state() { s.melee = true; } none() },
        SetTargetHeroes => |w, _, a| set_flag(w, "target_heroes", a.b(0)),
        SetPeonsRepair => |w, _, a| set_flag(w, "peons_repair", a.b(0)),
        SetRandomPaths => |w, _, a| set_flag(w, "random_paths", a.b(0)),
        SetDefendPlayer => |w, _, a| set_flag(w, "defend_player", a.b(0)),
        SetHeroesFlee => |w, _, a| set_flag(w, "heroes_flee", a.b(0)),
        SetHeroesBuyItems => |w, _, a| set_flag(w, "heroes_buy_items", a.b(0)),
        SetWatchMegaTargets => |w, _, a| set_flag(w, "watch_mega_targets", a.b(0)),
        SetIgnoreInjured => |w, _, a| set_flag(w, "ignore_injured", a.b(0)),
        SetHeroesTakeItems => |w, _, a| set_flag(w, "heroes_take_items", a.b(0)),
        SetUnitsFlee => |w, _, a| set_flag(w, "units_flee", a.b(0)),
        SetGroupsFlee => |w, _, a| set_flag(w, "groups_flee", a.b(0)),
        SetSlowChopping => |w, _, a| set_flag(w, "slow_chopping", a.b(0)),
        SetCaptainChanges => |w, _, a| set_flag(w, "captain_changes", a.b(0)),
        SetSmartArtillery => |w, _, a| set_flag(w, "smart_artillery", a.b(0)),
        SetReplacementCount => |_, _, _| none(),
        GroupTimedLife => |_, _, _| none(),
        RemoveInjuries => |_, _, _| none(),
        RemoveSiege => |_, _, _| none(),
        InitAssault => |w, _, _| { if let Some(s) = w.ai_state() { s.assault.clear(); } none() },
        AddAssault => |w, _, a| {
            let id = id_from_int(a.i(1));
            if let Some(s) = w.ai_state() { s.assault.push((id, a.i(0))); }
            boolean(w.fill_captain(captain_flag(true), id, a.i(0)))
        },
        AddDefenders => |w, _, a| {
            let id = id_from_int(a.i(1));
            if let Some(s) = w.ai_state() { s.defenders.push((id, a.i(0))); }
            boolean(w.fill_captain(false, id, a.i(0)))
        },
        GetCreepCamp => |w, _, a| {
            // The closest hostile neutral whose level fits the range.
            let p = w.ai_player();
            let (sx, sy) = w.start_of(p);
            let (lo, hi) = (a.i(0), a.i(1));
            let best = w.unit_order.iter().copied()
                .filter_map(|h| w.unit(h).map(|u| (h, u)))
                .filter(|(_, u)| u.owner == PLAYER_NEUTRAL_AGGRESSIVE && !u.dead && (lo..=hi).contains(&u.level))
                .map(|(h, u)| ((u.x - sx).hypot(u.y - sy), h))
                .min_by(|a, b| a.0.total_cmp(&b.0)).map(|x| x.1);
            handle(best.unwrap_or(0))
        },
        StartGetEnemyBase => |w, _, _| {
            let p = w.ai_player();
            let e = w.enemy_town_hall(p).unwrap_or(0);
            if let Some(s) = w.ai_state() { s.enemy_base = e; }
            none()
        },
        WaitGetEnemyBase => |_, _, _| boolean(false),
        GetEnemyBase => |w, _, _| handle(w.ai_state().map_or(0, |s| s.enemy_base)),
        GetExpansionFoe => |_, _, _| none(),
        GetEnemyExpansion => |_, _, _| none(),
        GetExpansionX => |_, _, _| int(0),
        GetExpansionY => |_, _, _| int(0),
        GetExpansionPeon => |_, _, _| none(),
        SetStagePoint => |w, _, a| { let p = (a.r(0), a.r(1)); if let Some(s) = w.ai_state() { s.stage = Some(p); } none() },
        AttackMoveKill => |w, _, a| {
            let t = a.h(0);
            if let Some(c) = w.captain(true) { c.target = t; c.retreating = false; }
            none()
        },
        AttackMoveXY => |w, _, a| {
            let p = (a.i(0) as f32, a.i(1) as f32);
            if let Some(c) = w.captain(true) { c.goal = Some(p); c.target = 0; c.retreating = false; }
            none()
        },
        LoadZepWave => |_, _, _| none(),
        SuicidePlayer => |_, _, _| boolean(false),
        SuicidePlayerUnits => |_, _, _| boolean(false),
        CaptainInCombat => |w, _, a| {
            let units = w.captain_units(a.b(0));
            boolean(units.iter().any(|&h| w.unit(h).is_some_and(|u| u.swing.is_some() || matches!(u.action, Action::Attack { .. }))))
        },
        IsTowered => |_, _, _| boolean(false),
        ClearHarvestAI => |w, _, _| { if let Some(s) = w.ai_state() { s.harvest.clear(); } none() },
        HarvestGold => |w, _, a| { let (t, n) = (a.i(0), a.i(1)); if let Some(s) = w.ai_state() { s.harvest.push((true, t, n)); } none() },
        HarvestWood => |w, _, a| { let (t, n) = (a.i(0), a.i(1)); if let Some(s) = w.ai_state() { s.harvest.push((false, t, n)); } none() },
        StopGathering => |_, _, _| none(),
        AddGuardPost => |w, _, a| { let g = (id_from_int(a.i(0)), a.r(1), a.r(2)); if let Some(s) = w.ai_state() { s.guard_posts.push(g); } none() },
        FillGuardPosts => |_, _, _| none(),
        ReturnGuardPosts => |_, _, _| none(),
        CreateCaptains => |w, _, _| {
            let p = w.ai_player();
            let home = w.start_of(p);
            if let Some(s) = w.ai_state() {
                s.attack = Captain { home, ..Default::default() };
                s.defense = Captain { home, ..Default::default() };
            }
            none()
        },
        SetCaptainHome => |w, _, a| {
            let home = (a.r(1), a.r(2));
            // ATTACK_CAPTAIN = 1, DEFENSE_CAPTAIN = 2, BOTH_CAPTAINS = 3 in common.ai.
            let which = a.i(0);
            if let Some(s) = w.ai_state() {
                if which & 1 != 0 { s.attack.home = home; }
                if which & 2 != 0 { s.defense.home = home; }
            }
            none()
        },
        ResetCaptainLocs => |_, _, _| none(),
        ShiftTownSpot => |_, _, _| none(),
        TeleportCaptain => |_, _, _| none(),
        ClearCaptainTargets => |w, _, _| { if let Some(c) = w.captain(true) { c.goal = None; c.target = 0; } none() },
        CaptainAttack => |w, _, a| {
            let p = (a.r(0), a.r(1));
            if let Some(c) = w.captain(true) { c.goal = Some(p); c.target = 0; c.retreating = false; }
            none()
        },
        CaptainVsUnits => |w, _, a| {
            let Some(e) = w.player_of(a.h(0)) else { return none() };
            let units = w.captain_units(true);
            let (cx, cy) = units.first().and_then(|&h| w.unit(h)).map_or((0.0, 0.0), |u| (u.x, u.y));
            let t = w.unit_order.iter().copied().filter_map(|h| w.unit(h).map(|u| (h, u)))
                .filter(|(_, u)| u.owner == e && !u.dead)
                .map(|(h, u)| ((u.x - cx).hypot(u.y - cy), h))
                .min_by(|a, b| a.0.total_cmp(&b.0)).map(|x| x.1).unwrap_or(0);
            if let Some(c) = w.captain(true) { c.target = t; }
            none()
        },
        CaptainVsPlayer => |w, _, a| {
            let Some(e) = w.player_of(a.h(0)) else { return none() };
            let t = w.unit_order.iter().copied().find(|&h| w.unit(h).is_some_and(|u| u.owner == e && !u.dead && u.is_building)).unwrap_or(0);
            if let Some(c) = w.captain(true) { c.target = t; }
            none()
        },
        CaptainGoHome => |w, _, _| {
            if let Some(c) = w.captain(true) { c.goal = Some(c.home); c.target = 0; c.retreating = true; }
            none()
        },
        CaptainIsHome => |w, _, _| {
            let units = w.captain_units(true);
            let home = w.captain(true).map_or((0.0, 0.0), |c| c.home);
            boolean(units.iter().all(|&h| w.unit(h).is_none_or(|u| (u.x - home.0).hypot(u.y - home.1) < 800.0)))
        },
        CaptainIsFull => |w, _, _| {
            let want = w.ai_state().map(|s| s.assault.clone()).unwrap_or_default();
            let units = w.captain_units(true);
            boolean(want.iter().all(|(id, qty)| units.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.type_id == *id && !u.dead)).count() as i32 >= *qty))
        },
        CaptainIsEmpty => |w, _, _| { let u = w.captain_units(true); boolean(u.iter().all(|&h| w.unit(h).is_none_or(|u| u.dead))) },
        CaptainGroupSize => |w, _, _| { let u = w.captain_units(true); int(u.iter().filter(|&&h| w.unit(h).is_some_and(|u| !u.dead)).count() as i32) },
        CaptainReadiness => |w, _, _| {
            let u = w.captain_units(true);
            let (life, max) = u.iter().filter_map(|&h| w.unit(h)).fold((0.0, 0.0), |a, u| (a.0 + u.life, a.1 + u.max_life));
            int(if max > 0.0 { (life / max * 100.0) as i32 } else { 0 })
        },
        CaptainReadinessHP => |w, _, _| {
            let u = w.captain_units(true);
            let (life, max) = u.iter().filter_map(|&h| w.unit(h)).fold((0.0, 0.0), |a, u| (a.0 + u.life, a.1 + u.max_life));
            int(if max > 0.0 { (life / max * 100.0) as i32 } else { 0 })
        },
        CaptainReadinessMa => |w, _, _| {
            let u = w.captain_units(true);
            let (m, max) = u.iter().filter_map(|&h| w.unit(h)).fold((0.0, 0.0), |a, u| (a.0 + u.mana, a.1 + u.max_mana));
            int(if max > 0.0 { (m / max * 100.0) as i32 } else { 100 })
        },
        CaptainRetreating => |w, _, _| boolean(w.captain(true).is_some_and(|c| c.retreating)),
        CaptainAtGoal => |w, _, _| {
            let units = w.captain_units(true);
            let goal = w.captain(true).and_then(|c| c.goal);
            let Some((gx, gy)) = goal else { return boolean(true) };
            boolean(units.iter().filter_map(|&h| w.unit(h)).any(|u| (u.x - gx).hypot(u.y - gy) < 500.0))
        },
        CreepsOnMap => |w, _, _| boolean(w.unit_order.iter().any(|&h| w.unit(h).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE && !u.dead))),
        SuicideUnit => |_, _, _| none(),
        SuicideUnitEx => |_, _, _| none(),
        StartThread => |w, vm, a| {
            if let Some(f) = a.code(0) {
                let t = vm.new_thread(f, &[], true);
                if let Some(s) = w.ai_state() { s.spawned.push(t); }
            }
            none()
        },
        Sleep => |_, _, a| NativeResult::Sleep(a.r(0)),
        UnitAlive => |w, _, a| boolean(w.unit(a.h(0)).is_some_and(|u| !u.dead)),
        UnitInvis => |_, _, _| boolean(false),
        IgnoredUnits => |_, _, _| int(0),
        TownThreatened => |_, _, _| boolean(false),
        DisablePathing => |_, _, _| none(),
        SetAmphibious => |_, _, _| none(),
        CommandsWaiting => |w, _, _| int(w.ai_state().map_or(0, |s| s.commands.len() as i32)),
        GetLastCommand => |w, _, _| int(w.ai_state().and_then(|s| s.commands.back().map(|c| c.0)).unwrap_or(0)),
        GetLastData => |w, _, _| int(w.ai_state().and_then(|s| s.commands.back().map(|c| c.1)).unwrap_or(0)),
        PopLastCommand => |w, _, _| { if let Some(s) = w.ai_state() { s.commands.pop_back(); } none() },
        // ai_MeleeDifficulty (game.dll 0x6f9bcf70): MELEE_NEWBIE, MELEE_NORMAL, MELEE_INSANE.
        MeleeDifficulty => |w, _, _| int(match w.ai_difficulty.get(&w.ai_player()).copied().unwrap_or(1) {
            0 => 1,
            2 => 3,
            _ => 2,
        }),
    );
    let _ = (id_from_str, id_to_int, Value::Null);
    t
}
