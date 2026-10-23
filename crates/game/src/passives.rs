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
        u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Apiv")
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
        u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Ault")
    }

    /// Hardened Skin (Assk): a DataA% chance to take DataC less damage, but at least
    /// DataB, from ranged (DataD) and melee (DataE) attacks.
    pub fn hardened_skin(&mut self, target: u32, melee: bool, dmg: f32) -> f32 {
        let Some((a, l)) = self.passive(target, "Assk") else { return dmg };
        let applies = if melee { self.data(a, "DataE", l) != 0.0 } else { self.data(a, "DataD", l) != 0.0 };
        if !applies || (self.rng.int(1, 100) as f32) > self.data(a, "DataA", l) {
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
            if (self.rng.int(1, 100) as f32) <= self.data(a, "DataA", l) {
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
