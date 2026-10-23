//! Abilities: casting (orders, range, cast point, mana, cooldown), buffs and auras, and
//! the effects of each ability code.  Field meanings come from AbilityMetaData.slk.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// How an ability's order is aimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targeting {
    None,
    Unit,
    Point,
    UnitOrPoint,
    Passive,
}

/// Abilities whose code takes a point rather than a unit target.
fn point_target(code: &str) -> bool {
    matches!(code, "AHbz" | "AHfs" | "AOsh" | "AUcs" | "AUim" | "ANrf" | "AEbl" | "AUin" | "AOeq" | "AOsw" | "AOfs" | "AUdd" | "AEfn" | "ANst" | "ANvc" | "Adro" | "Aroo" | "Adis" | "Advm" | "Ahwd" | "Aeye" | "Asta")
}

/// Abilities with no target (cast instantly around the caster).
fn no_target(code: &str) -> bool {
    matches!(code, "AHtc" | "AHds" | "AOws" | "AOwk" | "AOmi" | "AHav" | "AUan" | "AHre" | "AEim" | "AEme" | "AOww" | "AEtq" | "AEsf" | "AOvd" | "AOsf" | "AHpx" | "AEst" | "AEsv" | "AUcb" | "AHwe" | "AEfk" | "ANdr" | "ANsy" | "Astd" | "Abtl" | "Arai" | "Aroa" | "Atau" | "Absk")
}

/// Passive abilities and auras.
fn passive(code: &str) -> bool {
    matches!(code, "AHbh" | "AOcr" | "AEev" | "AHad" | "AHab" | "AUav" | "AOae" | "AEar" | "AEah" | "AOr2" | "AUts" | "ANba" | "AUau" | "Aamk" | "AOre")
}

impl World {
    pub fn ability_code(&self, id: Id) -> String {
        self.objects.abilities.get(id).map(|a| a.str("code")).unwrap_or_default()
    }

    pub fn ability_level(&self, h: u32, id: Id) -> u32 {
        let Some(u) = self.unit(h) else { return 0 };
        let base = u.abilities.iter().find(|(a, _)| *a == id).map_or(0, |a| a.1.max(0) as u32);
        // Research raises unit spells (rlev); hero skills level by learning.
        if base > 0 && !u.is_hero {
            let code = self.ability_code(id);
            return base + self.upgrade_spell_levels(u.owner, u.type_id, &code).max(0) as u32;
        }
        base
    }

    pub(crate) fn data(&self, id: Id, field: &str, level: u32) -> f32 {
        self.objects.abilities.get(id).map_or(0.0, |a| a.level_real(field, level.max(1)))
    }

    pub fn targeting(&self, id: Id) -> Targeting {
        let code = self.ability_code(id);
        if passive(&code) {
            Targeting::Passive
        } else if no_target(&code) {
            Targeting::None
        } else if point_target(&code) {
            Targeting::Point
        } else {
            Targeting::Unit
        }
    }

    /// The unit's ability whose order string matches `order`.
    pub fn ability_for_order(&self, h: u32, order: i32) -> Option<Id> {
        let name = crate::natives::basic::order_name(order)?;
        let u = self.unit(h)?;
        u.abilities.iter().map(|(a, _)| *a).find(|&a| {
            self.objects.abilities.get(a).is_some_and(|o| o.str("Order").eq_ignore_ascii_case(name) || o.str("Orderon").eq_ignore_ascii_case(name))
        })
    }

    /// Movement, attack-speed, armor, and damage factors from a unit's buffs.
    pub fn buff_mods(&self, h: u32) -> (f32, f32, f32, f32, bool, bool) {
        let Some(u) = self.unit(h) else { return (1.0, 1.0, 0.0, 1.0, false, false) };
        let (mut mv, mut atk, mut armor, mut dmg, mut stun, mut inv) = (1.0, 1.0, 0.0, 1.0, false, false);
        for b in &u.buffs {
            if b.move_factor != 0.0 {
                mv *= 1.0 + b.move_factor;
            }
            if b.attack_factor != 0.0 {
                atk *= 1.0 + b.attack_factor;
            }
            armor += b.armor;
            if b.damage_factor != 0.0 {
                dmg *= 1.0 + b.damage_factor;
            }
            stun |= b.stun;
            inv |= b.invulnerable;
        }
        (mv.max(0.0), atk.max(0.0), armor, dmg, stun, inv)
    }

