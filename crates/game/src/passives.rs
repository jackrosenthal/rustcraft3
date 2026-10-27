//! Passive unit abilities: detection (True Sight, Magic Sentry, Detector), Permanent
//! Invisibility, Ultravision, Hardened Skin, Spiked Barricades, Pulverize, Slow
//! Poison, Corrosive Breath, and Kaboom.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// Abilities that reveal invisible units within their cast range.
const DETECTORS: [&str; 4] = ["Atru", "Adts", "Adet", "Agyv"];

impl World {
    fn passive(&self, h: u32, code: &str) -> Option<(Id, u32)> {
        let u = self.unit(h)?;
        u.abilities.iter().find(|(a, _)| self.ability_code(*a) == code && self.ability_enabled(u.owner, *a)).map(|&(a, l)| (a, l.max(1) as u32))
    }

    /// Permanent Invisibility (Apiv).
    pub fn permanently_invisible(&self, u: &Unit) -> bool {
        // Permanent Invisibility, and the Shade's Ghost.
        u.abilities.iter().any(|(a, _)| matches!(self.ability_code(*a).as_str(), "Apiv" | "Agho")) || self.mine_armed(u)
    }

    /// Whether `player` (or a player sharing vision with it) has a detector in range of
    /// unit `h`.
    pub fn detected_by(&self, h: u32, player: u8) -> bool {
        let Some(t) = self.unit(h) else { return false };
        // Reveal and Flare areas.
        let sees = |p: u8| p == player || self.players.get(p as usize).is_some_and(|o| o.alliances[player as usize % PLAYER_COUNT][crate::fog::ALLIANCE_SHARED_VISION]);
        if self.detect_areas.iter().any(|&(p, x, y, r, until)| until > self.time && sees(p) && (t.x - x).hypot(t.y - y) <= r) {
            return true;
        }
        self.unit_order.iter().filter_map(|&d| self.unit(d)).any(|d| {
            if d.dead || d.hidden {
                return false;
            }
            let ours = d.owner == player || self.players.get(d.owner as usize).is_some_and(|o| o.alliances[player as usize % PLAYER_COUNT][crate::fog::ALLIANCE_SHARED_VISION]);
            ours && d.abilities.iter().any(|&(a, l)| DETECTORS.contains(&self.ability_code(a).as_str()) && self.ability_enabled(d.owner, a) && (d.x - t.x).hypot(d.y - t.y) <= self.data(a, "Rng", l.max(1) as u32))
        })
    }

    /// Ultravision (Ault): day sight at night.
    pub fn ultravision(&self, u: &Unit) -> bool {
        u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Ault") || self.upgrade_bonus(u.owner, u.type_id).ultravision
    }

    /// Hardened Skin (Assk): a DataA% chance to take DataC less damage, but at least
    /// DataB, from ranged (DataD) and melee (DataE) attacks.
    pub fn hardened_skin(&mut self, target: u32, melee: bool, dmg: f32) -> f32 {
        let Some((a, l)) = self.passive(target, "Assk") else { return dmg };
        let applies = if melee { self.data(a, "DataE", l) != 0.0 } else { self.data(a, "DataD", l) != 0.0 };
        if !applies || !self.prd_roll(target, a, self.data(a, "DataA", l)) {
            return dmg;
        }
        (dmg - self.data(a, "DataC", l)).max(self.data(a, "DataB", l).min(dmg))
    }

    /// Spiked Barricades (Aspi): melee attackers of the structure take the owner's
    /// research rspi damage.
    pub fn spiked_barricades(&self, target: u32) -> f32 {
        let Some(t) = self.unit(target) else { return 0.0 };
        if self.passive(target, "Aspi").is_none() {
            return 0.0;
        }
        self.upgrade_bonus(t.owner, t.type_id).spikes
    }

