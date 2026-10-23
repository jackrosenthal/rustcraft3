//! Undead and Night Elf gathering: haunted gold mines (built on a gold mine, isBuildOn
//! over canBuildOn) mined by acolytes standing in a ring around them (Abgm, Aaha);
//! entangled gold mines (Aent) that wisps enter (Aegm, Aenc); and wisps gathering
//! lumber beside a tree without felling it (Awha).  Each miner earns the mine's gold
//! per interval once every interval times the mine's miner capacity (measured in the
//! original: two acolytes or two wisps earn 4 gold a second, five earn 10).

use w3_jass::Vm;

use crate::objects::Id;
use crate::types::*;
use crate::world::World;

/// Engine text tags for gathered resources (UI\MiscData.txt Gold/LumberText*, heights
/// from UI\MiscUI.txt): "+N" at the unit, rising.
#[derive(Clone, Copy)]
pub enum ResourceText {
    Gold,
    Lumber,
    Bounty,
}

impl World {
    /// Whether a structure type is built on top of another unit (a haunted gold mine).
    pub fn builds_on(&self, id: Id) -> bool {
        self.objects.units.get(id).is_some_and(|o| o.int("isBuildOn") != 0)
    }

    /// The unit a build-on structure placed near (x, y) would cover: the nearest visible
    /// living unit that can be built on (a gold mine), within 128.
    pub fn build_on_site(&self, x: f32, y: f32) -> Option<u32> {
        self.unit_order
            .iter()
            .copied()
            .filter_map(|h| {
                let u = self.unit(h)?;
                let o = self.objects.units.get(u.type_id)?;
                (!u.dead && !u.hidden && o.int("canBuildOn") != 0).then(|| ((u.x - x).hypot(u.y - y), h))
            })
            .filter(|(d, _)| *d <= 128.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h)
    }

    /// A structure takes over the gold mine under it: the mine hides and its gold moves
    /// to the structure.
    pub fn cover_mine(&mut self, cover: u32, mine: u32) {
        let gold = self.unit(mine).map_or(0, |m| m.resource);
        if let Some(m) = self.unit_mut(mine) {
            m.hidden = true;
        }
        if let Some(c) = self.unit_mut(cover) {
            c.extra.mine = Some(mine);
            c.resource = gold;
        }
    }

    /// A covering structure is gone: its mine returns with the gold left, or goes too
    /// when none is left.
    pub fn uncover_mine(&mut self, vm: &mut Vm, cover: u32) {
        let Some((mine, gold)) = self.unit(cover).and_then(|c| c.extra.mine.map(|m| (m, c.resource))) else { return };
        if let Some(c) = self.unit_mut(cover) {
            c.extra.mine = None;
        }
        if gold <= 0 {
            crate::natives::units::kill_unit(self, vm, mine, 0);
            return;
        }
        if let Some(m) = self.unit_mut(mine) {
            m.hidden = false;
            m.resource = gold;
        }
    }

    /// Adds gathered resources to a player, less the upkeep tax, and shows the amount
    /// added; returns it.
    pub fn gather(&mut self, owner: u8, at: u32, gold: i32, lumber: i32, z: f32) -> (i32, i32) {
        let (gold_tax, lumber_tax) = self.upkeep_tax(owner);
        let (kept_gold, kept_lumber) = (gold * (100 - gold_tax) / 100, lumber * (100 - lumber_tax) / 100);
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += kept_gold;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += kept_lumber;
        *p.state.entry(player_state::GOLD_GATHERED).or_insert(0) += gold;
        *p.state.entry(player_state::LUMBER_GATHERED).or_insert(0) += lumber;
        if kept_gold > 0 {
            self.resource_text(at, kept_gold, ResourceText::Gold, z);
        }
        if kept_lumber > 0 {
            self.resource_text(at, kept_lumber, ResourceText::Lumber, z);
        }
        (kept_gold, kept_lumber)
    }

