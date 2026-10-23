//! Human unit abilities beyond the shared ones: Control Magic, Spell Steal, the
//! Phoenix's egg (Aphx), Phoenix Fire, and Barrage.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// Control Magic: an enemy summoned unit of level DataA or less becomes the caster's,
    /// for DataB mana per hit point it has.  Returns false when it cannot be taken.
    pub(crate) fn control_magic(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32) -> bool {
        let Some(t) = self.unit(target) else { return false };
        if !self.is_summoned(target) || t.level as f32 > self.data(a, "DataA", lvl) {
            return false;
        }
        let cost = self.data(a, "DataB", lvl) * t.life;
        let Some(u) = self.unit_mut(h) else { return false };
        if u.mana < cost {
            return false;
        }
        u.mana -= cost;
        let owner = u.owner;
        let color = self.players.get(owner as usize).map_or(0, |p| p.color);
        let prev = self.unit(target).map_or(owner, |t| t.owner);
        if let Some(t) = self.unit_mut(target) {
            t.owner = owner;
            t.color = color;
            t.action = Action::Idle;
        }
        self.update_food();
        let ctx = EventCtx { state: prev as i32, ..Default::default() };
        use crate::event_ids::*;
        self.fire_unit_pair(vm, target, EVENT_PLAYER_UNIT_CHANGE_OWNER, EVENT_UNIT_CHANGE_OWNER, ctx);
        true
    }

    /// Spell Steal: a buff its caster's friends gave an enemy moves to a friend of the
    /// caster within Area, or one its enemies gave a friend moves to an enemy within
    /// Area.
    pub(crate) fn spell_steal(&mut self, h: u32, a: Id, lvl: u32, target: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some(t) = self.unit(target) else { return };
        let ally = |w: &World, p: u8| p == owner || crate::natives::players::is_ally(w, owner, p);
        let target_friendly = ally(self, t.owner);
        // The buff to take: from an enemy, one its own side cast on it; from a friend,
        // one an enemy cast on it.  Auras (no end) stay.
        let pick = t.buffs.iter().position(|b| {
            let from = self.unit(b.caster).map_or(t.owner, |c| c.owner);
            let from_friend = ally(self, from);
            b.remaining > 0.0 && b.remaining < 1e6 && if target_friendly { !from_friend } else { !from_friend && (from == t.owner || crate::natives::players::is_ally(self, from, t.owner)) }
        });
        let Some(k) = pick else { return };
        let area = self.data(a, "Area", lvl);
        let (tx, ty) = (t.x, t.y);
        let receiver = self
            .units_in_pub(tx, ty, area)
            .into_iter()
            .filter(|&r| r != target)
            .filter(|&r| self.unit(r).is_some_and(|ru| !ru.is_building && if target_friendly { !ally(self, ru.owner) } else { ally(self, ru.owner) }))
            .min_by(|&p, &q| {
                let d = |r: u32| self.unit(r).map_or(f32::MAX, |u| (u.x - tx).hypot(u.y - ty));
                d(p).total_cmp(&d(q))
            });
        let Some(t) = self.unit_mut(target) else { return };
        let mut b = t.buffs.remove(k);
        let effect = std::mem::take(&mut b.effect);
        if effect != 0 {
            self.handles.remove(effect);
        }
        if let Some(r) = receiver {
            b.caster = h;
            self.apply_buff(r, b);
        }
    }

    /// The Phoenix falls into its egg (UnitID) instead of dying; the egg hatches back
    /// into it (DataA) after HeroDur unless destroyed.  True when it did.
    pub(crate) fn phoenix_egg(&mut self, h: u32) -> bool {
        let Some((a, lvl)) = self.ability_with_code(h, "Aphx") else { return false };
        let lvl = lvl.max(1) as u32;
        let Some(o) = self.objects.abilities.get(a) else { return false };
        let (normal, egg) = (id_from_str(o.level_str("DataA", lvl).trim()), id_from_str(o.level_str("UnitID", lvl).trim()));
        let hatch = o.level_real("HeroDur", lvl);
        if self.unit(h).is_none_or(|u| u.type_id != normal || u.dead) {
            return false;
        }
        self.change_type(h, egg);
        if let Some(u) = self.unit_mut(h) {
            u.life = u.max_life;
            u.action = Action::Idle;
            u.extra.morph_revert = Some((hatch, normal));
        }
        true
    }

    /// Phoenix Fire and Barrage: units with them strike enemies near them every Cool
    /// seconds.  Phoenix Fire burns one unit not already burning for DataA, then DataB a
    /// second for Dur (HeroDur on Heroes); Barrage sends rockets at up to DataC air units
    /// within Area for DataA each, DataB at most in all.
    pub fn update_periodic_attacks(&mut self, vm: &mut Vm, dt: f32) {
        let _ = vm;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && u.constructing.is_none())).collect();
        for h in units {
            for code in ["Apxf", "Aroc"] {
                let Some((a, lvl)) = self.ability_with_code(h, code) else { continue };
                let lvl = lvl.max(1) as u32;
                let Some(u) = self.unit(h) else { continue };
                let owner = u.owner;
                if !self.ability_enabled(owner, a) {
                    continue;
                }
                let left = u.cooldowns.get(&a).copied().unwrap_or(0.0) - dt;
                if left > 0.0 {
                    self.unit_mut(h).unwrap().cooldowns.insert(a, left);
                    continue;
                }
                let (ux, uy) = (u.x, u.y);
                let area = self.data(a, "Area", lvl);
                let listed = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default();
                let first = listed.split(',').next().unwrap_or("").trim().to_string();
                let first = if first.is_empty() || first == "_" { crate::ability::default_buff(code).unwrap_or("").to_string() } else { first };
                let buff = id_from_str(&first);
                let targs = self.objects.abilities.get(a).map(|o| o.level_str("targs", lvl).to_ascii_lowercase()).unwrap_or_default();
                let mut targets: Vec<u32> = self
                    .enemies_in(owner, ux, uy, area)
                    .into_iter()
                    .filter(|&t| {
                        self.unit(t).is_some_and(|tu| {
                            let air = self.objects.units.get(tu.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
                            !tu.is_building && self.unit_visible_to(t, owner) && if air { targs.contains("air") } else { targs.contains("ground") }
                        })
                    })
                    .collect();
                targets.sort_by(|&p, &q| {
                    let d = |t: u32| self.unit(t).map_or(f32::MAX, |u| (u.x - ux).hypot(u.y - uy));
                    d(p).total_cmp(&d(q))
                });
                let shots: Vec<(u32, f32)> = if code == "Apxf" {
                    targets.into_iter().filter(|&t| !self.unit(t).is_some_and(|tu| tu.buffs.iter().any(|b| b.id == buff && buff != [0; 4]))).take(1).map(|t| (t, self.data(a, "DataA", lvl))).collect()
                } else {
                    let (each, total, max) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl), self.data(a, "DataC", lvl).max(1.0) as usize);
                    let n = targets.len().min(max);
                    let each = if n > 0 && total > 0.0 { each.min(total / n as f32) } else { each };
                    targets.into_iter().take(n).map(|t| (t, each)).collect()
                };
                if shots.is_empty() {
                    continue;
                }
                let cool = self.data(a, "Cool", lvl);
                self.unit_mut(h).unwrap().cooldowns.insert(a, cool);
                let model = self.objects.abilities.get(a).map(|o| o.str("Missileart")).unwrap_or_default();
                let speed = self.objects.abilities.get(a).map_or(900.0, |o| o.real("Missilespeed")).max(1.0);
                let arc = self.objects.abilities.get(a).map_or(0.0, |o| o.real("Missilearc"));
                for (t, damage) in shots {
                    if code == "Apxf" {
                        let dur = if self.hero_duration(t) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps: self.data(a, "DataB", lvl), ..Default::default() });
                    }
                    let launch = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(60.0, |o| o.real("launchZ"));
                    let z = self.ground_z(ux, uy) + launch;
                    self.missiles.push(crate::sim::Missile {
                        id: 0,
                        source: h,
                        target: t,
                        model: model.clone(),
                        x: ux,
                        y: uy,
                        z,
                        speed,
                        arc,
                        damage,
                        attack_type: "spells".into(),
                        start: (ux, uy),
                        spell: Some((a, lvl, 0.0, [0; 4])),
                        weapon: None,
                        impact: None,
                        bounces: 0,
                        hit: Vec::new(),
                    stampede: None,
                    });
                }
            }
        }
    }
}
