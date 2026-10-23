//! Orc unit abilities: Spirit Link, Unstable Concoction, Burning Oil, and Devour.

use w3_jass::Vm;

use crate::objects::Id;
use crate::types::*;
use crate::world::World;

impl World {
    /// Damage taking a unit's life, after Mana Shield; a Spirit Linked unit passes its
    /// group's share on to every unit linked with it.  Returns the other linked units
    /// it killed.
    pub(crate) fn take_life(&mut self, target: u32, dmg: f32) -> Vec<u32> {
        let link = self.unit(target).and_then(|t| t.buffs.iter().find(|b| b.link_group != 0).map(|b| (b.link_group, b.link_share)));
        let Some((group, share)) = link else {
            if let Some(t) = self.unit_mut(target) {
                t.life -= crate::ability::mana_shield(t, dmg);
            }
            return Vec::new();
        };
        let members: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.buffs.iter().any(|b| b.link_group == group))).collect();
        let shared = dmg * share;
        let each = shared / members.len().max(1) as f32;
        if let Some(t) = self.unit_mut(target) {
            t.life -= crate::ability::mana_shield(t, dmg - shared);
        }
        let mut dead = Vec::new();
        for m in members {
            if let Some(u) = self.unit_mut(m) {
                u.life -= crate::ability::mana_shield(u, each);
                if m != target && u.life <= 0.405 {
                    dead.push(m);
                }
            }
        }
        dead
    }

    /// Spirit Link: the target and up to DataB in all of the caster's friends within
    /// Area of it share DataA of the damage each takes, for Dur.
    pub(crate) fn spirit_link(&mut self, h: u32, a: Id, lvl: u32, target: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some((tx, ty)) = self.unit(target).map(|t| (t.x, t.y)) else { return };
        let buff = self.objects.abilities.get(a).map(|o| crate::objects::id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
        let (share, max, area, dur) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl).max(1.0) as usize, self.data(a, "Area", lvl), self.data(a, "Dur", lvl));
        self.next_link_group += 1;
        let group = self.next_link_group;
        let mut picks = vec![target];
        let mut near: Vec<u32> = self
            .units_in_pub(tx, ty, area)
            .into_iter()
            .filter(|&u| u != target)
            .filter(|&u| self.unit(u).is_some_and(|x| !x.is_building && !x.dead && (x.owner == owner || crate::natives::players::is_ally(self, owner, x.owner)) && !x.buffs.iter().any(|b| b.link_group != 0)))
            .collect();
        near.sort_by(|&p, &q| {
            let d = |u: u32| self.unit(u).map_or(f32::MAX, |x| (x.x - tx).hypot(x.y - ty));
            d(p).total_cmp(&d(q))
        });
        picks.extend(near.into_iter().take(max.saturating_sub(1)));
        for u in picks {
            // A unit joins only one link at a time.
            if let Some(x) = self.unit_mut(u) {
                x.buffs.retain(|b| b.link_group == 0);
            }
            self.apply_buff(u, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, link_group: group, link_share: share, ..Default::default() });
        }
    }

    /// Unstable Concoction: the caster blows up on its target: DataB to units within
    /// DataA of it and DataD within DataC (air units the ability allows).
    pub(crate) fn unstable_concoction(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32) {
        let Some((x, y)) = self.unit(target).map(|t| (t.x, t.y)) else { return };
        let (full_r, full, part_r, part) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl), self.data(a, "DataC", lvl), self.data(a, "DataD", lvl));
        let owner = self.unit(h).map_or(0, |u| u.owner);
        for t in self.enemies_in(owner, x, y, full_r.max(part_r)) {
            let Some(tu) = self.unit(t) else { continue };
            let air = self.objects.units.get(tu.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
            if !air {
                continue;
            }
            let d = (tu.x - x).hypot(tu.y - y);
            let dmg = if t == target || d <= full_r { full } else { part };
            self.spell_damage(vm, h, t, dmg);
        }
        crate::natives::units::kill_unit(self, vm, h, h);
    }

    /// Devour: a Kodo swallows a unit of level DataA or less, which its Devour Cargo
    /// (Advc) digests at DataB a second; it comes out again if the Kodo dies.
    pub(crate) fn devour(&mut self, h: u32, a: Id, lvl: u32, target: u32) -> bool {
        let Some(t) = self.unit(target) else { return false };
        if t.is_hero || t.level as f32 > self.data(a, "DataA", lvl) {
            return false;
        }
        let capacity = self.ability_with_code(h, "Advc").map_or(1.0, |(c, l)| self.data(c, "DataA", l.max(1) as u32)).max(1.0) as usize;
        if self.unit(h).is_none_or(|u| u.extra.devoured.len() >= capacity) {
            return false;
        }
        if let Some(t) = self.unit_mut(target) {
            t.hidden = true;
            t.paused = true;
            t.action = Action::Idle;
            t.path.clear();
            t.swing = None;
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.devoured.push(target);
        }
        true
    }

    /// Digests what Kodos have devoured, and lets it out of dead ones.
    pub fn update_devoured(&mut self, vm: &mut Vm, dt: f32) {
        let kodos: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.extra.devoured.is_empty())).collect();
        for k in kodos {
            let Some(u) = self.unit(k) else { continue };
            let (dead, x, y) = (u.dead, u.x, u.y);
            let inside = u.extra.devoured.clone();
            if dead {
                for t in inside {
                    if let Some(tu) = self.unit_mut(t) {
                        tu.hidden = false;
                        tu.paused = false;
                        tu.x = x;
                        tu.y = y;
                    }
                }
                self.unit_mut(k).unwrap().extra.devoured.clear();
                continue;
            }
            let dps = self.ability_with_code(k, "Advc").map_or(0.0, |(c, l)| self.data(c, "DataB", l.max(1) as u32));
            for t in inside {
                if self.unit(t).is_none_or(|tu| tu.dead) {
                    self.unit_mut(k).unwrap().extra.devoured.retain(|&x| x != t);
                    continue;
                }
                self.spell_damage(vm, k, t, dps * dt);
            }
        }
    }
}
