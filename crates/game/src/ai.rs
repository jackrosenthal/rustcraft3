//! Computer players: each runs its race script (with common.ai) in its own JASS VM, and the
//! engine carries out what the script asks for through the AI natives (production,
//! harvesting, towns, and the attack and defense "captains").

use std::collections::VecDeque;
use crate::collections::HashMap;

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
    #[serde(default)]
    pub posts: Vec<crate::aiposts::GuardPost>,
    /// How many times a guard post is filled (SetReplacementCount; CCompAI +0x1d4).
    #[serde(default)]
    pub replacements: i32,
    /// The enemy threatening the town (CCompAI +0x164; TownThreatened), whether an
    /// attack may be answered now (flag 2 of +0x2d0), and the 2 s check that reopens it.
    #[serde(default)]
    pub threat: u32,
    #[serde(default)]
    pub threat_open: bool,
    #[serde(default)]
    pub threat_timer: f32,
    /// The gold mine of the next expansion and until when that choice stands
    /// (FUN_6f9bb210 keeps it 10 s).
    #[serde(default)]
    pub expansion: Option<u32>,
    #[serde(default)]
    pub expansion_until: f32,
    /// The mega target (CCompAI +0x2c4) and the 10 s check that picks it.
    #[serde(default)]
    pub mega: u32,
    #[serde(default)]
    pub mega_timer: f32,
    /// Whether the assault group filled every request since InitAssault (the attack
    /// captain's flag 1).
    #[serde(default)]
    pub assault_full: bool,
    /// The player the attack captain is sent against (SuicidePlayer), and whether its
    /// structures count.
    #[serde(default)]
    pub attack_player: Option<(u8, bool)>,
    /// The target allied computers share (SetAllianceTarget; CCompAI +0x2a0).
    #[serde(default)]
    pub alliance_target: u32,
    /// The main town's center as ShiftTownSpot set it (FUN_6f9cbf00).
    #[serde(default)]
    pub town_shift: Option<(f32, f32)>,
    /// PauseCompAI's count (CCompAI +0x1d8): the AI stands still while above 0.
    #[serde(default)]
    pub paused: i32,
}

/// Distance within which a town hall and a gold mine belong to the same town.
const TOWN_MINE_RADIUS: f32 = 1500.0;