    /// Poisons, as the attack lands: Slow Poison (DataB slower movement, DataC slower
    /// attacks), and Poison and Venom, from the unit or a held item (DataB slower
    /// attacks, DataC slower movement), each DataA once a second for Dur, the first at
    /// once; each touches the target with a hit of nothing first (measured: a Dryad's
    /// arrow and an envenomed Wind Rider's spear each show a damage event of 0, then
    /// the attack's, then 4 of poison each second from the next moment).
    pub(crate) fn poison_on_hit(&mut self, vm: &mut Vm, h: u32, target: u32) {
        let Some(u) = self.unit(h) else { return };
        let Some(building) = self.unit(target).filter(|t| !t.dead).map(|t| t.is_building) else { return };
        let owner = u.owner;
        let mut abilities = u.abilities.clone();
        for item in u.inventory.clone() {
            if let Some(t) = self.item(item).map(|i| i.type_id) {
                abilities.extend(self.item_abilities(t).into_iter().map(|a| (a, 1)));
            }
        }
        for (a, l) in abilities {
            let code = self.ability_code(a);
            if !self.ability_enabled(owner, a) {
                continue;
            }
            let l = l.max(1) as u32;
            // Liquid Fire likewise: burning structures take DataA a second and attack
            // slower (measured: a farm hit at 0.98 seconds takes 0, the hit, and 8 at
            // 0.99, 1.99, and 2.99, its Dur 3).
            if code == "Aliq" && building {
                self.spell_damage(vm, h, target, 0.0);
                let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", l)).unwrap_or_default();
                let id = id_from_str(buff.split(',').next().unwrap_or("").trim());
                let d = |k: &str| self.data(a, k, l);
                let b = Buff { id, ability: a, level: l, caster: h, remaining: d("Dur"), dps: d("DataA"), dps_every: 1.0, dps_next: 0.0, attack_factor: -d("DataC"), no_repair: d("DataD") == 0.0, ..Default::default() };
                self.apply_buff(target, b);
                continue;
            }
            if building {
                continue;
            }
            // Frost Attack: its slow as the attack lands, touching the target with a hit
            // of nothing first (measured: a Nerubian Tower's every strike shows 0 then its
            // damage).
            if matches!(code.as_str(), "Afra" | "Afrb") {
                self.spell_damage(vm, h, target, 0.0);
                let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", l)).unwrap_or_default();
                let id = id_from_str(buff.split(',').next().unwrap_or("").trim());
                let dur = if self.hero_duration(target) { self.data(a, "HeroDur", l) } else { self.data(a, "Dur", l) };
                let (mv, at) = (self.misc_f32("FrostMoveSpeedDecrease", 0.5), self.misc_f32("FrostAttackSpeedDecrease", 0.25));
                self.apply_buff(target, Buff { id, ability: a, level: l, caster: h, remaining: dur, move_factor: -mv, attack_factor: -at, ..Default::default() });
                continue;
            }
            if !matches!(code.as_str(), "Aspo" | "Apoi" | "Aven" | "Apo2") {
                continue;
            }
            self.spell_damage(vm, h, target, 0.0);
            let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", l)).unwrap_or_default();
            let id = id_from_str(buff.split(',').next().unwrap_or("").trim());
            let dur = if self.hero_duration(target) { self.data(a, "HeroDur", l) } else { self.data(a, "Dur", l) };
            let (b, c) = (self.data(a, "DataB", l), self.data(a, "DataC", l));
            let (move_factor, attack_factor) = if code == "Aspo" { (-b, -c) } else { (-c, -b) };
            let b = Buff { id, ability: a, level: l, caster: h, remaining: dur, dps: self.data(a, "DataA", l), dps_every: 1.0, dps_next: 0.0, move_factor, attack_factor, ..Default::default() };
            self.apply_buff(target, b);
        }
    }

