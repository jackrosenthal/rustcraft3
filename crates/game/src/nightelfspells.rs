//! Night Elf unit abilities: Mount Hippogryph and Dismount, Detonate, Eat Tree,
//! Elune's Grace (AIdd), Mana Flare, Phase Shift, and Vengeance.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// Mount Hippogryph: an archer and a hippogryph that meet become a rider (UnitID).
    pub(crate) fn couple(&mut self, h: u32, a: Id, lvl: u32, partner: u32) -> bool {
        let Some(o) = self.objects.abilities.get(a) else { return false };
        let (partner_type, rider) = (id_from_str(o.level_str("DataA", lvl).trim()), id_from_str(o.level_str("UnitID", lvl).trim()));
        let (Some(u), Some(p)) = (self.unit(h), self.unit(partner)) else { return false };
        if p.type_id != partner_type || p.owner != u.owner || p.dead || u.dead {
            return false;
        }
        // The rider rises where the hippogryph is, with the pair's share of life left.
        let hippogryph = if self.objects.units.get(u.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly")) { h } else { partner };
        let (x, y, facing) = self.unit(hippogryph).map(|g| (g.x, g.y, g.facing)).unwrap_or_default();
        let share = (u.life / u.max_life.max(1.0) + p.life / p.max_life.max(1.0)) / 2.0;
        let owner = u.owner;
        crate::natives::units::remove_unit(self, h);
        crate::natives::units::remove_unit(self, partner);
        let r = crate::natives::units::create_unit(self, owner, rider, x, y, facing);
        if let Some(ru) = self.unit_mut(r) {
            ru.life = (ru.max_life * share).max(1.0);
        }
        self.update_food();
        true
    }

    /// Dismount: a rider becomes its archer (DataA) and hippogryph (DataB).
    pub(crate) fn decouple(&mut self, h: u32, a: Id, lvl: u32) {
        let Some(o) = self.objects.abilities.get(a) else { return };
        let (archer, hippogryph) = (id_from_str(o.level_str("DataA", lvl).trim()), id_from_str(o.level_str("DataB", lvl).trim()));
        let Some(u) = self.unit(h) else { return };
        let (owner, x, y, facing, share) = (u.owner, u.x, u.y, u.facing, u.life / u.max_life.max(1.0));
        crate::natives::units::remove_unit(self, h);
        for t in [archer, hippogryph] {
            let n = crate::natives::units::create_unit(self, owner, t, x, y, facing);
            if let Some(nu) = self.unit_mut(n) {
                nu.life = (nu.max_life * share).max(1.0);
            }
        }
        self.update_food();
    }

    /// Detonate: the wisp bursts: units within Area lose DataA mana and their magic,
    /// summoned ones taking DataB damage.
    pub(crate) fn detonate(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, x: f32, y: f32) {
        let (mana, summon_damage, area) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl), self.data(a, "Area", lvl));
        for t in self.units_in_pub(x, y, area) {
            if t == h {
                continue;
            }
            if let Some(u) = self.unit_mut(t) {
                u.mana = (u.mana - mana).max(0.0);
            }
            self.dispel_pub(vm, h, t, summon_damage);
        }
        crate::natives::units::kill_unit(self, vm, h, h);
    }

    /// Eat Tree: the tree goes, and the ancient regains DataC life over Dur.
    pub(crate) fn eat_tree(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, tree: u32) {
        if !matches!(self.handles.get(tree), Some(Obj::Destructable(d)) if d.life > 0.0) {
            return;
        }
        crate::natives::objects::kill_destructable_pub(self, vm, tree);
        let (life, dur) = (self.data(a, "DataC", lvl), self.data(a, "Dur", lvl).max(0.1));
        let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
        self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, life_regen: life / dur, ..Default::default() });
    }

    /// War Club: the tree comes up and the giant fights with it: only attack index
    /// DataD (not DataC) for DataE attacks.
    pub(crate) fn grab_tree(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, tree: u32) {
        if !matches!(self.handles.get(tree), Some(Obj::Destructable(d)) if d.life > 0.0) {
            return;
        }
        crate::natives::objects::kill_destructable_pub(self, vm, tree);
        let (index, attacks) = (self.data(a, "DataD", lvl) as u32, self.data(a, "DataE", lvl) as i32);
        let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
        self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::MAX, ..Default::default() });
        if let Some(u) = self.unit_mut(h) {
            u.extra.weapons = Some(1 << index.min(1));
            u.extra.club = Some((a, attacks.max(1)));
        }
    }

    /// One War Club attack spent; the last drops the tree.
    pub(crate) fn club_swing(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        let Some((a, left)) = u.extra.club else { return };
        if left > 1 {
            u.extra.club = Some((a, left - 1));
            return;
        }
        u.extra.club = None;
        u.extra.weapons = None;
        for b in u.buffs.iter_mut().filter(|b| b.ability == a) {
            b.remaining = 0.0;
        }
    }

    /// Elune's Grace (AIdd): the damage a unit takes, piercing scaled by DataA and
    /// spells by DataE.
    pub fn elunes_grace(&self, target: u32, attack_type: &str, dmg: f32) -> f32 {
        let Some((a, lvl)) = self.ability_with_code(target, "AIdd") else { return dmg };
        let owner = self.unit(target).map_or(0, |u| u.owner);
        if !self.ability_enabled(owner, a) {
            return dmg;
        }
        let lvl = lvl.max(1) as u32;
        match attack_type {
            "pierce" => dmg * self.data(a, "DataA", lvl),
            "spells" | "magic" => dmg * self.data(a, "DataE", lvl),
            _ => dmg,
        }
    }

    /// Mana Flare: an enemy casting within Area of a flaring caster takes DataA damage
    /// per mana point (DataB for Heroes), at most DataC (DataD), each unit at most
    /// once every DataE seconds.
    pub(crate) fn mana_flare(&mut self, vm: &mut Vm, caster: u32, cost: f32) {
        let Some((owner, x, y, hero)) = self.unit(caster).map(|u| (u.owner, u.x, u.y, u.is_hero)) else { return };
        let flarers: Vec<(u32, Id, u32)> = self
            .unit_order
            .iter()
            .copied()
            .filter_map(|f| {
                let u = self.unit(f)?;
                let b = u.buffs.iter().find(|b| self.ability_code(b.ability) == "Amfl" && b.caster == f)?;
                (!u.dead && !crate::natives::players::is_ally(self, u.owner, owner) && (u.x - x).hypot(u.y - y) <= self.data(b.ability, "Area", b.level)).then_some((f, b.ability, b.level))
            })
            .collect();
        for (f, a, lvl) in flarers {
            let key = (f, caster);
            if self.flare_cooldowns.get(&key).is_some_and(|&t| t > self.time) {
                continue;
            }
            self.flare_cooldowns.insert(key, self.time + self.data(a, "DataE", lvl));
            let (per, max) = if hero { (self.data(a, "DataB", lvl), self.data(a, "DataD", lvl)) } else { (self.data(a, "DataA", lvl), self.data(a, "DataC", lvl)) };
            self.spell_damage(vm, f, caster, (cost * per).min(max));
        }
    }

    /// Phase Shift: while its autocast is on and mana lasts, a unit about to be hit
    /// shifts out for Dur instead, every Cool.  True when it did.
    pub(crate) fn phase_shift(&mut self, target: u32) -> bool {
        let Some((a, lvl)) = self.ability_with_code(target, "Apsh") else { return false };
        let lvl = lvl.max(1) as u32;
        let Some(u) = self.unit(target) else { return false };
        let cost = self.data(a, "Cost", lvl);
        if !u.autocast.contains(&a) || u.mana < cost || u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0 || !self.ability_enabled(u.owner, a) {
            return false;
        }
        let (dur, cool) = (self.data(a, "Dur", lvl), self.data(a, "Cool", lvl));
        let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
        let u = self.unit_mut(target).unwrap();
        u.mana -= cost;
        u.cooldowns.insert(a, cool);
        self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: target, remaining: dur, invulnerable: true, ..Default::default() });
        true
    }

    /// Vengeance: a corpse rises as a spirit (DataC) for Dur, while the caster has fewer
    /// than DataE; they die with the caster (DataF 1).
    pub(crate) fn vengeance(&mut self, h: u32, a: Id, lvl: u32, corpse: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let spirit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("DataC", lvl)).unwrap_or_default().trim());
        let max = self.data(a, "DataE", lvl) as usize;
        let have = self.unit_order.iter().filter(|&&s| self.unit(s).is_some_and(|u| !u.dead && u.type_id == spirit && u.extra.summoner == h)).count();
        if max > 0 && have >= max {
            return;
        }
        let Some((x, y)) = self.unit(corpse).filter(|c| c.dead).map(|c| (c.x, c.y)) else { return };
        crate::natives::units::remove_unit(self, corpse);
        let life = self.data(a, "Dur", lvl);
        for s in self.summon(h, owner, spirit, 1, x, y, life) {
            if let Some(su) = self.unit_mut(s) {
                su.extra.summoner = h;
            }
        }
    }
}
