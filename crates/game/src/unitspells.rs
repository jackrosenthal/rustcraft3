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

    pub(crate) fn first_buff(&self, a: Id, lvl: u32) -> Id {
        let listed = self.level_str(a, "BuffID", lvl);
        let first = listed.split(',').next().unwrap_or("").trim();
        if first.is_empty() || first == "_" {
            return crate::ability::default_buff(&self.ability_code(a)).map_or([0; 4], id_from_str);
        }
        id_from_str(first)
    }

    /// Whether a unit was summoned (it has a timed life or the Summoned classification).
    pub fn is_summoned(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.timed_life.is_some() || self.objects.units.get(u.type_id).is_some_and(|o| o.str("type").to_ascii_lowercase().contains("summoned")))
    }

    /// Removes buffs a dispel takes: from an ally of the caster those its enemies cast,
    /// from an enemy those its allies cast.  Summoned units take `summon_damage`.
    pub(crate) fn dispel_pub(&mut self, vm: &mut Vm, caster: u32, target: u32, summon_damage: f32) -> usize {
        self.dispel(vm, caster, target, summon_damage)
    }

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
        let dur = if self.hero_duration(target) { d(self, "HeroDur") } else { d(self, "Dur") };
        let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
        match code {
            // Raise Dead: DataA units of DataC and DataB of DataD from the nearest corpse,
            // which is used up; they last Dur.
            "Arai" => {
                let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
                let Some(&corpse) = self.corpses_in(cx, cy, d(self, "Area")).first() else { return true };
                let (px, py) = self.unit(corpse).map(|u| (u.x, u.y)).unwrap();
                crate::natives::units::remove_unit(self, corpse);
                let life = d(self, "Dur") + self.unit(h).map_or(0.0, |u| self.upgrade_ability_bonus(u.owner, u.type_id, "rrai", "Arai"));
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
            // The Orb of Lightning's purge (CAbilityLightningPurge) is Purge's.
            "Aprg" | "AIlp" => {
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
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, life_regen: d(self, "DataA") / t, mana_regen: d(self, "DataB") / t, ethereal_heal: true, ..Default::default() });
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
                    u.extra.decaying = false;
                    u.life = u.max_life * lf;
                    u.mana = u.max_mana * mf;
                    u.action = Action::Idle;
                    crate::sim::set_anim(u, "stand");
                }
            }
            // Sentinel: an owl perches on the tree, DataC above it, seeing DataB around,
            // until the tree falls; the caster keeps at most DataD.
            "Aesn" | "Aesr" => {
                if !self.is_tree_pub(target) {
                    return true;
                }
                let (px, py) = self.widget_pos(target).unwrap_or((x, y));
                let z = self.ground_z(px, py) + d(self, "DataC");
                let model = self.profile_art(a, "Missileart");
                let e = self.handles.insert(Obj::Effect(Effect { model, x: px, y: py, z, target: None, destroyed: false, facing: 0.0 }));
                let max = d(self, "DataD").max(1.0) as usize;
                let mine: Vec<usize> = self.sentinels.iter().enumerate().filter(|(_, s)| s.caster == h).map(|(i, _)| i).collect();
                if mine.len() >= max {
                    let old = self.sentinels.remove(mine[0]);
                    if let Some(Obj::Effect(x)) = self.handles.get_mut(old.effect) {
                        x.destroyed = true;
                    }
                }
                self.sentinels.push(crate::world::Sentinel { caster: h, owner, tree: target, radius: d(self, "DataB"), effect: e });
            }
            // Wards: Healing Ward, Sentry Ward, and Stasis Trap place their UnitID at the
            // point for Dur.
            "Ahwd" | "Aeye" | "Asta" => {
                let unit = id_from_str(&self.level_str(a, "UnitID", lvl));
                let wards = self.summon(h, owner, unit, 1, x, y, d(self, "Dur"));
                if code == "Asta" {
                    let now = self.time;
                    for w in wards {
                        if let Some(u) = self.unit_mut(w) {
                            u.extra.trap = Some((a, lvl, now));
                        }
                    }
                }
            }
            // Unsummon: an owned structure is taken down, returning DataA of its cost.
            "Auns" => {
                let Some(t) = self.unit(target).filter(|t| t.is_building && t.owner == owner) else { return true };
                let ratio = d(self, "DataA");
                let frac = if t.max_life > 0.0 { t.life / t.max_life } else { 1.0 };
                let (gold, lumber) = self.objects.units.get(t.type_id).map_or((0.0, 0.0), |o| (o.real("goldcost"), o.real("lumbercost")));
                let p = &mut self.players[owner as usize];
                *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += (gold * ratio * frac) as i32;
                *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += (lumber * ratio * frac) as i32;
                crate::natives::units::kill_unit(self, vm, target, 0);
            }
            // Cannibalize: a corpse within reach heals DataA a second, up to DataB.
            "Acan" => {
                let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
                let Some(&corpse) = self.corpses_in(cx, cy, d(self, "Rng") + 64.0).first() else { return true };
                crate::natives::units::remove_unit(self, corpse);
                let rate = d(self, "DataA").max(0.01);
                let missing = self.unit(h).map_or(0.0, |u| u.max_life - u.life);
                let time = (missing.min(d(self, "DataB")) / rate).min(d(self, "Dur").max(0.0));
                self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: time, life_regen: rate, ..Default::default() });
                if let Some(u) = self.unit_mut(h) {
                    crate::sim::set_anim(u, "stand channel");
                }
            }
            _ => return false,
        }
        let _ = (tx, ty);
        true
    }

    /// Sacrifice: an acolyte at a sacrificial pit (Asac) becomes a shade there.
    pub fn sacrifice(&mut self, acolyte: u32, pit: u32) -> bool {
        if self.ability_with_code(pit, "Asac").is_none() || self.unit(acolyte).is_none_or(|u| u.dead) {
            return false;
        }
        let (owner, x, y) = self.unit(pit).map(|p| (p.owner, p.x, p.y)).unwrap();
        if self.unit(acolyte).is_none_or(|u| u.owner != owner) {
            return false;
        }
        crate::natives::units::remove_unit(self, acolyte);
        let shade = crate::natives::units::create_unit(self, owner, *b"ushd", x, y, 270.0);
        if shade != 0 {
            let r = self.unit(shade).map_or(16.0, |u| u.collision);
            let (sx, sy) = self.spawn_point_pub(pit, r);
            if let Some(u) = self.unit_mut(shade) {
                u.x = sx;
                u.y = sy;
            }
        }
        true
    }

    /// Defend (Adef): on, the unit takes DataA of piercing damage and moves at DataC
    /// less speed.
    pub fn defend_order(&mut self, h: u32, on: bool) -> bool {
        let Some((a, lvl)) = self.ability_with_code(h, "Adef") else { return false };
        let buff = self.first_buff(a, lvl);
        if let Some(u) = self.unit_mut(h) {
            u.buffs.retain(|b| b.ability != a);
        }
        if on {
            let (taken, slow) = (self.data(a, "DataA", lvl), self.data(a, "DataC", lvl));
            self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::INFINITY, pierce_taken: taken, move_factor: -slow, ..Default::default() });
        }
        true
    }

    /// Defend's deflection (CAbilityDefend::vf91, FUN_6f5d1920): under DefendDeflection,
    /// an attack missile reaching a defending unit has a DataF% chance (by pseudo-random
    /// distribution) to go back at its source when the unit would take other than all of
    /// it, DataG for Piercing attacks and DataH for Spells and Magic ones; a structure's
    /// attacks only where DataI is set.  Returns the share the defender still takes.
    pub(crate) fn defend_deflects(&mut self, defender: u32, source: u32, attack_type: &str) -> Option<f32> {
        if self.misc_f32("DefendDeflection", 0.0) == 0.0 {
            return None;
        }
        let (a, lvl) = self.ability_with_code(defender, "Adef")?;
        if !self.unit(defender)?.buffs.iter().any(|b| b.ability == a) {
            return None;
        }
        let taken = match attack_type.to_ascii_lowercase().as_str() {
            "pierce" => self.data(a, "DataG", lvl),
            "spells" | "magic" => self.data(a, "DataH", lvl),
            _ => return None,
        };
        if taken == 1.0 || !self.prd_roll(defender, a, self.data(a, "DataF", lvl)) {
            return None;
        }
        if self.unit(source).is_none_or(|u| u.dead || (u.is_building && self.data(a, "DataI", lvl) == 0.0)) {
            return None;
        }
        log::debug!("defend: {defender} deflects an attack from {source}");
        Some(taken)
    }

    /// Stasis traps arm after DataA seconds; then an enemy within DataB sets them off,
    /// stunning enemies within DataC for DataD.  Shadowmeld hides idle units at night.
    pub fn update_traps(&mut self, vm: &mut Vm) {
        let traps: Vec<(u32, Id, u32, f32)> = self.unit_order.iter().filter_map(|&h| self.unit(h).filter(|u| !u.dead).and_then(|u| u.extra.trap.map(|(a, l, t)| (h, a, l, t)))).collect();
        for (h, a, lvl, placed) in traps {
            if self.time - placed < self.data(a, "DataA", lvl) {
                continue;
            }
            let (owner, x, y) = self.unit(h).map(|u| (u.owner, u.x, u.y)).unwrap();
            if self.enemies_in(owner, x, y, self.data(a, "DataB", lvl)).is_empty() {
                continue;
            }
            let buff = self.first_buff(a, lvl);
            for t in self.enemies_in(owner, x, y, self.data(a, "DataC", lvl)) {
                let dur = if self.hero_duration(t) { self.data(a, "HeroDur", lvl).min(self.data(a, "DataD", lvl)) } else { self.data(a, "DataD", lvl) };
                self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, ..Default::default() });
            }
            crate::natives::units::kill_unit(self, vm, h, 0);
        }
        // Shadowmeld (Ashm): at night, a unit that has stood still DataA seconds hides.
        let night = !(6.0..18.0).contains(&self.time_of_day);
        let melders: Vec<(u32, Id, u32)> = self.unit_order.iter().filter_map(|&h| self.ability_with_code(h, "Ashm").map(|(a, l)| (h, a, l))).collect();
        for (h, a, lvl) in melders {
            let Some(u) = self.unit(h) else { continue };
            let idle = matches!(u.action, Action::Idle | Action::Hold) && u.swing.is_none() && u.casting.is_none();
            let hidden = u.buffs.iter().any(|b| b.ability == a);
            if night && idle {
                let fade = self.data(a, "DataA", lvl);
                let since = u.extra.still_since;
                if !hidden && self.time - since >= fade {
                    let buff = self.first_buff(a, lvl);
                    self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::INFINITY, invisible: true, ..Default::default() });
                }
            } else {
                let now = self.time;
                if let Some(u) = self.unit_mut(h) {
                    u.extra.still_since = now;
                    if hidden {
                        u.buffs.retain(|b| b.ability != a);
                    }
                }
            }
        }
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