    /// Attack passives of the attacker: Pulverize and Kaboom.  (Corrosive Breath burns
    /// nothing: measured, a farm hit by a Chimaera takes the hit alone.)
    pub fn attack_passives(&mut self, vm: &mut Vm, h: u32, target: u32) {
        let Some(u) = self.unit(h) else { return };
        let (owner, ux, uy) = (u.owner, u.x, u.y);
        let (tx, ty) = self.unit(target).map_or((ux, uy), |t| (t.x, t.y));
        // Pulverize: a DataA% chance of DataB damage to enemy ground units within DataC
        // of the attacker, half within DataD.
        if let Some((a, l)) = self.passive(h, "Awar") {
            if self.prd_roll(h, a, self.data(a, "DataA", l)) {
                let (dmg, full, half) = (self.data(a, "DataB", l), self.data(a, "DataC", l), self.data(a, "DataD", l));
                for t in self.enemies_in(owner, ux, uy, half) {
                    let flying = self.unit(t).is_some_and(|tu| self.objects.units.get(tu.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly")));
                    if flying {
                        continue;
                    }
                    let d = self.unit(t).map_or(f32::MAX, |tu| (tu.x - ux).hypot(tu.y - uy));
                    self.spell_damage(vm, h, t, if d <= full { dmg } else { dmg * 0.5 });
                }
            }
        }
        // Kaboom: the attacker explodes: DataB damage within DataA, DataD within DataC,
        // structures taking DataE times as much.
        if let Some((a, l)) = self.passive(h, "Asds") {
            self.kaboom(vm, h, a, l, tx, ty);
        }
    }

    pub fn kaboom(&mut self, vm: &mut Vm, h: u32, a: Id, l: u32, x: f32, y: f32) {
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let (full_r, full, part_r, part, bld) = (self.data(a, "DataA", l), self.data(a, "DataB", l), self.data(a, "DataC", l), self.data(a, "DataD", l), self.data(a, "DataE", l));
        for t in self.enemies_in(owner, x, y, full_r.max(part_r)) {
            let (d, building) = self.unit(t).map_or((f32::MAX, false), |tu| ((tu.x - x).hypot(tu.y - y), tu.is_building));
            let dmg = if d <= full_r { full } else { part };
            self.spell_damage(vm, h, t, if building { dmg * bld } else { dmg });
        }
        crate::natives::units::kill_unit(self, vm, h, h);
    }
}

impl World {
    /// Wander (CAbilityWander): an idle wanderer waits 5 to 10 seconds (vf189, the main
    /// generator), then walks to a point up to 250 away on each axis (vf190).
    pub fn update_wander(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && !u.paused && matches!(u.action, Action::Idle))).collect();
        let mut due: Vec<(usize, u32)> = Vec::new();
        for h in units {
            // A walk ended: its order's objects go (`Tags::move_done`).
            if let Some(slots) = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.order_tags)).filter(|s| s.len() == 18) {
                let late = self.unit(h).is_some_and(|u| u.extra.arrived_last_step);
                self.tags.move_done(&slots, late);
            }
            if self.ability_with_code(h, "Awan").is_none() {
                continue;
            }
            // The wait runs only while it stands (measured: sheep start a walk every 9 to
            // 11 seconds, walk included).
            // The first wait counts from the unit's making, and a walk starts after the
            // first movement step past its time, more than 1e-4 past it (traced: Booty
            // Bay's first walks, the third given after its turn's third step).
            let wait = match self.unit(h).and_then(|u| u.extra.wander_wait) {
                Some(t) => t,
                None => self.rng.real(5.0, 10.0),
            };
            // (Its wait counts from a step before the unit's making: fitted to the traced
            // turns, the first walks given after the steps the original's are.)
            let early = 0.03;
            match self.step_times.iter().position(|&s| s + early - wait as f64 > 1e-4) {
                Some(k) => due.push((k, h)),
                None => self.unit_mut(h).unwrap().extra.wander_wait = Some(wait - dt),
            }
        }
        // Walks start in the order their steps come.
        due.sort_by_key(|&(k, _)| k);
        for (k, h) in due {
            let next = self.rng.real(5.0, 10.0);
            // The point: on each axis, x first, a draw of stream 8 times 500 less 250
            // (CAbilityWander vf190, FUN_6f59af20; its constants read from the running
            // original: 250 and 500).
            let r = &mut self.rand_data.streams[8];
            let dx = r.real(0.0, 1.0) * 500.0 - 250.0;
            let dy = r.real(0.0, 1.0) * 500.0 - 250.0;
            // The next wait counts from the step it starts after.
            let since = dt as f64 - self.step_times.get(k).copied().unwrap_or(dt as f64);
            let u = self.unit_mut(h).unwrap();
            u.extra.wander_wait = Some(next - since as f32);
            u.extra.walk_from = Some(k + 1);
            let (x, y) = (u.x + dx, u.y + dy);
            log::debug!("wander: {h:#x} {} from ({:.1},{:.1}) to ({x:.1},{y:.1}) after step {k}", crate::objects::id_str(u.type_id), u.x, u.y);
            u.action = Action::Move { x, y };
            u.path.clear();
            u.repath_timer = 0.0;
            let slots = self.tags.move_issued();
            if let Some(u) = self.unit_mut(h) {
                u.extra.order_tags = slots;
            }
        }
    }
}