    /// "+N" over a unit for its owner when that is the local player (0x6f3f6220).
    pub fn resource_text(&mut self, unit: u32, amount: i32, kind: ResourceText, z: f32) {
        let Some((x, y, owner)) = self.unit(unit).map(|u| (u.x, u.y, u.owner)) else { return };
        if owner != self.local_player {
            return;
        }
        let key = match kind {
            ResourceText::Gold => "Gold",
            ResourceText::Lumber => "Lumber",
            ResourceText::Bounty => "Bounty",
        };
        let tag = self.engine_text_tag(key, format!("+{amount}"), x, y, self.ground_z(x, y) + z);
        self.handles.insert(Obj::TextTag(Box::new(tag)));
    }

    /// An engine text tag of one of UI\MiscData.txt's kinds (Gold, Lumber, Bounty, Miss,
    /// CriticalStrike, ShadowStrike, ManaBurn, Bash): its color, velocity, lifetime, and
    /// fade start, and its height from UI\MiscUI.txt.
    pub fn engine_text_tag(&self, kind: &str, text: String, x: f32, y: f32, z: f32) -> TextTag {
        let list = |k: &str| -> Vec<f32> { self.misc.get_list("Misc", &format!("{kind}{k}")).iter().filter_map(|v| v.trim().parse().ok()).collect() };
        let num = |k: &str, d: f32| list(k).first().copied().unwrap_or(d);
        let c = list("TextColor");
        // Colors are alpha, red, green, blue.
        let color = if c.len() == 4 { [c[1] as u8, c[2] as u8, c[3] as u8, c[0] as u8] } else { [255; 4] };
        let v = list("TextVelocity");
        let height = self.misc_f32(&format!("{kind}TextHeight"), 0.024);
        TextTag {
            text,
            height,
            x,
            y,
            z,
            unit: None,
            color,
            velocity: (v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)),
            visible: true,
            suspended: false,
            permanent: false,
            age: 0.0,
            lifespan: num("TextLifetime", 2.0),
            fadepoint: num("TextFadeStart", 1.0),
            local: true,
        }
    }

    /// Text tags age; temporary ones expire at their lifespan.
    pub fn update_text_tags(&mut self, dt: f32) {
        let mut expired = Vec::new();
        for (&h, o) in self.handles.iter_mut() {
            if let Obj::TextTag(t) = o {
                if t.suspended {
                    continue;
                }
                t.age += dt;
                if !t.permanent && t.age >= t.lifespan {
                    expired.push(h);
                }
            }
        }
        for h in expired {
            self.handles.remove(h);
        }
    }

    /// The ring slot positions around a haunted mine (0x6f4d7e40): min(DataC, 8) slots
    /// at DataD, the first north of it and the rest clockwise (x by the sine, y by the
    /// cosine of k turns over the count), each facing the mine; None for unwalkable ones.
    fn mine_slots(&self, mine: u32) -> Vec<Option<(f32, f32, f32)>> {
        let Some((abgm, lvl)) = self.ability_with_code(mine, "Abgm") else { return Vec::new() };
        let Some(m) = self.unit(mine) else { return Vec::new() };
        let a = self.objects.abilities.get(abgm);
        let n = a.map_or(5, |a| a.level_real("DataC", lvl) as usize).min(8);
        let r = a.map_or(200.0, |a| a.level_real("DataD", lvl));
        (0..n)
            .map(|k| {
                let ang = k as f32 * std::f32::consts::TAU / n as f32;
                let (x, y) = (m.x + r * ang.sin(), m.y + r * ang.cos());
                let ok = self.pathing.as_ref().is_none_or(|p| p.flags_at(p.cell_of(x, y)).is_some_and(|f| f & crate::pathing::NO_WALK == 0));
                let face = (m.y - y).atan2(m.x - x).to_degrees();
                ok.then_some((x, y, face))
            })
            .collect()
    }

    /// Whether a unit can mine a haunted mine or entangled mine of its owner.
    pub fn can_mine_covered(&self, h: u32, mine: u32) -> bool {
        let (Some(u), Some(m)) = (self.unit(h), self.unit(mine)) else { return false };
        if m.dead || m.owner != u.owner || m.constructing.is_some() {
            return false;
        }
        (self.ability_with_code(h, "Aaha").is_some() && self.ability_with_code(mine, "Abgm").is_some())
            || (self.ability_with_code(h, "Awha").is_some() && self.ability_with_code(mine, "Aegm").is_some())
    }

    /// Takes gold from a covering mine; it collapses when empty.
    fn take_gold(&mut self, vm: &mut Vm, mine: u32, amount: i32) -> i32 {
        let left = self.unit(mine).map_or(0, |m| m.resource);
        let take = amount.min(left).max(0);
        if let Some(m) = self.unit_mut(mine) {
            m.resource -= take;
        }
        if self.unit(mine).is_some_and(|m| m.resource <= 0) {
            crate::natives::units::kill_unit(self, vm, mine, 0);
        }
        take
    }

    /// An acolyte at a haunted mine: to a free ring slot, then gold every interval times
    /// the slot count.
    pub(crate) fn acolyte_harvest(&mut self, vm: &mut Vm, h: u32, mine: u32, dt: f32) {
        if !self.can_mine_covered(h, mine) {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            u.extra.mine_slot = None;
            crate::sim::set_anim(u, "stand");
            return;
        }
        let slots = self.mine_slots(mine);
        let slot = match self.unit(h).and_then(|u| u.extra.mine_slot) {
            Some(k) => Some(k),
            None => {
                let taken: Vec<usize> = self
                    .unit_order
                    .iter()
                    .filter(|&&o| o != h)
                    .filter_map(|&o| self.unit(o).filter(|u| !u.dead && matches!(u.action, Action::Harvest { target } if target == mine)).and_then(|u| u.extra.mine_slot))
                    .collect();
                // The nearest free slot.
                let (ux, uy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or_default();
                (0..slots.len())
                    .filter(|k| !taken.contains(k))
                    .filter_map(|k| slots[k].map(|(x, y, _)| ((x - ux).hypot(y - uy), k)))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, k)| k)
            }
        };
        let Some(k) = slot else {
            // The mine is full.
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            crate::sim::set_anim(u, "stand");
            return;
        };
        let Some((sx, sy, face)) = slots.get(k).copied().flatten() else { return };
        let first = self.unit(h).is_some_and(|u| u.extra.mine_slot != Some(k));
        if first {
            if let Some(u) = self.unit_mut(h) {
                u.extra.mine_slot = Some(k);
                u.path.clear();
                u.work_timer = -1.0;
            }
        }
        let (ux, uy) = self.unit(h).map(|u| (u.x, u.y)).unwrap();
        if (ux - sx).hypot(uy - sy) > 8.0 {
            let need = self.unit(h).is_some_and(|u| u.path.is_empty() || u.repath_timer <= 0.0);
            if need {
                self.path_to(h, (sx, sy));
            }
            if !self.walk(h, dt) {
                return;
            }
            if let Some(u) = self.unit_mut(h) {
                u.x = sx;
                u.y = sy;
            }
        }
        let (abgm, lvl) = self.ability_with_code(mine, "Abgm").unwrap();
        let a = self.objects.abilities.get(abgm);
        let per = a.map_or(10.0, |a| a.level_real("DataA", lvl)) as i32;
        let interval = a.map_or(1.0, |a| a.level_real("DataB", lvl)) * slots.len().max(1) as f32;
        let u = self.unit_mut(h).unwrap();
        crate::sim::turn(u, face, dt);
        if u.work_timer < 0.0 {
            u.work_timer = interval;
            crate::sim::set_anim(u, "stand work gold");
            return;
        }
        crate::sim::set_anim(u, "stand work gold");
        u.work_timer -= dt;
        if u.work_timer > 0.0 {
            return;
        }
        u.work_timer += interval;
        let owner = u.owner;
        let got = self.take_gold(vm, mine, per);
        if got > 0 {
            self.gather(owner, mine, got, 0, 0.0);
        }
    }

    /// A wisp: into its entangled mine for gold, or beside a tree for lumber.
    pub(crate) fn wisp_harvest(&mut self, vm: &mut Vm, h: u32, target: u32, dt: f32) {
        if self.unit(target).is_some() {
            self.wisp_mine(vm, h, target, dt);
        } else {
            self.wisp_lumber(h, target, dt);
        }
    }

    fn wisp_mine(&mut self, vm: &mut Vm, h: u32, mine: u32, dt: f32) {
        if !self.can_mine_covered(h, mine) {
            self.leave_mine(h);
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            crate::sim::set_anim(u, "stand");
            return;
        }
        let capacity = self.ability_with_code(mine, "Aenc").map_or(5, |(a, l)| self.objects.abilities.get(a).map_or(5.0, |o| o.level_real("DataA", l)) as u32).max(1);
        let (aegm, lvl) = self.ability_with_code(mine, "Aegm").unwrap();
        let a = self.objects.abilities.get(aegm);
        let per = a.map_or(10.0, |a| a.level_real("DataA", lvl)) as i32;
        let interval = a.map_or(1.0, |a| a.level_real("DataB", lvl)) * capacity as f32;
        if self.unit(h).is_some_and(|u| u.inside != mine) {
            if !self.approach(h, mine, 8.0, dt) {
                return;
            }
            if self.unit(mine).is_some_and(|m| m.inside >= capacity) {
                let u = self.unit_mut(h).unwrap();
                u.action = Action::Idle;
                crate::sim::set_anim(u, "stand");
                return;
            }
            if let Some(m) = self.unit_mut(mine) {
                m.inside += 1;
            }
            let u = self.unit_mut(h).unwrap();
            u.inside = mine;
            u.hidden = true;
            u.work_timer = interval;
            self.mine_work_anim(mine);
            return;
        }
        let u = self.unit_mut(h).unwrap();
        u.work_timer -= dt;
        if u.work_timer > 0.0 {
            return;
        }
        u.work_timer += interval;
        let owner = u.owner;
        let got = self.take_gold(vm, mine, per);
        if got > 0 {
            self.gather(owner, mine, got, 0, 0.0);
        }
    }

    /// The entangled mine shows how many wisps are inside ("stand work first" .. "fifth").
    pub(crate) fn mine_work_anim(&mut self, mine: u32) {
        const N: [&str; 5] = ["first", "second", "third", "fourth", "fifth"];
        if self.ability_with_code(mine, "Aegm").is_none() {
            return;
        }
        if let Some(m) = self.unit_mut(mine) {
            let name = match m.inside {
                0 => "stand".to_string(),
                n => format!("stand work {}", N[(n as usize - 1).min(4)]),
            };
            crate::sim::set_anim(m, &name);
        }
    }

    /// A wisp beside a tree: DataA lumber every Dur seconds, the tree unharmed; the
    /// amount shows 100 above it (0x6f5af880).
    fn wisp_lumber(&mut self, h: u32, tree: u32, dt: f32) {
        let Some((awha, lvl)) = self.ability_with_code(h, "Awha") else { return };
        if !self.is_tree_pub(tree) {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            crate::sim::set_anim(u, "stand");
            return;
        }
        if !self.approach(h, tree, 64.0, dt) {
            // No way to the tree: take the nearest one instead.
            if self.unit(h).is_some_and(|u| u.path.is_empty()) {
                if let Some(t) = self.nearest_tree_pub(h, 900.0).filter(|&t| t != tree) {
                    self.unit_mut(h).unwrap().action = Action::Harvest { target: t };
                }
            }
            return;
        }
        let a = self.objects.abilities.get(awha);
        let per = a.map_or(5.0, |a| a.level_real("DataA", lvl)) as i32;
        let interval = a.map_or(8.0, |a| a.level_real("Dur", lvl)).max(0.1);
        let tp = self.widget_pos(tree).unwrap_or_default();
        let u = self.unit_mut(h).unwrap();
        let want = crate::sim::angle_to((u.x, u.y), tp);
        crate::sim::turn(u, want, dt);
        crate::sim::set_anim(u, "stand work");
        if u.harvest_source != tree {
            u.harvest_source = tree;
            u.work_timer = interval;
        }
        u.work_timer -= dt;
        if u.work_timer > 0.0 {
            return;
        }
        u.work_timer += interval;
        let owner = u.owner;
        self.gather(owner, h, 0, per, 100.0);
    }

    /// Entangle Gold Mine (Aent): a gold mine becomes the caster's entangled mine
    /// (UnitID), which takes it over.
    pub fn entangle(&mut self, caster: u32, mine: u32) {
        let Some((aent, lvl)) = self.ability_with_code(caster, "Aent") else { return };
        let Some(owner) = self.unit(caster).map(|u| u.owner) else { return };
        if !self.unit(mine).is_some_and(|m| !m.dead && !m.hidden) || self.ability_with_code(mine, "Agld").is_none() {
            return;
        }
        let id = crate::objects::id_from_str(self.objects.abilities.get(aent).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
        let (x, y, facing) = self.unit(mine).map(|m| (m.x, m.y, m.facing)).unwrap();
        let e = crate::natives::units::create_unit(self, owner, id, x, y, facing);
        if e != 0 {
            self.cover_mine(e, mine);
        }
    }

    /// A finished haunted mine marks its slots with its Effectart (UndeadMineCircle).
    pub fn update_mines(&mut self) {
        let mines: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| u.constructing.is_none() && (u.dead != u.extra.slot_effects.is_empty())))
            .filter(|&h| self.ability_with_code(h, "Abgm").is_some())
            .collect();
        for h in mines {
            if self.unit(h).is_some_and(|u| u.dead) {
                for e in std::mem::take(&mut self.unit_mut(h).unwrap().extra.slot_effects) {
                    if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                        x.destroyed = true;
                    }
                }
                continue;
            }
            let (a, _) = self.ability_with_code(h, "Abgm").unwrap();
            let model = self.profile_art(a, "Effectart");
            let mut effects = Vec::new();
            let (mx, my) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or_default();
            for (x, y, _) in self.mine_slots(h).into_iter().flatten() {
                // Each marker faces away from the mine, its tail toward it.
                let z = self.ground_z(x, y);
                let facing = (y - my).atan2(x - mx).to_degrees();
                effects.push(self.handles.insert(Obj::Effect(Effect { model: model.clone(), x, y, z, target: None, destroyed: false, facing })));
            }
            if let Some(u) = self.unit_mut(h) {
                u.extra.slot_effects = effects;
            }
        }
    }

    /// A moon well's pool (CAbilityManaBattery, Ambt): its Effectart for the owner's race
    /// (human, orc, undead, night elf, demon) over the finished structure, risen above
    /// the ground by its mana fraction of DataD (0x6f64d2f0).
    pub fn update_moon_wells(&mut self) {
        let wells: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.ability_with_code(h, "Ambt").is_some()).collect();
        for h in wells {
            let Some(u) = self.unit(h) else { continue };
            let (x, y, dead, building, frac, pool, owner) = (u.x, u.y, u.dead, u.constructing.is_some(), if u.max_mana > 0.0 { u.mana / u.max_mana } else { 0.0 }, u.extra.slot_effects.first().copied(), u.owner);
            if dead || building {
                if let (true, Some(e)) = (dead, pool) {
                    if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                        x.destroyed = true;
                    }
                    self.unit_mut(h).unwrap().extra.slot_effects.clear();
                }
                continue;
            }
            let (a, lvl) = self.ability_with_code(h, "Ambt").unwrap();
            let height = self.objects.abilities.get(a).map_or(0.0, |o| o.level_real("DataD", lvl));
            let z = self.ground_z(x, y) + frac * height;
            match pool {
                Some(e) => {
                    if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                        x.z = z;
                    }
                }
                None => {
                    let race = self.players.get(owner as usize).map_or(4, |p| p.race);
                    let index = match race {
                        1 => 0,
                        2 => 1,
                        3 => 2,
                        4 => 3,
                        _ => 4,
                    };
                    let model = self.profile_art(a, "Effectart");
                    let model = model.split(',').nth(index).or_else(|| model.split(',').next()).unwrap_or_default().trim().to_string();
                    let e = self.handles.insert(Obj::Effect(Effect { model, x, y, z, target: None, destroyed: false, facing: 0.0 }));
                    self.unit_mut(h).unwrap().extra.slot_effects = vec![e];
                }
            }
        }
    }
}
