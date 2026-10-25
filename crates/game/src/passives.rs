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

    /// Attack passives of the attacker: Pulverize, Slow Poison, Corrosive Breath, and
    /// Kaboom.
    pub fn attack_passives(&mut self, vm: &mut Vm, h: u32, target: u32) {
        let Some(u) = self.unit(h) else { return };
        let (owner, ux, uy) = (u.owner, u.x, u.y);
        let (tx, ty, building) = self.unit(target).map_or((ux, uy, false), |t| (t.x, t.y, t.is_building));
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
        // Slow Poison: DataA a second and DataB slower movement, DataC slower attacks.
        if let Some((a, l)) = self.passive(h, "Aspo").filter(|_| !building) {
            let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", l)).unwrap_or_default();
            let id = id_from_str(buff.split(',').next().unwrap_or("").trim());
            let dur = if self.hero_duration(target) { self.data(a, "HeroDur", l) } else { self.data(a, "Dur", l) };
            let b = Buff { id, ability: a, level: l, caster: h, remaining: dur, dps: self.data(a, "DataA", l), move_factor: -self.data(a, "DataB", l), attack_factor: -self.data(a, "DataC", l), ..Default::default() };
            self.apply_buff(target, b);
        }
        // Corrosive Breath: structures hit take DataA a second.
        if let Some((a, l)) = self.passive(h, "Acor").filter(|_| building) {
            let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", l)).unwrap_or_default();
            let id = id_from_str(buff.split(',').next().unwrap_or("").trim());
            let b = Buff { id, ability: a, level: l, caster: h, remaining: self.data(a, "Dur", l), dps: self.data(a, "DataA", l), ..Default::default() };
            self.apply_buff(target, b);
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
    /// Wander (CAbilityWander): an idle wanderer waits 5 to 10 seconds (vf189), then
    /// walks to a point up to 250 away on each axis (vf190).
    pub fn update_wander(&mut self, _dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && !u.paused && matches!(u.action, Action::Idle))).collect();
        for h in units {
            let Some((a, _)) = self.ability_with_code(h, "Awan") else { continue };
            let left = self.unit(h).and_then(|u| u.cooldowns.get(&a).copied());
            // Ability cooldowns count down with the rest (update_buffs).
            let left = match left {
                Some(t) => t,
                None => self.rng.real(5.0, 10.0),
            };
            if left > 0.0 {
                self.unit_mut(h).unwrap().cooldowns.insert(a, left);
                continue;
            }
            let next = self.rng.real(5.0, 10.0);
            let (dx, dy) = (self.rng.real(0.0, 500.0) - 250.0, self.rng.real(0.0, 500.0) - 250.0);
            let u = self.unit_mut(h).unwrap();
            u.cooldowns.insert(a, next);
            let (x, y) = (u.x + dx, u.y + dy);
            u.action = Action::Move { x, y };
            u.path.clear();
            u.repath_timer = 0.0;
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
    /// defense (rart).  CAbilitySpiked and CAbilityReinforce share it.
    pub fn update_upgrade_art(&mut self) {
        let mut changes: Vec<(u32, Id, bool)> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            for &(a, _) in &u.abilities {
                let code = self.ability_code(a);
                if !matches!(code.as_str(), "Aspi" | "Arbr") {
                    continue;
                }
                let bonus = self.upgrade_bonus(u.owner, u.type_id);
                let want = !u.dead && if code == "Aspi" { bonus.spikes > 0.0 } else { bonus.armor_type > 0 };
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
                let models = o.list("Targetart");
                let n = o.str("Targetattachcount").trim().parse::<usize>().unwrap_or(1).max(1);
                let mut art = Vec::new();
                for k in 0..n {
                    let key = if k == 0 { "Targetattach".to_string() } else { format!("Targetattach{k}") };
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
    /// where it appeared every Dur.  Tornado Damage (Atdg): structures it allows take
    /// DataA a second within Area, DataC within DataB, and DataE within DataD.  Tornado
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
                for t in self.units_in_pub(x, y, area) {
                    if t == h || self.cast_target_error(h, a, t).is_some() {
                        continue;
                    }
                    let dist = self.unit(t).map_or(f32::MAX, |u| (u.x - x).hypot(u.y - y));
                    let rate = if small_r > 0.0 && dist <= small_r { small } else if dist <= mid_r { mid } else { dps };
                    self.spell_damage(vm, h, t, rate * dt);
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
