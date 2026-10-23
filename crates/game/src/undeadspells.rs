//! Undead unit abilities: Disease Cloud, Possession, Replenish, the meat wagon's
//! corpses (Get Corpse, Drop All Corpses, Exhume Corpses), and the Graveyard's.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// Disease Cloud (Aapl): what the ability allows within Area of its carrier catches
    /// the disease, DataB a second for DataA after leaving it.  Replenish (Arpl, Arpm)
    /// while its autocast is on: every Cool, up to Cast friends within Area short of
    /// life (or mana) gain DataA life (DataB mana), the caster paying Cost for each of
    /// up to DataE of them.
    pub fn update_undead_auras(&mut self, vm: &mut Vm, dt: f32) {
        let _ = vm;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden)).collect();
        for h in units {
            for code in ["Aapl", "Arpl", "Arpm"] {
                let Some((a, lvl)) = self.ability_with_code(h, code) else { continue };
                let lvl = lvl.max(1) as u32;
                let Some(u) = self.unit(h) else { continue };
                let (owner, ux, uy) = (u.owner, u.x, u.y);
                if !self.ability_enabled(owner, a) || (code != "Aapl" && !u.autocast.contains(&a)) {
                    continue;
                }
                let left = u.cooldowns.get(&a).copied().unwrap_or(0.0) - dt;
                if left > 0.0 {
                    self.unit_mut(h).unwrap().cooldowns.insert(a, left);
                    continue;
                }
                let area = self.data(a, "Area", lvl);
                let buffs: Vec<Id> = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').map(|b| id_from_str(b.trim())).filter(|b| b[0] != 0).collect();
                if code == "Aapl" {
                    self.unit_mut(h).unwrap().cooldowns.insert(a, 0.5);
                    let (dps, last) = (self.data(a, "DataB", lvl), self.data(a, "DataA", lvl));
                    for t in self.units_in_pub(ux, uy, area) {
                        if t == h || self.cast_target_error(h, a, t).is_some() {
                            continue;
                        }
                        self.apply_buff(t, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: h, remaining: last, dps, ..Default::default() });
                    }
                    continue;
                }
                let life = code == "Arpl";
                let (gain, max, charged, cost) = (self.data(a, if life { "DataA" } else { "DataB" }, lvl), self.data(a, "Cast", lvl).max(1.0) as usize, self.data(a, "DataE", lvl) as usize, self.data(a, "Cost", lvl));
                let mut needy: Vec<u32> = self
                    .units_in_pub(ux, uy, area)
                    .into_iter()
                    .filter(|&t| self.cast_target_error(h, a, t).is_none())
                    .filter(|&t| self.unit(t).is_some_and(|tu| if life { tu.life < tu.max_life } else { tu.max_mana > 0.0 && tu.mana < tu.max_mana }))
                    .collect();
                needy.truncate(max);
                if needy.is_empty() {
                    continue;
                }
                let pay = cost * needy.len().min(charged.max(1)) as f32;
                if self.unit(h).is_none_or(|u| u.mana < pay) {
                    continue;
                }
                let cool = self.data(a, "Cool", lvl);
                let u = self.unit_mut(h).unwrap();
                u.mana -= pay;
                u.cooldowns.insert(a, cool);
                for t in needy {
                    if let Some(tu) = self.unit_mut(t) {
                        if life {
                            tu.life = (tu.life + gain).min(tu.max_life);
                        } else {
                            tu.mana = (tu.mana + gain).min(tu.max_mana);
                        }
                    }
                }
            }
        }
    }

    /// Possession begins: the target takes DataB times damage while the Banshee
    /// channels for Dur (and is invulnerable when DataC is 1).
    pub(crate) fn begin_possession(&mut self, h: u32, a: Id, lvl: u32, target: u32) {
        let dur = self.data(a, "Dur", lvl);
        let buffs: Vec<Id> = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').map(|b| id_from_str(b.trim())).filter(|b| b[0] != 0).collect();
        let amp = self.data(a, "DataB", lvl);
        let invulnerable = self.data(a, "DataC", lvl) == 1.0;
        self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: h, remaining: dur, damage_taken: amp - 1.0, invulnerable, stun: true, ..Default::default() });
        if let Some(u) = self.unit_mut(h) {
            u.channel = Some(Channel { ability: a, level: lvl, target, x: 0.0, y: 0.0, elapsed: 0.0, duration: dur, interval: dur + 1.0, next: dur + 1.0, waves: 0, link: 0, held: buffs.first().copied().unwrap_or_default() });
        }
    }

    /// A finished Possession: the target becomes the caster's, and the Banshee is spent.
    pub(crate) fn finish_possession(&mut self, vm: &mut Vm, h: u32, target: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let color = self.players.get(owner as usize).map_or(0, |p| p.color);
        let prev = self.unit(target).map_or(owner, |t| t.owner);
        if let Some(t) = self.unit_mut(target) {
            if t.dead {
                return;
            }
            t.owner = owner;
            t.color = color;
            t.action = Action::Idle;
        }
        self.update_food();
        use crate::event_ids::*;
        let ctx = EventCtx { state: prev as i32, ..Default::default() };
        self.fire_unit_pair(vm, target, EVENT_PLAYER_UNIT_CHANGE_OWNER, EVENT_UNIT_CHANGE_OWNER, ctx);
        crate::natives::units::remove_unit(self, h);
    }

    /// The corpses a meat wagon holds at most: its cargo hold's (Amtc) DataA.
    fn corpse_capacity(&self, h: u32) -> usize {
        self.ability_with_code(h, "Amtc").map_or(0, |(a, lvl)| self.data(a, "DataA", lvl.max(1) as u32) as usize)
    }

    /// Get Corpse: the wagon takes up a corpse.
    pub(crate) fn load_corpse(&mut self, h: u32, corpse: u32) {
        let Some(c) = self.unit(corpse).filter(|c| c.dead && !c.hidden && !c.is_hero) else { return };
        let type_id = c.type_id;
        if self.unit(h).is_none_or(|u| u.extra.corpses.len() >= self.corpse_capacity(h)) {
            return;
        }
        crate::natives::units::remove_unit(self, corpse);
        if let Some(u) = self.unit_mut(h) {
            u.extra.corpses.push(type_id);
        }
    }

    fn place_corpse(&mut self, owner: u8, type_id: Id, x: f32, y: f32) {
        let facing = self.rng.real(0.0, 360.0);
        let c = crate::natives::units::create_unit(self, owner, type_id, x, y, facing);
        if let Some(u) = self.unit_mut(c) {
            u.dead = true;
            u.life = 0.0;
        }
    }

    /// Drop All Corpses: the wagon's corpses fall around it.
    pub(crate) fn drop_corpses(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        let corpses = std::mem::take(&mut u.extra.corpses);
        let (owner, x, y) = (u.owner, u.x, u.y);
        for t in corpses {
            let (dx, dy) = (self.rng.real(-48.0, 48.0), self.rng.real(-48.0, 48.0));
            self.place_corpse(owner, t, x + dx, y + dy);
        }
    }

    /// Exhume Corpses: a wagon with fewer than DataA corpses gains one of UnitID every
    /// Dur.  The Graveyard: every Cool, while fewer than DataA corpses lie within
    /// DataC, one of UnitID appears within DataB.
    pub fn update_corpse_makers(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && u.constructing.is_none())).collect();
        for h in units {
            for code in ["Aexh", "Agyd"] {
                let Some((a, lvl)) = self.ability_with_code(h, code) else { continue };
                let lvl = lvl.max(1) as u32;
                let Some(u) = self.unit(h) else { continue };
                let (owner, x, y, held) = (u.owner, u.x, u.y, u.extra.corpses.len());
                if !self.ability_enabled(owner, a) {
                    continue;
                }
                let max = self.data(a, "DataA", lvl) as usize;
                let full = if code == "Aexh" { held >= max.min(self.corpse_capacity(h)) } else { self.corpses_in(x, y, self.data(a, "DataC", lvl)).len() >= max };
                let period = self.data(a, if code == "Aexh" { "Dur" } else { "Cool" }, lvl);
                let u = self.unit_mut(h).unwrap();
                if full {
                    u.cooldowns.insert(a, period);
                    continue;
                }
                let left = u.cooldowns.get(&a).copied().unwrap_or(period) - dt;
                if left > 0.0 {
                    u.cooldowns.insert(a, left);
                    continue;
                }
                u.cooldowns.insert(a, period);
                let type_id = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                if code == "Aexh" {
                    self.unit_mut(h).unwrap().extra.corpses.push(type_id);
                } else {
                    let r = self.data(a, "DataB", lvl);
                    let ang = self.rng.real(0.0, std::f32::consts::TAU);
                    let d = self.rng.real(0.0, r);
                    self.place_corpse(owner, type_id, x + d * ang.cos(), y + d * ang.sin());
                }
            }
        }
    }
}

