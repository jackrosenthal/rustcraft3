//! Morphs (Bear Form, Raven Form, Stone Form, Burrow, Metamorphosis, Chemical Rage,
//! Robo-Goblin, Ethereal and Corporeal Form, Destroyer Form): a unit of the ability's
//! DataA type turns into its UnitID type and back, taking Dur seconds to change; a hero's
//! form with a HeroDur turns back when that runs out.

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

const MORPHS: [&str; 9] = ["Abrf", "Arav", "Astn", "Abur", "AEme", "ANcr", "ANrg", "Acpf", "Aetf"];

impl World {
    /// The morph ability a unit has whose order or unorder is `name`, and the type it
    /// turns into.
    fn morph_for_order(&self, h: u32, name: &str) -> Option<(Id, u32, Id)> {
        let u = self.unit(h)?;
        u.abilities.iter().find_map(|&(a, lvl)| {
            let o = self.objects.abilities.get(a)?;
            if !MORPHS.contains(&o.str("code").as_str()) {
                return None;
            }
            let lvl = lvl.max(1) as u32;
            let (normal, alternate) = (id_from_str(o.level_str("DataA", lvl).trim()), id_from_str(o.level_str("UnitID", lvl).trim()));
            let to = if o.str("Order").eq_ignore_ascii_case(name) && u.type_id == normal {
                alternate
            } else if o.str("Unorder").eq_ignore_ascii_case(name) && u.type_id == alternate {
                normal
            } else {
                return None;
            };
            Some((a, lvl, to))
        })
    }

    /// Starts a morph ordered by name; true when the order was one.
    pub fn morph_order(&mut self, h: u32, name: &str) -> bool {
        let Some((a, lvl, to)) = self.morph_for_order(h, name) else { return false };
        // Turning back costs nothing and waits on no cooldown.
        let back_to_normal = self.objects.abilities.get(a).is_some_and(|o| id_from_str(o.level_str("DataA", lvl).trim()) == to);
        if self.unit(h).is_none_or(|u| u.dead || u.extra.morph.is_some()) || (!back_to_normal && self.can_cast(h, a).is_err()) {
            return false;
        }
        let o = self.objects.abilities.get(a);
        let dur = o.map_or(0.0, |o| o.level_real("Dur", lvl));
        let cost = o.map_or(0.0, |o| o.level_real("Cost", lvl));
        let cool = o.map_or(0.0, |o| o.level_real("Cool", lvl));
        let back = self.unit(h).unwrap().type_id;
        // A hero's timed form (HeroDur) turns back on its own.
        let limit = o.map_or(0.0, |o| o.level_real("HeroDur", lvl));
        let timed = self.unit(h).is_some_and(|u| u.is_hero) && limit > 0.0 && o.is_some_and(|o| o.str("Unorder").is_empty());
        let u = self.unit_mut(h).unwrap();
        if !back_to_normal {
            u.mana -= cost;
            u.cooldowns.insert(a, cool);
        }
        u.action = Action::Idle;
        u.path.clear();
        u.swing = None;
        u.extra.morph = Some((dur, to, if timed { Some((limit, back)) } else { None }));
        crate::sim::set_anim(u, "morph");
        true
    }

    pub fn update_morphs(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && (u.extra.morph.is_some() || u.extra.morph_revert.is_some()))).collect();
        for h in units {
            if let Some((left, to, timed)) = self.unit(h).and_then(|u| u.extra.morph) {
                let left = left - dt;
                if left > 0.0 {
                    self.unit_mut(h).unwrap().extra.morph = Some((left, to, timed));
                    continue;
                }
                self.unit_mut(h).unwrap().extra.morph = None;
                self.change_type(h, to);
                if let Some(u) = self.unit_mut(h) {
                    u.extra.morph_revert = timed;
                    crate::sim::set_anim(u, "stand");
                }
                continue;
            }
            if let Some((left, back)) = self.unit(h).and_then(|u| u.extra.morph_revert) {
                let left = left - dt;
                if left > 0.0 {
                    self.unit_mut(h).unwrap().extra.morph_revert = Some((left, back));
                    continue;
                }
                let u = self.unit_mut(h).unwrap();
                u.extra.morph_revert = None;
                u.extra.morph = Some((0.0, back, None));
                crate::sim::set_anim(u, "morph");
            }
        }
    }

    /// Turns a unit into another type: its life and mana keep their fractions, it takes
    /// the new type's stats, movement, weapons, and abilities, and keeps learned hero
    /// skills and the levels of abilities both types have.
    pub fn change_type(&mut self, h: u32, to: Id) {
        let Some(o) = self.objects.units.get(to) else { return };
        let hp = o.real("HP").max(1.0);
        let mana_max = o.real("manaN");
        let new_abils: Vec<Id> = o.list("abilList").iter().map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect();
        let hero_abils: Vec<Id> = o.list("heroAbilList").iter().map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect();
        let (move_speed, turn, prop, coll, scale, height, acquire) = (o.real("spd"), o.real("turnRate"), o.real("propWin"), o.real("collision"), o.real("modelScale").max(0.0001), o.real("moveHeight"), o.real("acquire"));
        let old_hero: Vec<Id> = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map(|o| o.list("heroAbilList").iter().map(|s| id_from_str(s.trim())).collect()).unwrap_or_default();
        let Some(u) = self.unit_mut(h) else { return };
        let life_frac = if u.max_life > 0.0 { u.life / u.max_life } else { 1.0 };
        let mana_frac = if u.max_mana > 0.0 { u.mana / u.max_mana } else { 1.0 };
        u.type_id = to;
        u.max_life = hp;
        u.life = (hp * life_frac).max(1.0);
        u.max_mana = mana_max;
        u.mana = mana_max * mana_frac;
        u.move_speed = move_speed;
        u.turn_speed = turn;
        u.prop_window = prop;
        u.collision = coll;
        u.scale = scale;
        u.fly_height = height;
        u.acquire_range = acquire;
        let mut abilities: Vec<(Id, i32)> = u.abilities.iter().copied().filter(|(a, _)| new_abils.contains(a) || hero_abils.contains(a) || old_hero.contains(a)).collect();
        for a in new_abils {
            if !abilities.iter().any(|x| x.0 == a) {
                abilities.push((a, 1));
            }
        }
        u.abilities = abilities;
        u.extra.weapons = None;
    }
}