impl World {
    /// Abilities that only attach art to their unit (Sphere's BloodElfBall at "sprite
    /// first", "second", and "third"; the Tree of Life upgrades'): each Targetart at its
    /// Targetattach, TargetattachN.
    pub fn attach_ability_art(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let mut out = Vec::new();
        for &(a, _) in &u.abilities {
            if !matches!(self.ability_code(a).as_str(), "Asph" | "Atol") {
                continue;
            }
            let Some(o) = self.objects.abilities.get(a) else { continue };
            let models = o.list("Targetart");
            let n = o.str("Targetattachcount").trim().parse::<usize>().unwrap_or(1).max(1);
            for k in 0..n {
                let key = if k == 0 { "Targetattach".to_string() } else { format!("Targetattach{k}") };
                let attach = o.str(&key);
                let model = models.get(k).or(models.first()).cloned().unwrap_or_default();
                if !model.is_empty() {
                    out.push((model, attach));
                }
            }
        }
        let mut handles = Vec::new();
        for (model, attach) in out {
            handles.push(self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((h, attach)), destroyed: false, facing: 0.0 })));
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.art = handles;
        }
    }

    /// Spiked Barricades (Aspi) and Reinforced Burrows (Arbr) show their Targetart once
    /// the owner has researched what they stand for: spike damage (rspi) and reinforced
    /// defense (rart).  CAbilitySpiked and CAbilityReinforce share it.  A hero's aura
    /// shows its Targetart on its bearer while it lives (measured: UndeadX05's
    /// Varimathras stands in Vampiric Aura's rune ring in the original, where ours
    /// showed none).
    pub fn update_upgrade_art(&mut self) {
        let mut changes: Vec<(u32, Id, bool)> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            for &(a, lvl) in &u.abilities {
                let code = self.ability_code(a);
                // Magic Sentry's eye shows over a tower once its research enables it
                // (measured: HumanX02's Scout Tower, Rhse researched, wears it).
                let aura = matches!(code.as_str(), "AHad" | "AHab" | "AUau" | "AOae" | "AEar" | "AUav" | "AEah" | "Adts");
                if !aura && !matches!(code.as_str(), "Aspi" | "Arbr") {
                    continue;
                }
                let bonus = self.upgrade_bonus(u.owner, u.type_id);
                let want = !u.dead && if aura { lvl >= 1 && self.ability_enabled(u.owner, a) } else if code == "Aspi" { bonus.spikes > 0.0 } else { bonus.armor_type > 0 };
                let has = u.extra.upgrade_art.iter().any(|e| e.0 == a);
                if want != has {
                    changes.push((h, a, want));
                }
            }
            // Art of abilities it no longer has goes.
            for (a, _) in &u.extra.upgrade_art {
                if !u.abilities.iter().any(|x| x.0 == *a) {
                    changes.push((h, *a, false));
                }
            }
        }
        for (h, a, want) in changes {
            if want {
                let Some(o) = self.objects.abilities.get(a) else { continue };
                let side = if self.ability_code(a) == "Adts" { "Caster" } else { "Target" };
                let models = o.list(&format!("{side}art"));
                let n = o.str(&format!("{side}attachcount")).trim().parse::<usize>().unwrap_or(1).max(1);
                let mut art = Vec::new();
                for k in 0..n {
                    let key = if k == 0 { format!("{side}attach") } else { format!("{side}attach{k}") };
                    let attach = o.str(&key);
                    let model = models.get(k).or(models.first()).cloned().unwrap_or_default();
                    if !model.is_empty() {
                        art.push(self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((h, attach)), destroyed: false, facing: 0.0 })));
                    }
                }
                if let Some(u) = self.unit_mut(h) {
                    u.extra.upgrade_art.push((a, art));
                }
            } else {
                let gone: Vec<u32> = self.unit_mut(h).map(|u| {
                    let (out, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut u.extra.upgrade_art).into_iter().partition(|e| e.0 == a);
                    u.extra.upgrade_art = keep;
                    out.into_iter().flat_map(|e| e.1).collect()
                }).unwrap_or_default();
                for e in gone {
                    if let Some(Obj::Effect(f)) = self.handles.get_mut(e) {
                        f.destroyed = true;
                    }
                }
            }
        }
    }

    /// A computer player's idle workers go to mine gold on their own, AI script or none
    /// (measured: a computer player's peasants by its town hall and a gold mine are
    /// harvesting half a second in, and one made a second in is by four, where a user's
    /// stay idle; workers with only trees about, peasant or ghoul, stay idle too;
    /// Human02's Uther, a computer slot without a script, so works the mine its intro
    /// shows, and holds it when the intro is skipped).  Looked for every quarter second.
    pub fn computer_idle_workers(&mut self, dt: f32) {
        const EVERY: f32 = 0.25;
        if (self.time / EVERY).floor() == ((self.time - dt) / EVERY).floor() {
            return;
        }
        let order = crate::natives::basic::order_id("autoharvestgold");
        let idle: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| {
                self.unit(h).is_some_and(|u| {
                    !u.dead && !u.hidden && !u.paused && !u.is_building && matches!(u.action, Action::Idle) && u.order.id == 0
                        && self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER)
                })
            })
            .filter(|&h| ["Ahar", "Ahrl"].iter().any(|c| self.ability_with_code(h, c).is_some()))
            .collect();
        for h in idle {
            self.order_unit(h, order, 0.0, 0.0, 0);
        }
    }

    /// Takes a unit's ability art away.
    pub fn detach_ability_art(&mut self, h: u32) {
        let Some(handles) = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.art)) else { return };
        for e in handles {
            if let Some(Obj::Effect(f)) = self.handles.get_mut(e) {
                f.destroyed = true;
            }
        }
    }
}