impl World {
    /// Permanent Immolation (ANpi, and the goblin's Apig): the unit burns what its
    /// ability allows within Area for DataA a second, its buff showing the flames.
    pub fn update_permanent_immolation(&mut self, vm: &mut Vm, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden)).collect();
        for h in units {
            for code in ["ANpi", "Apig"] {
                let Some((a, lvl)) = self.ability_with_code(h, code) else { continue };
                let lvl = lvl.max(1) as u32;
                let Some(u) = self.unit(h) else { continue };
                let (owner, x, y) = (u.owner, u.x, u.y);
                if !self.ability_enabled(owner, a) {
                    continue;
                }
                if !u.buffs.iter().any(|b| b.ability == a) {
                    let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
                    self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::MAX, ..Default::default() });
                }
                let left = self.unit(h).and_then(|u| u.cooldowns.get(&a).copied()).unwrap_or(0.0) - dt;
                if left > 0.0 {
                    self.unit_mut(h).unwrap().cooldowns.insert(a, left);
                    continue;
                }
                self.unit_mut(h).unwrap().cooldowns.insert(a, left + 1.0);
                let (area, dps) = (self.data(a, "Area", lvl), self.data(a, "DataA", lvl));
                for t in self.units_in_pub(x, y, area) {
                    if t != h && self.cast_target_error(h, a, t).is_none() {
                        self.spell_damage(vm, h, t, dps);
                    }
                }
            }
        }
    }
}
