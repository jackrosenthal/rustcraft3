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
            u.channel = Some(Channel { ability: a, level: lvl, target, x: 0.0, y: 0.0, elapsed: 0.0, duration: dur, interval: dur + 1.0, next: dur + 1.0, waves: 0, link: 0, held: buffs.first().copied().unwrap_or_default(), lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
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
            // The Cloak of Flames (AIcf) burns as Permanent Immolation does.
            let cloak = self.unit(h).map(|u| u.inventory.clone()).unwrap_or_default().into_iter().filter_map(|i| self.item(i).map(|i| i.type_id)).flat_map(|t| self.item_abilities(t)).find(|&a| self.ability_code(a) == "AIcf");
            for code in ["ANpi", "Apig", "AIcf"] {
                let found = if code == "AIcf" { cloak.map(|a| (a, 1)) } else { self.ability_with_code(h, code) };
                let Some((a, lvl)) = found else {
                    // Without it (a cloak dropped), its flames go.
                    let gone: Vec<Id> = self.unit(h).map(|u| u.buffs.iter().filter(|b| self.ability_code(b.ability) == code).map(|b| b.ability).collect()).unwrap_or_default();
                    if !gone.is_empty() {
                        self.unit_mut(h).unwrap().buffs.retain(|b| !gone.contains(&b.ability));
                    }
                    continue;
                };
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

/// A Locust Swarm under way.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Swarm {
    pub caster: u32,
    pub ability: Id,
    pub level: u32,
    pub until: f32,
    pub next: f32,
    pub released: u32,
    pub locusts: Vec<u32>,
}

impl World {
    /// Locust Swarm: DataA locusts (UnitID) leave the caster one every DataB for Dur;
    /// each attacks an enemy within Area of it (at most DataC on one), and after dealing
    /// DataE comes back to give the caster DataD of what it dealt as life.
    pub(crate) fn begin_swarm(&mut self, h: u32, a: Id, lvl: u32) {
        let until = self.time + self.data(a, "Dur", lvl).max(self.data(a, "HeroDur", lvl));
        self.swarms.push(Swarm { caster: h, ability: a, level: lvl, until, next: self.time, released: 0, locusts: Vec::new() });
    }

    pub fn update_swarms(&mut self) {
        let mut swarms = std::mem::take(&mut self.swarms);
        swarms.retain_mut(|s| {
            let (a, lvl) = (s.ability, s.level);
            let caster = self.unit(s.caster).filter(|u| !u.dead).map(|u| (u.owner, u.x, u.y));
            let Some((owner, cx, cy)) = caster.filter(|_| self.time < s.until) else {
                for &l in &s.locusts {
                    crate::natives::units::remove_unit(self, l);
                }
                return false;
            };
            let (count, every, per_target, ret, threshold, area) = (self.data(a, "DataA", lvl) as u32, self.data(a, "DataB", lvl).max(0.01), self.data(a, "DataC", lvl) as usize, self.data(a, "DataD", lvl), self.data(a, "DataE", lvl), self.data(a, "Area", lvl));
            let kind = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
            while s.released < count && self.time >= s.next {
                let l = crate::natives::units::create_unit(self, owner, kind, cx, cy, 0.0);
                if let Some(u) = self.unit_mut(l) {
                    u.extra.swarm = Some((s.caster, 0.0, false));
                }
                s.locusts.push(l);
                s.released += 1;
                s.next += every;
            }
            s.locusts.retain(|&l| self.unit(l).is_some_and(|u| !u.dead));
            let enemies: Vec<u32> = self.enemies_in(owner, cx, cy, area).into_iter().filter(|&t| self.unit(t).is_some_and(|u| !u.is_building)).collect();
            for &l in &s.locusts.clone() {
                let Some((_, dealt, back)) = self.unit(l).and_then(|u| u.extra.swarm) else { continue };
                let Some(u) = self.unit(l) else { continue };
                let (lx, ly) = (u.x, u.y);
                if back {
                    if (lx - cx).hypot(ly - cy) < 64.0 {
                        if let Some(c) = self.unit_mut(s.caster) {
                            c.life = (c.life + dealt * ret).min(c.max_life);
                        }
                        self.unit_mut(l).unwrap().extra.swarm = Some((s.caster, 0.0, false));
                    } else {
                        self.unit_mut(l).unwrap().action = Action::Move { x: cx, y: cy };
                    }
                    continue;
                }
                if dealt >= threshold {
                    let u = self.unit_mut(l).unwrap();
                    u.extra.swarm = Some((s.caster, dealt, true));
                    u.action = Action::Move { x: cx, y: cy };
                    u.path.clear();
                    continue;
                }
                let current = match u.action {
                    Action::Attack { target } if enemies.contains(&target) => Some(target),
                    _ => None,
                };
                if current.is_some() {
                    continue;
                }
                let on = |t: u32| s.locusts.iter().filter(|&&o| o != l && matches!(self.unit(o).map(|u| &u.action), Some(Action::Attack { target }) if *target == t)).count();
                let pick = enemies.iter().copied().filter(|&t| on(t) < per_target).min_by(|&p, &q| {
                    let d = |t: u32| self.unit(t).map_or(f32::MAX, |u| (u.x - lx).hypot(u.y - ly));
                    d(p).total_cmp(&d(q))
                });
                let u = self.unit_mut(l).unwrap();
                u.action = match pick {
                    Some(t) => Action::Attack { target: t },
                    None => Action::Follow { target: s.caster },
                };
            }
            true
        });
        swarms.append(&mut self.swarms);
        self.swarms = swarms;
    }
}
