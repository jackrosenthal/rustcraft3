//! What some units do as they die, and the goblin land mine: Death Damage
//! (CAbilityDeathDamageAoe, Adda) deals DataB within DataA and DataD within DataC to
//! what it allows; Spawn Spiderling and Spawn Spider (CAbilityDeathSpawn, Assp and
//! Aspd) leave DataA of DataB.
//! A land mine (CAbilityLandMine, Amin) arms DataA after it is made, unseen, and once
//! an enemy it allows comes within Rng it shows itself and goes off 0.1 seconds later.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// Incinerated units bursting when their delay is up.
    pub(crate) fn update_incinerations(&mut self, vm: &mut Vm) {
        let now = self.time;
        let due: Vec<_> = self.incinerations.iter().copied().filter(|e| e.0 <= now).collect();
        self.incinerations.retain(|e| e.0 > now);
        for (_, caster, a, lvl, buff, x, y) in due {
            let art = self.objects.buffs.get(buff).map(|o| o.str("Specialart")).unwrap_or_default();
            self.spawn_effect(&art, x, y);
            let (full, full_r, half, half_r) = (self.data(a, "DataB", lvl), self.data(a, "DataC", lvl), self.data(a, "DataD", lvl), self.data(a, "DataE", lvl));
            for t in self.units_in_pub(x, y, full_r.max(half_r)) {
                if self.unit(t).is_none_or(|u| u.dead) || self.cast_target_error(caster, a, t).is_some() {
                    continue;
                }
                let d = self.unit(t).map_or(f32::MAX, |u| (u.x - x).hypot(u.y - y));
                let amount = if d <= full_r { full } else { half };
                self.spell_damage(vm, caster, t, amount);
            }
        }
    }

    /// Death abilities of `h`, which is dying (`killer` its killer).
    pub(crate) fn death_abilities(&mut self, vm: &mut Vm, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let (owner, x, y) = (u.owner, u.x, u.y);
        if let Some((a, lvl)) = self.ability_with_code(h, "Adda") {
            let lvl = lvl.max(1) as u32;
            let (full_r, full, part_r, part) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl), self.data(a, "DataC", lvl), self.data(a, "DataD", lvl));
            for t in self.units_in_pub(x, y, full_r.max(part_r)) {
                if t == h || self.cast_target_error(h, a, t).is_some() {
                    continue;
                }
                let d = self.unit(t).map_or(f32::MAX, |u| (u.x - x).hypot(u.y - y));
                let amount = if d <= full_r { full } else { part };
                self.spell_damage(vm, h, t, amount);
            }
        }
        // A unit dying under Incinerate bursts DataF later: DataB to what the spell allows
        // within DataC and DataD within DataE, with the buff's Specialart.
        let incinerated = self.unit(h).and_then(|u| u.buffs.iter().find(|b| matches!(self.ability_code(b.ability).as_str(), "ANia" | "ANic")).map(|b| (b.ability, b.level, b.caster, b.id)));
        if let Some((a, lvl, caster, buff)) = incinerated {
            let delay = self.data(a, "DataF", lvl);
            self.incinerations.push((self.time + delay, caster, a, lvl, buff, x, y));
        }
        // A unit dying with Parasite on it gives the parasite's caster DataE of UnitID
        // for Cast seconds.
        let parasite = self.unit(h).and_then(|u| u.buffs.iter().find(|b| self.ability_code(b.ability) == "ANpa").map(|b| (b.ability, b.level, b.caster)));
        if let Some((a, lvl, caster)) = parasite {
            let n = self.data(a, "DataE", lvl) as i32;
            let t = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
            let life = self.data(a, "Cast", lvl);
            if let Some(o) = self.unit(caster).map(|c| c.owner) {
                self.summon(caster, o, t, n, x, y, life);
            }
        }
        // Spawn Spiderling and Spawn Spider (a hydra's young), each it has.
        let spawns: Vec<(Id, i32)> = self.unit(h).map(|u| u.abilities.iter().copied().filter(|&(a, _)| matches!(self.ability_code(a).as_str(), "Assp" | "Aspd")).collect()).unwrap_or_default();
        for (a, lvl) in spawns {
            let lvl = lvl.max(1) as u32;
            let n = self.data(a, "DataA", lvl) as i32;
            let t = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("DataB", lvl)).unwrap_or_default().trim());
            for k in 0..n.max(0) {
                let ang = k as f32 / n.max(1) as f32 * std::f32::consts::TAU;
                crate::natives::units::create_unit(self, owner, t, x + 32.0 * ang.cos(), y + 32.0 * ang.sin(), 270.0);
            }
        }
    }

    /// Whether a land mine has armed (and so is unseen).
    pub fn mine_armed(&self, u: &Unit) -> bool {
        self.ability_with_code(u.handle, "Amin").is_some_and(|(a, lvl)| self.time - u.extra.born >= self.data(a, "DataA", lvl.max(1) as u32) && u.extra.mine_fuse.is_none())
    }

    pub fn update_mines_armed(&mut self, vm: &mut Vm) {
        let mines: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead) && self.ability_with_code(h, "Amin").is_some()).collect();
        for h in mines {
            let Some(u) = self.unit(h) else { continue };
            if let Some(at) = u.extra.mine_fuse {
                if self.time >= at {
                    crate::natives::units::kill_unit(self, vm, h, h);
                }
                continue;
            }
            if !self.mine_armed(u) {
                continue;
            }
            let (a, lvl) = self.ability_with_code(h, "Amin").unwrap();
            let range = self.data(a, "Rng", lvl.max(1) as u32);
            let (x, y) = (u.x, u.y);
            let set_off = self.units_in_pub(x, y, range).into_iter().any(|t| t != h && self.cast_target_error(h, a, t).is_none() && self.move_kind(t) != crate::pathing::Move::Fly);
            if set_off {
                let at = self.time + 0.1;
                self.unit_mut(h).unwrap().extra.mine_fuse = Some(at);
            }
        }
    }
}