impl World {
    /// Places the hostile neutrals not yet in a camp (FUN_6f9b9230, FUN_6f9b7b30): each
    /// joins the camp whose first living member is nearest within
    /// CreepCampPathingCellDistance pathing cells (Misc) and has room for it (12), or
    /// starts its own.
    fn place_creeps(&mut self) {
        let cells = self.misc_f32("CreepCampPathingCellDistance", 26.0);
        let reach = cells * 32.0;
        let placed: crate::collections::HashSet<u32> = self.creep_camps.iter().flatten().copied().collect();
        let new: Vec<u32> = self.unit_order.iter().copied().filter(|h| !placed.contains(h)).filter(|&h| self.unit(h).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE && !u.dead && !self.is_summoned(h))).collect();
        for h in new {
            let Some((x, y)) = self.unit(h).map(|u| (u.x, u.y)) else { continue };
            let mut best: Option<(usize, f32)> = None;
            for (i, camp) in self.creep_camps.iter().enumerate() {
                if camp.len() >= 12 {
                    continue;
                }
                let Some(first) = camp.iter().filter_map(|&c| self.unit(c)).find(|u| !u.dead) else { continue };
                let d = (first.x - x).hypot(first.y - y);
                if d < reach && best.is_none_or(|b| d < b.1) {
                    best = Some((i, d));
                }
            }
            match best {
                Some((i, _)) => self.creep_camps[i].push(h),
                None => self.creep_camps.push(vec![h]),
            }
        }
    }

    /// The hostile neutrals grouped into camps as place_creeps groups them, from
    /// scratch and without keeping them (for the minimap): each camp's center and the
    /// sum of its living creeps' levels.
    pub fn creep_camp_markers(&self) -> Vec<(f32, f32, i32)> {
        let reach = self.misc_f32("CreepCampPathingCellDistance", 26.0) * 32.0;
        let mut camps: Vec<Vec<u32>> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h).filter(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE && !u.dead && !u.hidden && !self.is_summoned(h)) else { continue };
            let mut best: Option<(usize, f32)> = None;
            for (i, camp) in camps.iter().enumerate() {
                if camp.len() >= 12 {
                    continue;
                }
                let Some(first) = camp.first().and_then(|&c| self.unit(c)) else { continue };
                let d = (first.x - u.x).hypot(first.y - u.y);
                if d < reach && best.is_none_or(|b| d < b.1) {
                    best = Some((i, d));
                }
            }
            match best {
                Some((i, _)) => camps[i].push(h),
                None => camps.push(vec![h]),
            }
        }
        camps
            .iter()
            .map(|c| {
                let us: Vec<_> = c.iter().filter_map(|&h| self.unit(h)).collect();
                let n = us.len().max(1) as f32;
                let (x, y) = us.iter().fold((0.0, 0.0), |a, u| (a.0 + u.x / n, a.1 + u.y / n));
                let level = us.iter().map(|u| self.objects.units.get(u.type_id).map_or(u.level, |o| if u.is_hero { u.level } else { o.int("level") })).sum();
                (x, y, level)
            })
            .collect()
    }

    /// The length of the ground path from `from` to `to`; infinite when the path ends
    /// short of it (more than a cell and the unit's reach away).
    fn walk_length(&self, from: (f32, f32), to: (f32, f32), radius: f32) -> f32 {
        // Asked between cell centers, so a creep shifting within its cell asks the same.
        let (from, to) = match &self.pathing {
            Some(map) => (map.center(map.cell_of(from.0, from.1)), map.center(map.cell_of(to.0, to.1))),
            None => (from, to),
        };
        self.prof_time("walk_length", || self.ai_walk(0, from, to, radius, || self.walk_length_inner(from, to, radius)))
    }

    fn walk_length_inner(&self, from: (f32, f32), to: (f32, f32), radius: f32) -> f32 {
        let Some(map) = &self.pathing else { return (to.0 - from.0).hypot(to.1 - from.1) };
        // Measured between cell centers, so a creep shifting within its cell asks the
        // same question again.
        let (from, to) = (map.center(map.cell_of(from.0, from.1)), map.center(map.cell_of(to.0, to.1)));
        // What cannot be walked to at all is not searched for.
        if !map.reaches(from, to, radius, radius + 2.0 * crate::pathing::CELL) {
            return f32::INFINITY;
        }
        let mut at = from;
        let mut len = 0.0;
        for p in map.find_path_memo(from, to, radius, 200_000) {
            len += (p.0 - at.0).hypot(p.1 - at.1);
            at = p;
        }
        let left = (to.0 - at.0).hypot(to.1 - at.1);
        if left > radius + 2.0 * crate::pathing::CELL {
            return f32::INFINITY;
        }
        len + left
    }

    /// GetCreepCamp (FUN_6f9baa40): the nearest camp to the player's start whose
    /// strength (its creeps' levels, air units counted twice) lies within min..max and
    /// that has no air units unless they are allowed; a camp too strong (or with air
    /// units not allowed) rules out those more than 200 farther away.  Failing that, the
    /// nearest weaker camp.  Its first living creep.  A creep's strength is its level
    /// (FUN_6f685150; a Hero's plus one).
    pub(crate) fn creep_camp(&mut self, player: u8, min: i32, max: i32, flyers_ok: bool) -> u32 {
        self.place_creeps();
        let (sx, sy) = self.start_of(player);
        let air = |w: &World, u: &Unit| w.objects.units.get(u.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
        let mut limit = 999_999.0f32;
        let mut best: Option<(u32, f32)> = None;
        let mut weak: Option<(u32, f32)> = None;
        for camp in self.creep_camps.clone() {
            let living: Vec<&Unit> = camp.iter().filter_map(|&c| self.unit(c)).filter(|u| !u.dead).collect();
            let Some(first) = camp.iter().copied().find(|&c| self.unit(c).is_some_and(|u| !u.dead)) else { continue };
            // The length of the walk from the start to the camp's first living creep.
            let d = self.unit(first).map_or(f32::MAX, |u| self.walk_length((sx, sy), (u.x, u.y), u.collision));
            if d > limit {
                continue;
            }
            let strength: i32 = living.iter().map(|u| {
                let s = if u.is_hero { u.level + 1 } else { u.level };
                if air(self, u) { 2 * s } else { s }
            }).sum();
            let has_air = living.iter().any(|u| air(self, u));
            if max < strength || (!flyers_ok && has_air) {
                if d < limit - 200.0 {
                    limit = d + 200.0;
                }
                if best.is_some_and(|b| limit < b.1) {
                    best = None;
                }
                if weak.is_some_and(|b| limit < b.1) {
                    weak = None;
                }
            } else if strength < min {
                if weak.is_none_or(|b| d < b.1) {
                    weak = Some((first, d));
                }
            } else if best.is_none_or(|b| d < b.1) {
                best = Some((first, d));
            }
        }
        best.or(weak).map_or(0, |b| b.0)
    }

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
            if self.ai.get(&p).is_some_and(|s| s.paused > 0) {
                continue;
            }
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
                    Outcome::Aborted(e) => log::warn!("AI thread for player {p} ended: {e} at {:?}", t.backtrace(&aivm.vm.module)),
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
            self.ai_threat_tick(p, dt);
            self.ai_mega_tick(p, dt);
            self.ai_attack_player_tick(p);
            self.ai_suicide_tick(p);
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
            .filter(|&h| self.unit(h).is_some_and(|u| self.is_town_hall(u)))
            .filter_map(|h| self.unit(h).map(|u| (u.x, u.y, h)))
            .collect();
        if out.is_empty() {
            let s = self.start_of(p);
            out.push((s.0, s.1, 0));
        }
        out
    }

    pub(crate) fn town_pos_pub(&self, p: u8, town: i32) -> (f32, f32) {
        self.town_pos(p, town)
    }

    pub(crate) fn mine_near_pub(&self, x: f32, y: f32) -> Option<u32> {
        self.mine_near(x, y)
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
            .filter(|&h| self.is_gold_mine(h) && self.unit(h).is_some_and(|m| !m.dead && !m.hidden))
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

    /// Whether a unit of type `have` counts as `want`: the same type, a type its
    /// DependencyOr names (a Headhunter counts as a Berserker), or a structure upgraded
    /// from it (a Stronghold counts as a Great Hall).
    fn counts_as(&self, have: Id, want: Id) -> bool {
        if self.objects.units.get(have).is_some_and(|o| o.list("DependencyOr").iter().any(|d| id_from_str(d.trim()) == want)) {
            return true;
        }
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

    pub(crate) fn workers(&self, p: u8) -> Vec<u32> {
        self.ai_units(p).into_iter().filter(|&h| ["Ahar", "Ahrl", "Aaha", "Awha"].iter().any(|c| self.ability_with_code(h, c).is_some())).collect()
    }

    /// A Hero type first (FUN_6f9caff0): an idle altar of the player revives a dead
    /// Hero of that type; failing that, owning one (dead or alive) is enough.
    fn ai_revive(&mut self, vm: &mut Vm, p: u8, id: Id) -> bool {
        if !(b'A'..=b'Z').contains(&id[0]) {
            return false;
        }
        let units = self.ai_units(p);
        for altar in units {
            if self.unit(altar).is_some_and(|a| a.revive.is_some() || !a.queue.is_empty()) {
                continue;
            }
            if let Some(hero) = self.revivable(altar).into_iter().find(|&h| self.unit(h).is_some_and(|u| u.type_id == id)) {
                let _ = self.start_revive(vm, altar, hero);
                return true;
            }
        }
        self.unit_order.iter().any(|&h| self.unit(h).is_some_and(|u| u.owner == p && u.is_hero && u.type_id == id))
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
            let Some((x, y)) = self.ai_build_site(p, id, town, b) else { return false };
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
        // The order is given to an idle producer that lists the type, and that is the
        // answer, whether or not the order then goes through (FUN_6f9cb530).
        let _ = self.train(b, id);
        true
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
        // Units in a captain or at a guard post do not gather.
        let mut held: Vec<u32> = st.attack.units.iter().chain(st.defense.units.iter()).copied().collect();
        held.extend(self.guards(p));
        let workers: Vec<u32> = self.workers(p).into_iter().filter(|h| !held.contains(h)).collect();
        let busy = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Build { .. } | Action::Construct { .. } | Action::Attack { .. } | Action::Move { .. }));
        let on_gold = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Harvest { target } if w.is_gold_mine(target)) || (matches!(u.action, Action::ReturnResources) && u.carry_gold > 0));
        let on_wood = |w: &World, h: u32| w.unit(h).is_some_and(|u| matches!(u.action, Action::Harvest { target } if !w.is_gold_mine(target)) || (matches!(u.action, Action::ReturnResources) && u.carry_lumber > 0));
        // Each request in priority order takes workers that can do its work and are free,
        // those already at it first: an Undead's acolytes mine and its ghouls cut wood
        // (measured: the original's computer ghoul cuts wood from the start; ours, counted
        // against the gold request, stood idle for half a minute).
        let mut gold_town = 0;
        let (tx0, ty0) = self.town_pos(p, 0);
        let tree_near = |w: &World, h: u32| {
            w.destructable_order
                .iter()
                .copied()
                .filter(|&d| w.can_harvest(h, d))
                .filter_map(|d| w.widget_pos(d).map(|q| ((q.0 - tx0).hypot(q.1 - ty0), d)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|x| x.1)
        };
        for (gold, town, _) in &requests {
            if *gold {
                gold_town = *town;
            }
        }
        let (tx, ty) = self.town_pos(p, gold_town);
        let mine = self.mine_near(tx, ty);
        let free: Vec<u32> = workers.iter().copied().filter(|&h| !busy(self, h)).collect();
        let mut taken: Vec<u32> = Vec::new();
        let (mut to_gold, mut to_wood) = (Vec::new(), Vec::new());
        for (gold, _, n) in &requests {
            let can = |w: &World, h: u32| if *gold { mine.is_some_and(|m| w.can_harvest(h, m)) } else { tree_near(w, h).is_some() };
            let doing = |w: &World, h: u32| if *gold { on_gold(w, h) } else { on_wood(w, h) };
            let mut pool: Vec<u32> = free.iter().copied().filter(|h| !taken.contains(h) && can(self, *h)).collect();
            pool.sort_by_key(|&h| !doing(self, h));
            for h in pool.into_iter().take((*n).max(0) as usize) {
                taken.push(h);
                if *gold { to_gold.push(h) } else { to_wood.push(h) }
            }
        }
        let harvest = crate::natives::basic::order_id("harvest");
        if let Some(mine) = mine {
            for &h in &to_gold {
                if !on_gold(self, h) {
                    self.order_unit(h, harvest, 0.0, 0.0, mine);
                }
            }
        }
        for &h in &to_wood {
            if !on_wood(self, h) {
                if let Some(t) = tree_near(self, h) {
                    self.order_unit(h, harvest, 0.0, 0.0, t);
                }
            }
        }
        // Captains keep their units moving toward the goal.
        for which in 0..2 {
            let Some(st) = self.ai.get_mut(&p) else { return };
            let cap = if which == 0 { &mut st.attack } else { &mut st.defense };
            cap.units.retain(|_| true);
            let units = cap.units.clone();
            let goal = cap.goal;
            let mut target = cap.target;
            // A target that died (its handle may since name another unit) or that is
            // friendly is dropped.
            if target != 0 && self.unit(target).is_none_or(|t| t.dead || crate::natives::players::is_ally(self, p, t.owner)) {
                target = 0;
            }
            let units: Vec<u32> = units.into_iter().filter(|&h| self.unit(h).is_some_and(|u| !u.dead)).collect();
            if let Some(st) = self.ai.get_mut(&p) {
                let cap = if which == 0 { &mut st.attack } else { &mut st.defense };
                cap.units = units.clone();
                cap.target = target;
            }
            let attack = crate::natives::basic::order_id("attack");
            for h in units {
                // Idle units, and gatherers the captain has taken, go.
                let idle = self.unit(h).is_some_and(|u| matches!(u.action, Action::Idle | Action::Harvest { .. } | Action::ReturnResources));
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

    /// The 2 s threat check (FUN_6f9ccbd0): attacks may be answered again, and the
    /// threat ends once the defense captain is no longer fighting.
    fn ai_threat_tick(&mut self, p: u8, dt: f32) {
        let Some(st) = self.ai.get_mut(&p) else { return };
        st.threat_timer -= dt;
        if st.threat_timer > 0.0 {
            return;
        }
        st.threat_timer = 2.0;
        st.threat_open = true;
        let (threat, units) = (st.threat, st.defense.units.clone());
        if threat != 0 {
            let fighting = units.iter().any(|&h| self.unit(h).is_some_and(|u| !u.dead && (u.swing.is_some() || matches!(u.action, Action::Attack { .. }))));
            let gone = self.unit(threat).is_none_or(|u| u.dead);
            if !fighting || gone {
                if let Some(st) = self.ai.get_mut(&p) {
                    st.threat = 0;
                    if st.defense.target == threat {
                        st.defense.target = 0;
                    }
                }
            }
        }
    }

    /// An enemy hurt a computer player's unit (the CallForHelp handler FUN_6f66e9e0
    /// and FUN_6f9cbfc0), when it is a structure, a worker, or a guard (other units
    /// look after themselves).  A structure's attack is major when its life is under
    /// 60%; a guard's when the attacker's side has more than 8 strength within 1200 of
    /// it (a Hero (level + 1) / 2 + 4, any other its level); a worker's never.
    /// The attacker becomes the threat (a Hero before any other), the defense captain
    /// goes at it, and on a major attack the attack captain too (when it has more
    /// than 2 units and there is no mega target).  Allies' computers that defend
    /// their allies (flag 0x200000) answer attacks on a player's structures.
    pub(crate) fn ai_town_attacked(&mut self, victim: u32, attacker: u32) {
        let Some(v) = self.unit(victim) else { return };
        let Some(a) = self.unit(attacker) else { return };
        let (owner, enemy) = (v.owner, a.owner);
        let structure = v.is_building;
        let low = v.life < v.max_life * 0.6;
        let peon = self.objects.units.get(v.type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon")));
        let respond = |w: &mut World, p: u8, major: Option<bool>| {
            if crate::natives::players::is_ally(w, p, enemy) {
                return;
            }
            let Some(st) = w.ai.get_mut(&p) else { return };
            if !st.threat_open {
                return;
            }
            st.threat_open = false;
            // An attacker no nearer the defense captain than the threat is already
            // answered (FUN_6f9c8fd0).
            let threat = st.threat;
            let units = st.defense.units.clone();
            let home = st.defense.home;
            let living: Vec<(f32, f32)> = units.iter().filter_map(|&h| w.unit(h)).filter(|u| !u.dead).map(|u| (u.x, u.y)).collect();
            let at = if living.is_empty() { home } else { (living.iter().map(|p| p.0).sum::<f32>() / living.len() as f32, living.iter().map(|p| p.1).sum::<f32>() / living.len() as f32) };
            let dist = |h: u32| w.unit(h).map_or(f32::MAX, |u| (u.x - at.0).hypot(u.y - at.1));
            if threat != 0 && w.unit(threat).is_some_and(|u| !u.dead) && dist(threat) <= dist(attacker) {
                return;
            }
            let major = major.unwrap_or_else(|| w.enemy_strength_near(attacker, enemy) > 8);
            let hero = |w: &World, h: u32| w.unit(h).is_some_and(|u| u.is_hero && u.extra.illusion.is_none());
            let threat = w.ai.get(&p).map_or(0, |s| s.threat);
            if threat == 0 || w.unit(threat).is_none_or(|u| u.dead) || hero(w, attacker) && !hero(w, threat) {
                if let Some(st) = w.ai.get_mut(&p) {
                    st.threat = attacker;
                }
            }
            let at = w.unit(attacker).map(|u| (u.x, u.y));
            let Some(st) = w.ai.get_mut(&p) else { return };
            st.defense.goal = at;
            st.defense.target = 0;
            st.defense.retreating = false;
            let mut sent = st.defense.units.clone();
            let attack_units = st.attack.units.clone();
            // FUN_6f9d02c0: the attack captain comes only when it can see the attacker
            // and one of its units can attack it.
            let reachable = w.unit_visible_to(attacker, p) && attack_units.iter().any(|&h| w.can_attack_unit(h, attacker));
            let mega = w.mega_target(p) != 0;
            let Some(st) = w.ai.get_mut(&p) else { return };
            if major && !mega && st.attack.units.len() > 2 && reachable {
                st.attack.goal = at;
                st.attack.target = 0;
                st.attack.retreating = false;
                sent.extend(st.attack.units.iter().copied());
            }
            // The captains move at once, every unit with them (FUN_6f9d44d0).
            let Some((x, y)) = at else { return };
            let order = crate::natives::basic::order_id("attack");
            for h in sent {
                // Units already fighting fight on.
                if w.unit(h).is_some_and(|u| !u.dead && u.swing.is_none() && !matches!(u.action, Action::Attack { .. })) && w.acquirable_target(h).is_none() {
                    w.order_unit(h, order, x, y, 0);
                }
            }
        };
        if self.ai.contains_key(&owner) {
            if !structure && !peon && !self.guards(owner).contains(&victim) {
                return;
            }
            let major = if structure { Some(low) } else if peon { Some(false) } else { None };
            respond(self, owner, major);
        } else if structure {
            let allies: Vec<u8> = self.ai.keys().copied().filter(|&p| p != owner && crate::natives::players::is_ally(self, p, owner) && self.ai.get(&p).is_some_and(|s| s.flags.get("defend_player").copied().unwrap_or(false))).collect();
            for p in allies {
                respond(self, p, Some(low));
            }
        }
    }

    /// The strength of `player`'s units within 1200 of `at` (LAB_6f9c5d30).
    fn enemy_strength_near(&self, at: u32, player: u8) -> i32 {
        let Some((x, y)) = self.unit(at).map(|u| (u.x, u.y)) else { return 0 };
        self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == player && !u.dead && (u.x - x).hypot(u.y - y) <= 1200.0).map(|u| if u.is_hero { (u.level + 1) / 2 + 4 } else { u.level }).sum()
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
        let mut others: Vec<u32> = self.captain(!attack).map(|c| c.units.clone()).unwrap_or_default();
        others.extend(self.guards(p));
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
            .filter(|(_, u)| self.is_town_hall(u))
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
    let mut t: HashMap<&'static str, NativeFn> = HashMap::default();
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
        // A hero costs nothing while the AI's player holds a hero token (ai_GetUnitGoldCost).
        GetUnitGoldCost => |w, _, a| { let id = id_from_int(a.i(0)); int(if w.hero_token_pays(w.ai_player(), id) { 0 } else { w.objects.units.get(id).map_or(0, |o| o.int("goldcost")) }) },
        GetUnitWoodCost => |w, _, a| { let id = id_from_int(a.i(0)); int(if w.hero_token_pays(w.ai_player(), id) { 0 } else { w.objects.units.get(id).map_or(0, |o| o.int("lumbercost")) }) },
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
        GetNextExpansion => |w, _, _| {
            let p = w.ai_player();
            int(w.next_expansion(p).map_or(-1, |m| w.mine_index(m)))
        },
        GetMegaTarget => |w, _, _| {
            let p = w.ai_player();
            handle(w.mega_target(p))
        },
        GetBuilding => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            handle(w.unit_order.iter().copied().find(|&h| w.unit(h).is_some_and(|u| u.owner == p && u.is_building && !u.dead)).unwrap_or(0))
        },
        GetEnemyPower => |_, _, _| int(0),
        SetAllianceTarget => |w, _, a| {
            let p = w.ai_player();
            w.set_alliance_target(p, a.h(0));
            none()
        },
        GetAllianceTarget => |w, _, _| {
            let p = w.ai_player();
            let t = w.ai.get(&p).map_or(0, |s| s.alliance_target);
            handle(if w.unit(t).is_some_and(|u| !u.dead) { t } else { 0 })
        },
        SetProduce => |w, vm, a| {
            let p = w.ai_player();
            let id = id_from_int(a.i(1));
            let mut ok = true;
            for _ in 0..a.i(0).max(0) {
                if !w.ai_revive(vm, p, id) && !w.ai_produce(p, id, a.i(2)) {
                    ok = false;
                    break;
                }
            }
            boolean(ok)
        },
        Unsummon => |w, _, a| {
            let p = w.ai_player();
            w.ai_unsummon(p, a.h(0));
            none()
        },
        SetExpansion => |w, _, a| {
            let p = w.ai_player();
            boolean(w.set_expansion(p, a.h(0), id_from_int(a.i(1))))
        },
        SetUpgrade => |w, _, a| {
            let p = w.ai_player();
            let id = id_from_int(a.i(0));
            boolean(w.research(p, id))
        },
        SetHeroLevels => |w, _, a| { let f = a.code(0); if let Some(s) = w.ai_state() { s.hero_levels = f; } none() },
        SetNewHeroes => |w, _, a| set_flag(w, "new_heroes", a.b(0)),
        PurchaseZeppelin => |_, _, _| none(),
        MergeUnits => |w, _, a| {
            let p = w.ai_player();
            boolean(w.merge_units(p, a.i(0), id_from_int(a.i(1)), id_from_int(a.i(2)), id_from_int(a.i(3))))
        },
        ConvertUnits => |w, _, a| {
            let p = w.ai_player();
            w.ai_convert(p, a.i(0), id_from_int(a.i(1)));
            boolean(false)
        },
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
        SetReplacementCount => |w, _, a| {
            if let Some(s) = w.ai_state() {
                s.replacements = a.i(0);
            }
            none()
        },
        GroupTimedLife => |w, _, a| set_flag(w, "group_timed_life", a.b(0)),
        RemoveInjuries => |w, _, _| {
            let p = w.ai_player();
            w.remove_injuries(p);
            none()
        },
        RemoveSiege => |w, _, _| {
            let p = w.ai_player();
            w.remove_siege(p);
            none()
        },
        InitAssault => |w, _, _| {
            if let Some(s) = w.ai_state() {
                s.assault.clear();
                s.assault_full = true;
            }
            none()
        },
        AddAssault => |w, _, a| {
            let id = id_from_int(a.i(1));
            if let Some(s) = w.ai_state() { s.assault.push((id, a.i(0))); }
            let full = w.fill_captain(captain_flag(true), id, a.i(0));
            if !full {
                if let Some(s) = w.ai_state() { s.assault_full = false; }
            }
            boolean(full)
        },
        AddDefenders => |w, _, a| {
            let id = id_from_int(a.i(1));
            if let Some(s) = w.ai_state() { s.defenders.push((id, a.i(0))); }
            boolean(w.fill_captain(false, id, a.i(0)))
        },
        GetCreepCamp => |w, _, a| {
            let p = w.ai_player();
            handle(w.creep_camp(p, a.i(0), a.i(1), a.b(2)))
        },
        StartGetEnemyBase => |w, _, _| {
            let p = w.ai_player();
            let e = w.enemy_town_hall(p).unwrap_or(0);
            if let Some(s) = w.ai_state() { s.enemy_base = e; }
            none()
        },
        WaitGetEnemyBase => |_, _, _| boolean(false),
        GetEnemyBase => |w, _, _| handle(w.ai_state().map_or(0, |s| s.enemy_base)),
        GetExpansionFoe => |w, _, _| {
            let p = w.ai_player();
            handle(w.expansion_foe(p).unwrap_or(0))
        },
        GetEnemyExpansion => |w, _, _| {
            let p = w.ai_player();
            handle(w.enemy_expansion(p))
        },
        GetExpansionX => |w, _, _| {
            let p = w.ai_player();
            int(w.next_expansion(p).and_then(|m| w.unit(m)).map_or(-1, |m| m.x as i32))
        },
        GetExpansionY => |w, _, _| {
            let p = w.ai_player();
            int(w.next_expansion(p).and_then(|m| w.unit(m)).map_or(-1, |m| m.y as i32))
        },
        GetExpansionPeon => |w, _, _| {
            let p = w.ai_player();
            handle(w.expansion_peon(p).unwrap_or(0))
        },
        SetStagePoint => |w, _, a| { let p = (a.r(0), a.r(1)); if let Some(s) = w.ai_state() { s.stage = Some(p); } none() },
        AttackMoveKill => |w, _, a| {
            let t = a.h(0);
            if let Some(s) = w.ai_state() { s.attack_player = None; }
            if let Some(c) = w.captain(true) { c.target = t; c.retreating = false; }
            none()
        },
        AttackMoveXY => |w, _, a| {
            let p = (a.i(0) as f32, a.i(1) as f32);
            if let Some(s) = w.ai_state() { s.attack_player = None; }
            if let Some(c) = w.captain(true) { c.goal = Some(p); c.target = 0; c.retreating = false; }
            none()
        },
        LoadZepWave => |_, _, _| none(),
        SuicidePlayer => |w, _, a| {
            let p = w.ai_player();
            let Some(t) = w.player_of(a.h(0)) else { return boolean(false) };
            boolean(w.suicide_player(p, t, a.b(1), true))
        },
        SuicidePlayerUnits => |w, _, a| {
            let p = w.ai_player();
            let Some(t) = w.player_of(a.h(0)) else { return boolean(false) };
            boolean(w.suicide_player(p, t, a.b(1), false))
        },
        CaptainInCombat => |w, _, a| {
            let units = w.captain_units(a.b(0));
            boolean(units.iter().any(|&h| w.unit(h).is_some_and(|u| u.swing.is_some() || matches!(u.action, Action::Attack { .. }))))
        },
        IsTowered => |w, _, a| boolean(w.is_towered(a.h(0))),
        ClearHarvestAI => |w, _, _| { if let Some(s) = w.ai_state() { s.harvest.clear(); } none() },
        // Workers are put to the work at once (FUN_6f9c7330 orders them within the call),
        // so the engine takes the requests up on this tick.
        HarvestGold => |w, _, a| { let (t, n) = (a.i(0), a.i(1)); if let Some(s) = w.ai_state() { s.harvest.push((true, t, n)); s.harvest_timer = 0.0; } none() },
        HarvestWood => |w, _, a| { let (t, n) = (a.i(0), a.i(1)); if let Some(s) = w.ai_state() { s.harvest.push((false, t, n)); s.harvest_timer = 0.0; } none() },
        StopGathering => |w, _, _| {
            if let Some(s) = w.ai_state() { s.harvest.clear(); }
            set_flag(w, "gathering", false)
        },
        AddGuardPost => |w, _, a| {
            let p = w.ai_player();
            w.add_guard_post(p, id_from_int(a.i(0)), a.r(1), a.r(2));
            none()
        },
        FillGuardPosts => |w, _, _| {
            let p = w.ai_player();
            w.fill_guard_posts(p);
            none()
        },
        ReturnGuardPosts => |w, _, _| {
            let p = w.ai_player();
            w.return_guard_posts(p);
            none()
        },
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
        ResetCaptainLocs => |w, _, _| {
            let p = w.ai_player();
            w.reset_captain_locs(p);
            none()
        },
        ShiftTownSpot => |w, _, a| {
            let p = (a.r(0), a.r(1));
            if let Some(s) = w.ai_state() { s.town_shift = Some(p); }
            none()
        },
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
            if let Some(s) = w.ai_state() { s.attack_player = None; }
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
        SuicideUnit => |w, _, a| {
            let p = w.ai_player();
            if a.i(0) > 0 {
                w.suicide_units(p, a.i(0), id_from_int(a.i(1)), -1);
            }
            none()
        },
        SuicideUnitEx => |w, _, a| {
            let p = w.ai_player();
            if a.i(0) > 0 {
                w.suicide_units(p, a.i(0), id_from_int(a.i(1)), a.i(2));
            }
            none()
        },
        StartThread => |w, vm, a| {
            if let Some(f) = a.code(0) {
                let t = vm.new_thread(f, &[], true);
                if let Some(s) = w.ai_state() { s.spawned.push(t); }
            }
            none()
        },
        Sleep => |_, _, a| NativeResult::Sleep(a.r(0)),
        UnitAlive => |w, _, a| boolean(w.unit(a.h(0)).is_some_and(|u| !u.dead)),
        UnitInvis => |w, _, a| {
            let p = w.ai_player();
            boolean(w.ai_unit_invis(p, a.h(0)))
        },
        IgnoredUnits => |w, _, a| {
            let p = w.ai_player();
            int(w.ignored_units(p, id_from_int(a.i(0))))
        },
        TownThreatened => |w, _, _| {
            let p = w.ai_player();
            boolean(w.ai.get(&p).is_some_and(|s| s.threat != 0))
        },
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