    pub fn has_buff_flag(&self, h: u32, f: impl Fn(&Buff) -> bool) -> bool {
        self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| f(b)))
    }

    /// Flat attack bonus from buffs (Avatar) and the life fraction stolen (Vampiric Aura).
    pub fn buff_damage_bonus(&self, h: u32) -> f32 {
        self.unit(h).map_or(0.0, |u| u.buffs.iter().map(|b| b.damage_bonus).sum())
    }

    pub fn is_magic_immune(&self, h: u32) -> bool {
        self.has_buff_flag(h, |b| b.magic_immune) || self.ability_with_code(h, "Amim").is_some() || self.ability_with_code(h, "ACmi").is_some()
    }

    pub fn is_invisible(&self, h: u32) -> bool {
        self.has_buff_flag(h, |b| b.invisible)
    }

    pub fn is_ethereal(&self, h: u32) -> bool {
        self.has_buff_flag(h, |b| b.ethereal)
    }

    /// Damage wakes sleeping units.
    pub fn wake(&mut self, h: u32) {
        if let Some(u) = self.unit_mut(h) {
            u.buffs.retain(|b| !b.sleep);
        }
    }

    /// Adds or refreshes a buff of the same type from the same ability.
    pub fn apply_buff(&mut self, target: u32, mut b: Buff) {
        if self.unit(target).is_none_or(|u| u.dead) {
            return;
        }
        // Buff art attaches to the target (the buff's Targetart and Targetattach).
        let art = self.objects.buffs.get(b.id).map(|o| (o.list("Targetart").first().cloned().unwrap_or_default(), o.list("Targetattach").first().cloned().unwrap_or_default()));
        let u = self.unit_mut(target).unwrap();
        if let Some(existing) = u.buffs.iter_mut().find(|x| x.id == b.id) {
            existing.remaining = existing.remaining.max(b.remaining);
            existing.level = b.level;
            return;
        }
        if let Some((model, attach)) = art {
            if !model.is_empty() {
                let e = self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((target, attach)), destroyed: false, facing: 0.0 }));
                b.effect = e;
            }
        }
        self.unit_mut(target).unwrap().buffs.push(b);
    }

    fn remove_buff_effect(&mut self, e: u32) {
        if e != 0 {
            if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                x.destroyed = true;
            }
            self.dying_effects.push((e, self.time));
        }
    }

    /// Ages buffs, applies damage and healing over time, and refreshes auras.
    pub fn update_buffs(&mut self, vm: &mut Vm, dt: f32) {
        let units = self.unit_order.clone();
        let mut expired_effects = Vec::new();
        let mut dots = Vec::new();
        for &h in &units {
            let Some(u) = self.unit_mut(h) else { continue };
            if u.dead {
                for b in u.buffs.drain(..) {
                    expired_effects.push(b.effect);
                }
                continue;
            }
            for b in &mut u.buffs {
                b.remaining -= dt;
                if b.dps != 0.0 {
                    dots.push((b.caster, h, b.dps * dt));
                }
                u.mana = (u.mana + b.mana_regen * dt).min(u.max_mana);
                u.life = (u.life + (b.life_regen + b.life_regen_pct * u.max_life) * dt).min(u.max_life);
            }
            let mut lost_life = 0.0;
            u.buffs.retain(|b| {
                if b.remaining <= 0.0 {
                    expired_effects.push(b.effect);
                    lost_life += b.life_bonus;
                    false
                } else {
                    true
                }
            });
            if lost_life > 0.0 {
                u.max_life -= lost_life;
                u.life = u.life.min(u.max_life).max(1.0);
            }
            for c in u.cooldowns.values_mut() {
                *c = (*c - dt).max(0.0);
            }
            if let Some(t) = u.timed_life.as_mut().filter(|_| !u.extra.timed_life_paused) {
                *t -= dt;
            }
        }
        for e in expired_effects {
            self.remove_buff_effect(e);
        }
        for (src, h, amount) in dots {
            if amount >= 0.0 {
                self.spell_damage(vm, src, h, amount);
            } else if let Some(u) = self.unit_mut(h) {
                u.life = (u.life - amount).min(u.max_life);
            }
        }
        // Summons whose timed life ran out die.
        let expired: Vec<u32> = units.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.timed_life.is_some_and(|t| t <= 0.0))).collect();
        for h in expired {
            crate::natives::units::kill_unit(self, vm, h, 0);
        }
        self.update_auras();
    }

    /// Auras give their buff to qualifying units in range, refreshed every update.
    fn update_auras(&mut self) {
        let mut grants = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                continue;
            }
            for &(a, lvl) in &u.abilities {
                let code = self.ability_code(a);
                let lvl = lvl.max(1) as u32;
                let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default();
                let mut b = Buff { id: id_from_str(buff.trim()), ability: a, level: lvl, caster: h, remaining: 0.5, ..Default::default() };
                match code.as_str() {
                    // Devotion Aura: armor bonus (DataA).
                    "AHad" => b.armor = self.data(a, "DataA", lvl),
                    // Brilliance Aura: mana regeneration (DataA).
                    "AHab" => b.mana_regen = self.data(a, "DataA", lvl),
                    // Unholy Aura: movement speed factor (DataA) and life regeneration (DataB).
                    "AUau" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.life_regen = self.data(a, "DataB", lvl);
                    }
                    // Endurance Aura: movement (DataA) and attack speed (DataB) factors.
                    "AOae" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.attack_factor = self.data(a, "DataB", lvl);
                    }
                    // Trueshot Aura: damage bonus factor (DataA).
                    "AEar" => b.damage_factor = self.data(a, "DataA", lvl),
                    // Vampiric Aura: a share of attack damage returns as life (DataA).
                    "AUav" => b.life_steal = self.data(a, "DataA", lvl),
                    // Thorns Aura: melee attackers take damage (DataA, a fraction when DataB).
                    "AEah" => {
                        b.return_damage = self.data(a, "DataA", lvl);
                        b.return_pct = self.data(a, "DataB", lvl) != 0.0;
                    }
                    // Regeneration (a healing ward): DataA life a second, of maximum life
                    // when DataB.
                    "Aoar" => {
                        if self.data(a, "DataB", lvl) != 0.0 {
                            b.life_regen_pct = self.data(a, "DataA", lvl);
                        } else {
                            b.life_regen = self.data(a, "DataA", lvl);
                        }
                    }
                    // War Drums and Command Aura: attack damage increase (DataA).
                    "Aakb" | "AOac" => b.damage_factor = self.data(a, "DataA", lvl),
                    // Slow Aura (a tornado): enemies move (DataA) and attack (DataB) slower.
                    "Aasl" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.attack_factor = self.data(a, "DataB", lvl);
                    }
                    _ => continue,
                }
                let area = self.data(a, "Area", lvl);
                let enemies = code == "Aasl";
                grants.push((h, u.owner, u.x, u.y, area, b, enemies));
            }
        }
        for (src, owner, x, y, area, b, enemies) in grants {
            let targets: Vec<u32> = self
                .unit_order
                .iter()
                .copied()
                .filter(|&t| {
                    self.unit(t).is_some_and(|u| !u.dead && !u.is_building && (u.x - x).hypot(u.y - y) <= area && crate::natives::players::is_ally(self, owner, u.owner) != enemies)
                })
                .collect();
            for t in targets {
                let _ = src;
                self.apply_buff(t, b.clone());
            }
        }
    }

    /// Spell damage: the DamageBonusSpells table applies, armor value does not.
    pub fn spell_damage(&mut self, vm: &mut Vm, source: u32, target: u32, amount: f32) {
        let Some(t) = self.unit(target) else { return };
        if t.dead || t.invulnerable || self.buff_mods(target).5 {
            return;
        }
        // Magic immune units ignore spell damage (MagicImmunesResistDamage).
        if self.is_magic_immune(target) && self.misc_f32("MagicImmunesResistDamage", 1.0) != 0.0 {
            return;
        }
        let def_type = self.objects.units.get(t.type_id).map(|o| o.str("defType").to_ascii_lowercase()).unwrap_or_default();
        let col = ["small", "medium", "large", "fort", "normal", "hero", "divine"].iter().position(|c| *c == def_type).unwrap_or(7);
        let mult: f32 = self.misc.get_list("Misc", "DamageBonusSpells").get(col).and_then(|v| v.trim().parse().ok()).unwrap_or(1.0);
        // Ethereal units take EtherealDamageBonus's SPELLS factor.
        let ethereal: f32 = if self.is_ethereal(target) { self.misc.get_list("Misc", "EtherealDamageBonus").get(5).and_then(|v| v.trim().parse().ok()).unwrap_or(1.0) } else { 1.0 };
        let dmg = amount * mult * ethereal;
        let owner = t.owner;
        self.wake(target);
        let t = self.unit_mut(target).unwrap();
        t.life -= dmg;
        let dead = t.life <= 0.405;
        let ctx = EventCtx { unit: target, other_unit: source, damage: dmg, event_id: EVENT_UNIT_DAMAGED, player: owner, has_player: true, ..Default::default() };
        self.fire(vm, ctx, |_, k| matches!(k, EventKind::Unit { unit, event } if *unit == target && *event == EVENT_UNIT_DAMAGED));
        if dead {
            crate::natives::units::kill_unit(self, vm, target, source);
        }
    }

    pub(crate) fn heal(&mut self, target: u32, amount: f32) {
        if let Some(u) = self.unit_mut(target) {
            if !u.dead {
                u.life = (u.life + amount).min(u.max_life);
            }
        }
    }

    pub(crate) fn spawn_effect(&mut self, model: &str, x: f32, y: f32) {
        if model.is_empty() {
            return;
        }
        let z = self.ground_z(x, y);
        let e = self.handles.insert(Obj::Effect(Effect { model: model.to_string(), x, y, z, target: None, destroyed: true, facing: 0.0 }));
        self.dying_effects.push((e, self.time));
    }

    pub(crate) fn profile_art(&self, id: Id, key: &str) -> String {
        self.objects.abilities.get(id).and_then(|o| o.list(key).first().cloned()).unwrap_or_default()
    }

    pub(crate) fn is_undead(&self, h: u32) -> bool {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.str("type").to_ascii_lowercase().split(',').any(|c| c.trim() == "undead"))
    }

    pub(crate) fn enemies_in(&self, owner: u8, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && (u.x - x).hypot(u.y - y) <= r && !crate::natives::players::is_ally(self, owner, u.owner)))
            .collect()
    }

    /// Checks mana and cooldown; returns the interface error key when the cast cannot start.
    pub fn can_cast(&self, h: u32, a: Id) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        let lvl = self.ability_level(h, a);
        if lvl == 0 {
            return Err("");
        }
        if u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0 {
            return Err("Cooldown");
        }
        if u.mana < self.data(a, "Cost", lvl) {
            return Err("Nomana");
        }
        Ok(())
    }

    /// One step of a cast order: approach, face, wait for the cast point, then fire.
    pub fn cast_step(&mut self, vm: &mut Vm, h: u32, a: Id, target: u32, x: f32, y: f32, dt: f32) {
        let lvl = self.ability_level(h, a);
        if lvl == 0 || self.can_cast(h, a).is_err() && self.unit(h).is_some_and(|u| u.casting.is_none()) {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Idle;
            }
            return;
        }
        let range = self.data(a, "Rng", lvl);
        let goal = if target != 0 { self.widget_pos(target) } else { Some((x, y)) };
        let Some(goal) = goal else {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        };
        if target != 0 && self.unit(target).is_some_and(|t| t.dead) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        let Some(u) = self.unit(h) else { return };
        if u.casting.is_none() {
            let d = (goal.0 - u.x).hypot(goal.1 - u.y) - u.collision;
            if self.targeting(a) != Targeting::None && d > range {
                if u.path.is_empty() || u.repath_timer <= 0.0 {
                    self.path_to(h, goal);
                }
                self.walk(h, dt);
                return;
            }
            let u = self.unit_mut(h).unwrap();
            u.path.clear();
            if self.targeting(a) != Targeting::None {
                let u = self.unit_mut(h).unwrap();
                let want = crate::sim::angle_to((u.x, u.y), goal);
                let left = crate::sim::turn(u, want, dt);
                if left.abs() > 15.0 {
                    return;
                }
            }
            let anim = self.objects.abilities.get(a).map(|o| o.str("Animnames").replace(',', " ")).unwrap_or_default();
            let u = self.unit_mut(h).unwrap();
            u.casting = Some((0.0, false));
            u.anim = if anim.is_empty() { "spell".into() } else { anim };
            u.anim_serial = u.anim_serial.wrapping_add(1);
            // The cast begins: channel, then cast.
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_CHANNEL, EVENT_UNIT_SPELL_CHANNEL, ctx.clone());
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_CAST, EVENT_UNIT_SPELL_CAST, ctx);
            return;
        }
        let (elapsed, fired) = u.casting.unwrap();
        let elapsed = elapsed + dt;
        let o = self.objects.units.get(u.type_id);
        let (castpt, backsw) = o.map_or((0.3, 0.5), |o| (o.real("castpt"), o.real("castbsw")));
        if !fired && elapsed >= castpt {
            let cost = self.data(a, "Cost", lvl);
            let cool = self.data(a, "Cool", lvl);
            let u = self.unit_mut(h).unwrap();
            u.mana -= cost;
            u.cooldowns.insert(a, cool);
            u.casting = Some((elapsed, true));
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_EFFECT, EVENT_UNIT_SPELL_EFFECT, ctx);
            // CasterArt at the caster and TargetArt at the target, as the ability lists.
            let (cx, cy) = self.unit(h).map_or(goal, |u| (u.x, u.y));
            let caster_art = self.profile_art(a, "CasterArt");
            self.spawn_effect(&caster_art, cx, cy);
            if target != 0 {
                let target_art = self.profile_art(a, "TargetArt");
                self.spawn_effect(&target_art, goal.0, goal.1);
            }
            self.ability_effect(vm, h, a, lvl, target, goal);
            return;
        }
        let u = self.unit_mut(h).unwrap();
        // A channeled spell holds the caster until it ends (update_channels).
        if u.channel.is_some() {
            u.casting = Some((elapsed, fired));
            return;
        }
        if fired && elapsed >= castpt + backsw {
            u.casting = None;
            // An autocast resumes what it interrupted.
            u.action = u.extra.resume.take().unwrap_or(Action::Idle);
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_FINISH, EVENT_UNIT_SPELL_FINISH, ctx.clone());
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_ENDCAST, EVENT_UNIT_SPELL_ENDCAST, ctx);
            let u = self.unit_mut(h).unwrap();
            u.anim = "stand".into();
            u.anim_serial = u.anim_serial.wrapping_add(1);
        } else {
            u.casting = Some((elapsed, fired));
        }
    }

    /// What each ability code does when cast.
    fn ability_effect(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32, (x, y): (f32, f32)) {
        let code = self.ability_code(a);
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let hero_target = self.unit(target).is_some_and(|t| t.is_hero);
        let dur = if hero_target { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
        let buff_id = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().trim());
        let area = self.data(a, "Area", lvl);
        match code.as_str() {
            // Entangle Gold Mine: the mine is entangled after the casting time.
            "Aent" => {
                let cast = self.data(a, "Cast", lvl);
                self.pending_entangles.push((self.time + cast, h, target));
            }
            // Storm Bolt: a missile that deals DataA damage and stuns.
            "AHtb" => {
                let model = self.profile_art(a, "Missileart");
                let speed = self.profile_art(a, "Missilespeed").parse().unwrap_or(1000.0);
                let src = self.unit(h).unwrap();
                let (sx, sy) = (src.x, src.y);
                let z = self.ground_z(sx, sy) + 60.0;
                self.missiles.push(crate::sim::Missile {
                    source: h,
                    target,
                    model,
                    x: sx,
                    y: sy,
                    z,
                    speed,
                    arc: 0.0,
                    damage: self.data(a, "DataA", lvl),
                    attack_type: String::new(),
                    start: (sx, sy),
                    spell: Some((a, lvl, dur, buff_id)),
                });
            }
            // Thunder Clap: area damage (DataA) and slow (DataC movement, DataD attack).
            "AHtc" | "AOws" => {
                let (cx, cy) = self.unit(h).map_or((x, y), |u| (u.x, u.y));
                let dmg = self.data(a, "DataA", lvl);
                for t in self.enemies_in(owner, cx, cy, area) {
                    self.spell_damage(vm, h, t, dmg);
                    let hero = self.unit(t).is_some_and(|u| u.is_hero);
                    let d = if hero { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    let mut b = Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, ..Default::default() };
                    if code == "AOws" {
                        b.stun = true;
                    } else {
                        b.move_factor = -self.data(a, "DataC", lvl);
                        b.attack_factor = -self.data(a, "DataD", lvl);
                    }
                    self.apply_buff(t, b);
                }
                let art = self.profile_art(a, "CasterArt");
                self.spawn_effect(&art, cx, cy);
            }
            // Holy Light: heals living allies (DataA) or damages undead enemies (half).
            "AHhb" => {
                let amount = self.data(a, "DataA", lvl);
                if self.is_undead(target) {
                    self.spell_damage(vm, h, target, amount / 2.0);
                } else {
                    self.heal(target, amount);
                }
                let art = self.profile_art(a, "TargetArt");
                if let Some((tx, ty)) = self.widget_pos(target) {
                    self.spawn_effect(&art, tx, ty);
                }
            }
            // Divine Shield: invulnerable for the duration.
            "AHds" => {
                let d = self.data(a, "Dur", lvl).max(self.data(a, "HeroDur", lvl));
                self.apply_buff(h, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, invulnerable: true, ..Default::default() });
            }
            // Heal: restores DataA hit points.
            "Ahea" => self.heal(target, self.data(a, "DataA", lvl)),
            // Death Coil: heals undead allies (DataA) or damages living enemies (half).
            "AUdc" => {
                let amount = self.data(a, "DataA", lvl);
                if self.is_undead(target) {
                    self.heal(target, amount);
                } else {
                    self.spell_damage(vm, h, target, amount / 2.0);
                }
            }
            // Frost Nova: target damage (DataB), area damage (DataA), and a slow.
            "AUfn" => {
                let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
                self.spell_damage(vm, h, target, self.data(a, "DataB", lvl));
                for t in self.enemies_in(owner, tx, ty, area) {
                    self.spell_damage(vm, h, t, self.data(a, "DataA", lvl));
                    let hero = self.unit(t).is_some_and(|u| u.is_hero);
                    let d = if hero { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    self.apply_buff(t, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, move_factor: -0.5, attack_factor: -0.25, ..Default::default() });
                }
            }
            // Slow: movement (DataA) and attack speed (DataB) factors.
            "Aslo" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -self.data(a, "DataA", lvl), attack_factor: -self.data(a, "DataB", lvl), ..Default::default() }),
            // Bloodlust: attack speed (DataA) and movement (DataB) factors.
            "Ablo" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, attack_factor: self.data(a, "DataA", lvl), move_factor: self.data(a, "DataB", lvl), ..Default::default() }),
            // Inner Fire: damage factor (DataA) and armor (DataB).
            "Ainf" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, damage_factor: self.data(a, "DataA", lvl), armor: self.data(a, "DataB", lvl), ..Default::default() }),
            // Ensnare, Web, and Entangling Roots hold the target in place.
            "Aens" | "Aweb" | "AEer" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -1.0, dps: if code == "AEer" { self.data(a, "DataA", lvl) } else { 0.0 }, ..Default::default() }),
            // Hurl Boulder, Firebolt, and similar: damage and stun.
            "ACtb" | "ANfb" => {
                self.spell_damage(vm, h, target, self.data(a, "DataA", lvl));
                self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, stun: true, ..Default::default() });
            }
            other => {
                if !self.unit_spell_effect(vm, h, a, lvl, other, target, (x, y)) && !self.spell_effect(vm, h, a, lvl, other, target, (x, y)) {
                    log::debug!("ability {other} has no effect yet");
                }
            }
        }
    }

    /// Passive attack modifiers: Bash and Critical Strike.  Returns the extra damage factor
    /// and flat bonus for this hit, and applies stuns.
    pub fn on_attack_passives(&mut self, attacker: u32, target: u32, damage: f32) -> f32 {
        let Some(u) = self.unit(attacker) else { return damage };
        let abilities = u.abilities.clone();
        let mut damage = damage;
        for (a, lvl) in abilities {
            let lvl = lvl.max(1) as u32;
            match self.ability_code(a).as_str() {
                // Bash: DataA% chance; DataC bonus damage; stun for Dur/HeroDur.
                "AHbh" => {
                    if (self.rng.int(1, 100) as f32) <= self.data(a, "DataA", lvl) {
                        damage += self.data(a, "DataC", lvl);
                        let hero = self.unit(target).is_some_and(|t| t.is_hero);
                        let d = if hero { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                        let buff = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().trim());
                        self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: attacker, remaining: d, stun: true, ..Default::default() });
                    }
                }
                // Critical Strike: DataA% chance to deal DataB times damage.
                "AOcr" => {
                    if (self.rng.int(1, 100) as f32) <= self.data(a, "DataA", lvl) {
                        damage = damage * self.data(a, "DataB", lvl) + self.data(a, "DataC", lvl);
                    }
                }
                _ => {}
            }
        }
        damage
    }

    /// Evasion: whether the target dodges this attack.
    pub fn evades(&mut self, target: u32) -> bool {
        let Some(u) = self.unit(target) else { return false };
        let abilities = u.abilities.clone();
        for (a, lvl) in abilities {
            if self.ability_code(a) == "AEev" {
                let chance = self.data(a, "DataA", lvl.max(1) as u32) * 100.0;
                if (self.rng.int(1, 100) as f32) <= chance {
                    return true;
                }
            }
        }
        false
    }
}