impl World {
    /// The tornado's own abilities.  Tornado Wander (Atwa): a new point within Area of
    /// where it appeared every Dur.  Tornado Damage (Atdg): structures it allows take,
    /// once a second, DataA within Area, DataC within DataB, and DataE within DataD.  Tornado
    /// Spin (Atsp): ground enemies within Area are thrown up for Dur (HeroDur on Heroes),
    /// each at most once every DataB.
    pub fn update_tornadoes(&mut self, vm: &mut Vm, dt: f32) {
        let tornadoes: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead) && (self.ability_with_code(h, "Atwa").is_some() || self.ability_with_code(h, "Atdg").is_some() || self.ability_with_code(h, "Atsp").is_some())).collect();
        for h in tornadoes {
            let Some(u) = self.unit(h) else { continue };
            let (x, y) = (u.x, u.y);
            if u.extra.home.is_none() {
                self.unit_mut(h).unwrap().extra.home = Some((x, y));
            }
            let (hx, hy) = self.unit(h).and_then(|u| u.extra.home).unwrap_or((x, y));
            if let Some((a, lvl)) = self.ability_with_code(h, "Atwa") {
                let lvl = lvl.max(1) as u32;
                let left = self.unit(h).and_then(|u| u.cooldowns.get(&a).copied()).unwrap_or(0.0);
                if left <= 0.0 {
                    let r = self.data(a, "Area", lvl);
                    let (ang, d) = (self.rng.real(0.0, std::f32::consts::TAU), self.rng.real(0.0, r));
                    let every = self.data(a, "Dur", lvl).max(0.1);
                    let u = self.unit_mut(h).unwrap();
                    u.action = Action::Move { x: hx + d * ang.cos(), y: hy + d * ang.sin() };
                    u.path.clear();
                    u.cooldowns.insert(a, every);
                } else {
                    self.unit_mut(h).unwrap().cooldowns.insert(a, left);
                }
            }
            if let Some((a, lvl)) = self.ability_with_code(h, "Atdg") {
                let lvl = lvl.max(1) as u32;
                let d = |k: &str| self.data(a, k, lvl);
                let (area, dps, mid_r, mid, small_r, small) = (d("Area"), d("DataA"), d("DataB"), d("DataC"), d("DataD"), d("DataE"));
                // A second's worth once a second, the first a second after it appears
                // (measured: a farm 300 from a new tornado takes 7 at 1.00 seconds and
                // each second after).
                let Some(u) = self.unit_mut(h) else { continue };
                let left = u.extra.pulse_in.unwrap_or(1.0) - dt;
                u.extra.pulse_in = Some(if left > 0.0 { left } else { left + 1.0 });
                if left > 0.0 {
                    continue;
                }
                for t in self.units_in_pub(x, y, area) {
                    if t == h || self.cast_target_error(h, a, t).is_some() {
                        continue;
                    }
                    let dist = self.unit(t).map_or(f32::MAX, |u| (u.x - x).hypot(u.y - y));
                    let rate = if small_r > 0.0 && dist <= small_r { small } else if dist <= mid_r { mid } else { dps };
                    self.spell_damage(vm, h, t, rate);
                }
            }
            if let Some((a, lvl)) = self.ability_with_code(h, "Atsp") {
                let lvl = lvl.max(1) as u32;
                let (area, again) = (self.data(a, "Area", lvl), self.data(a, "DataB", lvl));
                let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
                let now = self.time;
                for t in self.units_in_pub(x, y, area) {
                    if t == h || self.cast_target_error(h, a, t).is_some() || self.unit(t).is_some_and(|u| u.is_building || u.extra.spun_at > now - again) {
                        continue;
                    }
                    let dur = if self.hero_duration(t) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, invulnerable: true, ..Default::default() });
                    if let Some(u) = self.unit_mut(t) {
                        u.extra.spun_at = now + dur;
                    }
                }
            }
        }
    }
}

impl World {
    /// Pocket Factories: every DataA a goblin (DataB) for DataC seconds, DataD in front.
    pub fn update_factories(&mut self, dt: f32) {
        let factories: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.extra.factory.is_some())).collect();
        for h in factories {
            let Some(u) = self.unit(h) else { continue };
            let Some((a, lvl, left)) = u.extra.factory else { continue };
            let (owner, x, y, facing) = (u.owner, u.x, u.y, u.facing.to_radians());
            let left = left - dt;
            if left > 0.0 {
                self.unit_mut(h).unwrap().extra.factory = Some((a, lvl, left));
                continue;
            }
            let every = self.data(a, "DataA", lvl).max(0.1);
            self.unit_mut(h).unwrap().extra.factory = Some((a, lvl, left + every));
            let kind = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("DataB", lvl)).unwrap_or_default().trim());
            let (life, off) = (self.data(a, "DataC", lvl), self.data(a, "DataD", lvl));
            self.summon(h, owner, kind, 1, x + off * facing.cos(), y + off * facing.sin(), life);
        }
    }
}
