//! Unit spells, each from its AbilityData fields (named as in AbilityMetaData): Raise
//! Dead, Curse, Faerie Fire, Dispel and Abolish Magic, Devour Magic, Purge, Lightning
//! Shield, Polymorph, Invisibility, Cripple, Unholy Frenzy, Anti-magic Shell, Possession,
//! Cyclone, Roar, Rejuvenation, Taunt, Berserk, Absorb Mana, and Ancestral Spirit.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    fn level_str(&self, a: Id, field: &str, lvl: u32) -> String {
        self.objects.abilities.get(a).map(|o| o.level_str(field, lvl)).unwrap_or_default().trim().to_string()
    }

    fn first_buff(&self, a: Id, lvl: u32) -> Id {
        id_from_str(self.level_str(a, "BuffID", lvl).split(',').next().unwrap_or("").trim())
    }

    /// Whether a unit was summoned (it has a timed life or the Summoned classification).
    pub fn is_summoned(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.timed_life.is_some() || self.objects.units.get(u.type_id).is_some_and(|o| o.str("type").to_ascii_lowercase().contains("summoned")))
    }

    /// Removes buffs a dispel takes: from an ally of the caster those its enemies cast,
    /// from an enemy those its allies cast.  Summoned units take `summon_damage`.
    fn dispel(&mut self, vm: &mut Vm, caster: u32, target: u32, summon_damage: f32) -> usize {
        let Some(owner) = self.unit(caster).map(|u| u.owner) else { return 0 };
        let Some(tu) = self.unit(target) else { return 0 };
        let friend = crate::natives::players::is_ally(self, owner, tu.owner);
        let towner = tu.owner;
        let removable: Vec<bool> = tu
            .buffs
            .iter()
            .map(|b| {
                let from_ally_of_target = self.unit(b.caster).is_some_and(|c| crate::natives::players::is_ally(self, towner, c.owner));
                if friend { !from_ally_of_target } else { from_ally_of_target }
            })
            .collect();
        let mut gone = Vec::new();
        let u = self.unit_mut(target).unwrap();
        let mut k = 0;
        u.buffs.retain(|b| {
            let r = removable[k];
            k += 1;
            if r {
                gone.push(b.effect);
            }
            !r
        });
        let n = gone.len();
        for e in gone {
            if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                x.destroyed = true;
            }
        }
        if summon_damage > 0.0 && !friend && self.is_summoned(target) {
            self.spell_damage(vm, caster, target, summon_damage);
        }
        n
    }

    pub(crate) fn unit_spell_effect(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, code: &str, target: u32, (x, y): (f32, f32)) -> bool {
        let d = |w: &World, f: &str| w.data(a, f, lvl);
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let buff = self.first_buff(a, lvl);
        let hero = self.unit(target).is_some_and(|t| t.is_hero);
        let dur = if hero { d(self, "HeroDur") } else { d(self, "Dur") };
        let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
        match code {
            // Raise Dead: DataA units of DataC and DataB of DataD from the nearest corpse,
            // which is used up; they last Dur.
            "Arai" => {
                let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
                let Some(&corpse) = self.corpses_in(cx, cy, d(self, "Area")).first() else { return true };
                let (px, py) = self.unit(corpse).map(|u| (u.x, u.y)).unwrap();
                crate::natives::units::remove_unit(self, corpse);
                let life = d(self, "Dur");
                for (n, t) in [(d(self, "DataA") as i32, self.level_str(a, "DataC", lvl)), (d(self, "DataB") as i32, self.level_str(a, "DataD", lvl))] {
                    if n > 0 && !t.is_empty() {
                        self.summon(h, owner, id_from_str(&t), n, px, py, life);
                    }
                }
            }
            // Curse: the target misses DataA of its attacks.
            "Acrs" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, miss_chance: d(self, "DataA"), ..Default::default() }),
            // Faerie Fire: DataA less armor.
            "Afae" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, armor: -d(self, "DataA"), ..Default::default() }),
            // Abolish Magic: one target.
            "Aadm" => {
                self.dispel(vm, h, target, d(self, "DataB"));
            }
            // Dispel Magic: everyone within Area.
            "Adis" => {
                for t in self.units_in_pub(x, y, d(self, "Area")) {
                    self.dispel(vm, h, t, d(self, "DataB"));
                }
            }
            // Devour Magic: dispels within Area; the caster gains DataA life and DataB mana
            // per unit and DataC and DataD per buff.
            "Advm" => {
                let (mut units, mut buffs) = (0, 0);
                for t in self.units_in_pub(x, y, d(self, "Area")) {
                    let n = self.dispel(vm, h, t, d(self, "DataE"));
                    if n > 0 {
                        units += 1;
                        buffs += n;
                    }
                }
                let (life, mana) = (d(self, "DataA") * units as f32 + d(self, "DataC") * buffs as f32, d(self, "DataB") * units as f32 + d(self, "DataD") * buffs as f32);
                if let Some(u) = self.unit_mut(h) {
                    u.life = (u.life + life).min(u.max_life);
                    u.mana = (u.mana + mana).min(u.max_mana);
                }
            }
            // Purge: dispels and stops the target, its speed returning over Dur.
            "Aprg" => {
                self.dispel(vm, h, target, d(self, "DataC"));
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -1.0, decaying: dur, ..Default::default() });
            }
            // Lightning Shield: DataA damage a second to units within Area of the target.
            "Alsh" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, area_dps: d(self, "DataA"), area: d(self, "Area"), ..Default::default() }),
            // Polymorph: the target becomes a critter (DataB ground, DataC air, DataD
            // amphibious, DataE water) for Dur.
            "Aply" => {
                let kind = self.move_kind(target);
                let field = match kind {
                    crate::pathing::Move::Fly => "DataC",
                    crate::pathing::Move::Amphibious => "DataD",
                    crate::pathing::Move::Float => "DataE",
                    _ => "DataB",
                };
                let to = id_from_str(&self.level_str(a, field, lvl));
                let Some(back) = self.unit(target).map(|u| u.type_id) else { return true };
                if self.objects.units.get(to).is_some() {
                    self.change_type(target, to);
                    if let Some(u) = self.unit_mut(target) {
                        u.extra.morph_revert = Some((dur, back));
                    }
                    self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, ..Default::default() });
                }
            }
            // Invisibility.
            "Aivs" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, invisible: true, ..Default::default() }),
            // Cripple: DataA less movement, DataB less attack speed, DataC less damage.
            "Acri" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -d(self, "DataA"), attack_factor: -d(self, "DataB"), damage_factor: -d(self, "DataC"), ..Default::default() }),
            // Unholy Frenzy: DataA attack speed and DataB damage a second.
            "Auhf" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, attack_factor: d(self, "DataA"), dps: d(self, "DataB"), ..Default::default() }),
            // Anti-magic Shell: immune to magic.
            "Aams" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, magic_immune: true, ..Default::default() }),
            // Possession: the target becomes the caster's and the caster is spent.
            "Apos" => {
                if let Some(u) = self.unit_mut(target) {
                    u.owner = owner;
                    u.action = Action::Idle;
                }
                crate::natives::units::remove_unit(self, h);
            }
            // Cyclone: lifted, helpless and untouchable, for Dur.
            "Acyc" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, invulnerable: true, ..Default::default() }),
            // Roar: DataA more damage for allies within Area.
            "Aroa" => {
                let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
                for t in self.allies_in_pub(owner, cx, cy, d(self, "Area")) {
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, damage_factor: d(self, "DataA"), armor: d(self, "DataB"), life_regen: d(self, "DataC"), mana_regen: d(self, "DataD"), ..Default::default() });
                }
            }
            // Rejuvenation: DataA life and DataB mana over Dur.
            "Arej" => {
                let t = dur.max(0.01);
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, life_regen: d(self, "DataA") / t, mana_regen: d(self, "DataB") / t, ..Default::default() });
            }
            // Taunt: enemies within Area (at most DataC when set) attack the caster.
            "Atau" => {
                let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
                let max = d(self, "DataC") as usize;
                let mut n = 0;
                for t in self.enemies_in(owner, cx, cy, d(self, "Area")) {
                    if max > 0 && n >= max {
                        break;
                    }
                    if let Some(u) = self.unit_mut(t) {
                        if !u.is_building {
                            u.action = Action::Attack { target: h };
                            n += 1;
                        }
                    }
                }
            }
            // Berserk: DataA movement, DataB attack speed, and DataC more damage taken.
            "Absk" => self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, move_factor: d(self, "DataA"), attack_factor: d(self, "DataB"), damage_taken: d(self, "DataC"), ..Default::default() }),
            // Absorb Mana: the caster takes the target's mana (up to DataB) and life (DataA).
            "Aabs" => {
                let (tl, tm) = self.unit(target).map(|u| (u.life, u.mana)).unwrap_or_default();
                let mana = tm.min(d(self, "DataB"));
                let life = tl.min(d(self, "DataA"));
                if let Some(u) = self.unit_mut(target) {
                    u.mana -= mana;
                }
                if let Some(u) = self.unit_mut(h) {
                    u.mana = (u.mana + mana).min(u.max_mana);
                    u.life = (u.life + life).min(u.max_life);
                }
            }
            // Ancestral Spirit: a dead unit of the caster's type rises with DataA of its
            // life and DataB of its mana.
            "Aast" => {
                let Some(t) = self.unit(target).filter(|t| t.dead).map(|t| t.handle) else { return true };
                let (lf, mf) = (d(self, "DataA"), d(self, "DataB"));
                if let Some(u) = self.unit_mut(t) {
                    u.dead = false;
                    u.life = u.max_life * lf;
                    u.mana = u.max_mana * mf;
                    u.action = Action::Idle;
                    crate::sim::set_anim(u, "stand");
                }
            }
            _ => return false,
        }
        let _ = (tx, ty);
        true
    }

    /// Area buffs: Lightning Shield's damage around the shielded unit, and Purge's slow
    /// wearing off.
    pub fn update_area_buffs(&mut self, vm: &mut Vm, dt: f32) {
        let mut hits = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h).filter(|u| !u.dead) else { continue };
            for b in &u.buffs {
                if b.area_dps > 0.0 {
                    for t in self.units_in_pub(u.x, u.y, b.area) {
                        if t != h {
                            hits.push((b.caster, t, b.area_dps * dt));
                        }
                    }
                }
            }
        }
        for (src, t, amount) in hits {
            self.spell_damage(vm, src, t, amount);
        }
        for &h in &self.unit_order.clone() {
            if let Some(u) = self.unit_mut(h) {
                for b in &mut u.buffs {
                    if b.decaying > 0.0 {
                        b.move_factor = -(b.remaining / b.decaying).clamp(0.0, 1.0);
                    }
                }
            }
        }
    }
}
