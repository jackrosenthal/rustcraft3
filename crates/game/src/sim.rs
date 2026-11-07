//! Unit behavior: orders, movement, target acquisition, attacks, missiles, and death.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::natives::players::is_ally;
use crate::natives::units::kill_unit;
use crate::codes;

fn d_kill(w: &mut World, vm: &mut Vm, h: u32) {
    if let Some(Obj::Destructable(d)) = w.handles.get_mut(h) {
        d.life = 0.0;
    }
    crate::natives::objects::unstamp_destructable(w, h);
    let ctx = EventCtx { destructable: h, widget: h, ..Default::default() };
    w.fire(vm, ctx, |_, k| matches!(k, EventKind::Death(x) if *x == h));
}
use crate::objects::Id;
use crate::types::*;
use crate::world::{Request, World};

/// A projectile in flight.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Missile {
    /// Which missile this is, for the renderer to follow (given when it starts).
    #[serde(default)]
    pub id: u32,
    pub source: u32,
    pub target: u32,
    pub model: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub speed: f32,
    pub arc: f32,
    pub damage: f32,
    pub attack_type: String,
    pub start: (f32, f32),
    /// A spell missile: ability, level, stun duration, and buff type.
    pub spell: Option<(Id, u32, f32, Id)>,
    /// An attack's weapon, for its splash and spill, and where artillery lands.
    pub weapon: Option<Weapon>,
    pub impact: Option<(f32, f32)>,
    /// A bouncing attack (mbounce): the bounces left and the units it has hit.
    pub bounces: i32,
    pub hit: Vec<u32>,
    /// A Stampede beast: its ability, level, and the time until it next looks for
    /// something to strike (stampede.rs).
    #[serde(default)]
    pub stampede: Option<(Id, u32, f32)>,
    /// A Volcano's rock: its ability and level (volcano.rs).
    #[serde(default)]
    pub rock: Option<(Id, u32)>,
    /// An attack Defend sent back, which cannot be deflected again.
    #[serde(default)]
    pub reflected: bool,
    /// An attack's missile's objects (`crate::bullets`).
    #[serde(default)]
    pub bullet: Option<crate::bullets::Bullet>,
}

/// Weapon 1 of a unit type, from UnitWeapons.slk and the unit's profile.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Weapon {
    pub range: f32,
    /// The unit's minimum range (UnitWeapons minRange): nothing closer is attacked.
    pub min_range: f32,
    pub cooldown: f32,
    pub damage_point: f32,
    pub backswing: f32,
    pub dice: i32,
    pub sides: i32,
    pub base: f32,
    pub attack_type: String,
    pub weapon_type: String,
    pub targets: String,
    pub missile: String,
    pub missile_speed: f32,
    pub missile_arc: f32,
    /// Splash: full damage within Farea, Hfact within Harea, Qfact within Qarea, to
    /// what splashTargs allows.
    pub full_area: f32,
    pub half_area: f32,
    pub quarter_area: f32,
    pub half_factor: f32,
    pub quarter_factor: f32,
    pub splash_targets: String,
    /// Line spill (mline, aline): damage less damageLoss to units within spillRadius
    /// of the line spillDist past the target; bounces (mbounce) to targCount targets.
    pub damage_loss: f32,
    pub spill_dist: f32,
    pub spill_radius: f32,
    pub target_count: i32,
}

/// A unit type's first enabled weapon.
pub fn weapon(w: &World, type_id: Id) -> Option<Weapon> {
    let o = w.objects.units.get(type_id)?;
    let mask = o.int("weapsOn");
    (1..=2).find(|n| mask & (1 << (n - 1)) != 0).and_then(|n| weapon_n(w, type_id, n))
}

/// Weapon 1 or 2 of a unit type (UnitWeapons fields with the number; the missile art
/// lists hold one entry per weapon).
pub fn weapon_n(w: &World, type_id: Id, n: usize) -> Option<Weapon> {
    if let Some(wp) = w.weapon_cache.borrow().get(&(type_id, n)) {
        return wp.clone();
    }
    let wp = weapon_n_read(w, type_id, n);
    w.weapon_cache.borrow_mut().insert((type_id, n), wp.clone());
    wp
}

fn weapon_n_read(w: &World, type_id: Id, n: usize) -> Option<Weapon> {
    let o = w.objects.units.get(type_id)?;
    let f = |k: &str| format!("{k}{n}");
    let pick = |k: &str| -> String {
        let l = o.list(k);
        l.get(n - 1).or_else(|| l.first()).cloned().unwrap_or_default()
    };
    // The attack takes its weapons' times and reach as CAbilityAttack `FUN_6f49cab0`
    // reads them: the damage point and backswing at least 0.01, the cooldown at least
    // 0.1 and at least the damage point and 0.02, the backswing ending 0.01 before the
    // cooldown at the latest (at least 0.01 still), and the range no farther than the
    // unit acquires (Mafarazzo TD's towers of cooldown 0.01 to 0.3 and damage point 0.3
    // attack every 0.32 seconds, measured; its 200s Missile Tank, range 1500, acquire
    // 1200, reaches 1200).
    let add = |a: f32, b: f32| f32::from_bits(crate::efloat::add(a.to_bits(), b.to_bits()));
    let sub = |a: f32, b: f32| f32::from_bits(crate::efloat::sub(a.to_bits(), b.to_bits()));
    let damage_point = o.real(&f("dmgpt")).max(POINT_BEFORE_COOLDOWN);
    let mut backswing = o.real(&f("backSw")).max(POINT_BEFORE_COOLDOWN);
    let mut cooldown = o.real(&f("cool")).max(LEAST_COOLDOWN);
    if cooldown < add(damage_point, COOLDOWN_AFTER_POINT) {
        cooldown = add(damage_point, COOLDOWN_AFTER_POINT);
    }
    if cooldown < add(add(damage_point, backswing), POINT_BEFORE_COOLDOWN) {
        backswing = sub(sub(cooldown, damage_point), POINT_BEFORE_COOLDOWN).max(POINT_BEFORE_COOLDOWN);
    }
    let (range, acquire) = (o.real(&f("rangeN")), o.real("acquire"));
    let range = if range > acquire && (range - acquire).abs() >= 0.001 { acquire } else { range };
    Some(Weapon {
        range,
        min_range: o.real("minRange"),
        cooldown,
        damage_point,
        backswing,
        dice: o.int(&f("dice")),
        sides: o.int(&f("sides")),
        base: o.real(&f("dmgplus")),
        attack_type: o.str(&f("atkType")).to_ascii_lowercase(),
        weapon_type: o.str(&f("weapTp")).to_ascii_lowercase(),
        targets: o.str(&f("targs")).to_ascii_lowercase(),
        missile: pick("Missileart"),
        missile_speed: pick("Missilespeed").trim().parse().unwrap_or(0.0),
        missile_arc: pick("Missilearc").trim().parse().unwrap_or(0.0),
        full_area: o.real(&f("Farea")),
        half_area: o.real(&f("Harea")),
        quarter_area: o.real(&f("Qarea")),
        half_factor: o.real(&f("Hfact")),
        quarter_factor: o.real(&f("Qfact")),
        splash_targets: o.str(&f("splashTargs")).to_ascii_lowercase(),
        damage_loss: o.real(&f("damageLoss")),
        spill_dist: o.real(&f("spillDist")),
        spill_radius: o.real(&f("spillRadius")),
        target_count: o.int(&f("targCount")),
    })
}

/// A unit's enabled weapons, numbered: its type's weapsOn bits, or the ones its form
/// enables (rooted or uprooted ancients).
/// What steps of f32 leave of a whole time: none (thirty 0.02s fall 1e-7 short of 0.6).
/// Measured: a Watch Tower, cooldown 0.6, strikes every 0.60 seconds; ours, without
/// this, every 0.62.
pub(crate) const STEP_SLACK: f32 = 1e-4;

/// The least time from an attack's damage point to its cooldown's end (0x6fcd53a4,
/// `FUN_6f49d130`), and from its backswing's end to the cooldown's (`FUN_6f49d050`);
/// also a weapon's least damage point and backswing (`FUN_6f49cab0`).
const POINT_BEFORE_COOLDOWN: f32 = 0.01;
/// The least cooldown left at an attack's damage point (0x6fcd53a8, `FUN_6f49d050`),
/// and a weapon's least cooldown past its damage point (`FUN_6f49cab0`).
const COOLDOWN_AFTER_POINT: f32 = 0.02;
/// A weapon's least cooldown (0x6fcd53c4, `FUN_6f49cab0`).
const LEAST_COOLDOWN: f32 = 0.1;
/// A bound on a quick weapon's attacks in one step, should a step be long.
const QUICK_SWINGS_PER_STEP: u32 = 1000;
/// How far into a tick a swing begins that its unit was ready for since an earlier one
/// (provisional: three main ticks, `World::try_attack`; a structure's first on a target
/// it took up is timed from its look, `crate::tasks`).
const SWING_START_IN_TICK: f32 = 0.015;

/// Whether a swing is quick, and its damage point (in the swing's time, its unit's
/// attack speed `speed`).  A swing whose backswing runs to its cooldown's end is paced
/// by the cooldown: the damage point comes POINT_BEFORE_COOLDOWN before the cooldown's
/// end at the latest (`FUN_6f49d130`), the cooldown has COOLDOWN_AFTER_POINT left at
/// least then and the backswing ends POINT_BEFORE_COOLDOWN before it (`FUN_6f49d050`),
/// and the cooldown's end starts the next attack at once (`FUN_6f49a4f0`), within the
/// step if it falls there (measured: Mafarazzo TD's Plague Tower, cooldown 0.2 read as
/// 0.32, attacks every 0.32 seconds).
fn swing_point(wp: &Weapon, known: bool, speed: f32) -> (bool, f32) {
    let quick = known && (wp.cooldown - wp.damage_point - wp.backswing) / speed <= POINT_BEFORE_COOLDOWN + STEP_SLACK;
    (quick, if quick { wp.damage_point.min(wp.cooldown - POINT_BEFORE_COOLDOWN * speed) } else { wp.damage_point })
}

pub fn unit_weapons(w: &World, u: &Unit) -> Vec<(u8, Weapon)> {
    // A burrow shoots only with workers inside.
    if u.extra.cargo.is_empty() && w.ability_with_code(u.handle, codes::ability::CARGO_HOLD_BURROW).is_some() {
        return Vec::new();
    }
    let mut mask = u.extra.weapons.unwrap_or_else(|| w.objects.units.get(u.type_id).map_or(0, |o| o.int("weapsOn")));
    // A rooted ancient that has not changed form uses its rooted weapons (Aroo DataA;
    // measured: an Ancient Protector made rooted throws at a peasant 150 away, within
    // its 128 melee reach, every 2 seconds, its second weapon's cooldown).
    if u.extra.weapons.is_none() && !u.extra.uprooted {
        if let Some((a, l)) = w.ability_with_code(u.handle, codes::ability::ROOT) {
            if let Some(m) = w.objects.abilities.get(a).map(|o| o.level_real("DataA", l) as i32).filter(|&m| m > 0) {
                mask = m;
            }
        }
    }
    // Research can switch weapons on (renw: Flying Machine Bombs, Vorpal Blades).
    if u.extra.weapons.is_none() {
        if let Some(m) = w.upgrade_bonus(u.owner, u.type_id).weapons {
            mask = m;
        }
    }
    // Cloud keeps melee (1) or ranged (2) attacks from firing.
    let disarm = u.buffs.iter().fold(0u8, |m, b| m | b.disarm);
    (1..=2u8)
        .filter(|n| mask & (1 << (n - 1)) != 0)
        .filter_map(|n| weapon_n(w, u.type_id, n as usize).map(|wp| (n, wp)))
        .filter(|(_, wp)| disarm & if wp.weapon_type == "normal" { 1 } else { 2 } == 0)
        .collect()
}

impl World {
    /// Whether only the minimum range keeps a unit's weapons from a target.
    fn inside_min_range(&self, h: u32, target: u32) -> bool {
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return false };
        if is_ally(self, u.owner, t.owner) || weapon_for(self, u, t).is_some() {
            return false;
        }
        unit_weapons(self, u).into_iter().any(|(_, wp)| wp.min_range > 0.0 && can_target(self, &Weapon { min_range: 0.0, ..wp }, u, t))
    }
}

/// The first of a unit's weapons that can hit a target.
/// The weapon a unit attacks a target with: the first that may and reaches it, else
/// the first that may (measured: an Ancient Protector strikes a peasant 300 away every 2
/// seconds, its second weapon's, the first reaching only 128).
pub fn weapon_for(w: &World, u: &Unit, t: &Unit) -> Option<(u8, Weapon)> {
    let allowed: Vec<(u8, Weapon)> = unit_weapons(w, u).into_iter().filter(|(_, wp)| can_target(w, wp, u, t)).collect();
    let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - t.collision;
    allowed.iter().find(|(_, wp)| d <= wp.range).or(allowed.first()).cloned()
}

/// A unit's weapon by its number (`weapon_numbered`), for the other modules.
pub(crate) fn weapon_of(w: &World, u: &Unit, n: u8) -> Option<Weapon> {
    weapon_numbered(w, u, n)
}

/// The weapon a unit swings with.
fn weapon_numbered(w: &World, u: &Unit, n: u8) -> Option<Weapon> {
    unit_weapons(w, u).into_iter().find(|x| x.0 == n).map(|x| x.1).or_else(|| unit_weapons(w, u).into_iter().next().map(|x| x.1))
}

fn misc_list(w: &World, key: &str) -> Vec<f32> {
    w.misc.get_list("Misc", key).iter().filter_map(|v| v.trim().parse().ok()).collect()
}

/// Damage after attack type, armor type, and armor value (Units\MiscGame.txt
/// DamageBonus* tables and DefenseArmor).
pub fn final_damage(w: &World, raw: f32, attack_type: &str, target: &Unit) -> f32 {
    let o = w.objects.units.get(target.type_id);
    let def_type = w.defense_type(target);
    let armor = o.map_or(0.0, |o| o.real("def")) + w.buff_mods(target.handle).2;
    let key = match attack_type {
        "pierce" => "DamageBonusPierce",
        "siege" => "DamageBonusSiege",
        "magic" => "DamageBonusMagic",
        "chaos" => "DamageBonusChaos",
        "spells" => "DamageBonusSpells",
        "hero" => "DamageBonusHero",
        _ => "DamageBonusNormal",
    };
    // Columns: small, medium, large, fort, normal, hero, divine, none.
    let col = match def_type.as_str() {
        "small" => 0,
        "medium" => 1,
        "large" => 2,
        "fort" => 3,
        "normal" => 4,
        "hero" => 5,
        "divine" => 6,
        _ => 7,
    };
    let mult = misc_list(w, key).get(col).copied().unwrap_or(1.0);
    // Heroes gain armor from agility (AgiDefenseBonus per point, plus AgiDefenseBase).
    let armor = armor + target.item_bonus.armor + w.upgrade_bonus(target.owner, target.type_id).armor;
    let armor = if target.is_hero {
        armor + (target.agi + target.item_bonus.agi).floor() * w.misc_f32("AgiDefenseBonus", 0.3) + w.misc_f32("AgiDefenseBase", -2.0)
    } else {
        armor
    };
    let k = w.misc_f32("DefenseArmor", 0.06);
    let armor_factor = if w.damage_ignores_armor {
        1.0
    } else if armor >= 0.0 {
        1.0 - (armor * k) / (1.0 + armor * k)
    } else {
        2.0 - (1.0 - k).powf(-armor)
    };
    raw * mult * armor_factor
}

pub(crate) fn can_target(w: &World, wp: &Weapon, attacker: &Unit, t: &Unit) -> bool {
    if t.dead || t.hidden || w.unit_invulnerable(t) || w.is_locust(t) || t.handle == attacker.handle {
        return false;
    }
    // Siege weapons cannot hit what is inside their minimum range (measured like
    // their range, from edge to edge).
    if wp.min_range > 0.0 && (t.x - attacker.x).hypot(t.y - attacker.y) - attacker.collision - w.reach_radius(t.handle) < wp.min_range {
        return false;
    }
    let flying = w.objects.units.get(t.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
    let wants = |k: &str| wp.targets.split(',').any(|x| x.trim() == k);
    if flying {
        wants("air")
    } else if t.is_building {
        wants("structure") || wants("ground")
    } else {
        wants("ground")
    }
}

pub(crate) fn set_anim(u: &mut Unit, name: &str) {
    if u.anim != name {
        u.anim = name.to_string();
        u.anim_serial = u.anim_serial.wrapping_add(1);
    }
}

pub(crate) fn restart_anim(u: &mut Unit, name: &str) {
    u.anim = name.to_string();
    u.anim_serial = u.anim_serial.wrapping_add(1);
}

pub(crate) fn angle_to(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.1 - from.1).atan2(to.0 - from.0).to_degrees()
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (b - a + 540.0).rem_euclid(360.0) - 180.0
}

/// Turns toward `target_deg`; returns the remaining difference.
pub(crate) fn turn(u: &mut Unit, target_deg: f32, dt: f32) -> f32 {
    // Turn rate: the data's turnRate in radians every thirtieth of a second (measured: a
    // footman facing away starts walking once within its propulsion window and swinging
    // once within AttackHalfAngle as this rate gives).
    let rate = (u.turn_speed.max(0.05) * 30.0).to_degrees() * dt;
    let d = angle_diff(u.facing, target_deg);
    if d.abs() <= rate {
        u.facing = target_deg.rem_euclid(360.0);
        0.0
    } else {
        u.facing = (u.facing + rate * d.signum()).rem_euclid(360.0);
        d - rate * d.signum()
    }
}

impl World {
    /// Issues a unit order from a player or script, replacing the current one.
    /// An order from the player: at once, or (`queue`, Shift held) after the orders
    /// the unit is already carrying out.
    pub fn issue_order(&mut self, h: u32, order: i32, x: f32, y: f32, target: u32, queue: bool) {
        let busy = self.unit(h).is_some_and(|u| !matches!(u.action, Action::Idle) || !u.extra.order_queue.is_empty());
        if queue && busy {
            if let Some(u) = self.unit_mut(h) {
                u.extra.order_queue.push_back((order, x, y, target));
            }
            return;
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.order_queue.clear();
        }
        self.order_error = None;
        self.order_unit(h, order, x, y, target);
        if self.order_error.is_none() {
            let ctx = self.order_event(h, order, x, y, target);
            self.queue_unit_event(h, ctx.0, ctx.1, ctx.2);
        }
    }

    /// EVENT_UNIT_ACQUIRED_TARGET when a unit takes up a target of its own accord (once
    /// per target).
    pub(crate) fn acquired(&mut self, h: u32, t: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        if u.extra.acquired == t {
            return;
        }
        let was = u.extra.acquired;
        u.extra.acquired = t;
        log::trace!("{h} takes up {t} (was {was}) at {:.4}, its life {:?}", self.time, self.unit(t).map(|x| (x.life, x.dead)));
        // A structure's attack is its tasks (`crate::tasks`), from its look's time.
        if self.attacks_in_place(h) {
            let at = self.look_time(h);
            self.attack_taken_up(h, at);
        }
        self.queue_unit_event(h, 0, EVENT_UNIT_ACQUIRED_TARGET, EventCtx { target_unit: t, ..Default::default() });
    }

    /// When a unit's look for a target this tick fell due: as far back from the tick's
    /// end as its wait has run past it.
    fn look_time(&self, h: u32) -> f64 {
        // A structure's is its timer's due, from which its attack counts on, on the agent
        // queue (read: the flyer game's Arrow Tower's look due 21.662787, its damage point
        // 0.1 and 0.3 on, 22.062787, its missile sent by the main clock 22.062298;
        // `World::bullet_aimed`).
        if self.attacks_in_place(h) {
            // (Or one carried from the last tick, due after its agent queue's reach,
            // `World::update_units`: traced, the upgrade game's second h00X's look due
            // 50.59565 run at turn 507's start, its first shell 0.4 after.)
            if let Some((_, due, _)) = self.unit(h).and_then(|u| u.extra.look_timer) {
                let due = due as f64;
                if due > self.time_exact - 2.0 * self.tick_dt as f64 - 1e-6 && due <= self.time_exact + 1e-6 {
                    return due;
                }
            }
        }
        let late = self.unit(h).map_or(0.0, |u| u.acquire_timer.clamp(-self.tick_dt, 0.0));
        self.time_exact + late as f64
    }

    /// The issued-order events for an order (its kind by target) and their context:
    /// a target order's target unit, item, or destructable, a point order's point.
    pub(crate) fn order_event(&self, h: u32, order: i32, x: f32, y: f32, target: u32) -> (i32, i32, EventCtx) {
        let _ = h;
        let mut ctx = EventCtx { order_id: order, order_x: x, order_y: y, ..Default::default() };
        if target != 0 {
            match self.handles.get(target) {
                Some(Obj::Item(_)) => ctx.item = target,
                Some(Obj::Destructable(_)) => ctx.destructable = target,
                _ => ctx.target_unit = target,
            }
            if let Some((tx, ty)) = self.widget_pos(target) {
                ctx.order_x = tx;
                ctx.order_y = ty;
            }
            (EVENT_PLAYER_UNIT_ISSUED_TARGET_ORDER, EVENT_UNIT_ISSUED_TARGET_ORDER, ctx)
        } else if x != 0.0 || y != 0.0 {
            (EVENT_PLAYER_UNIT_ISSUED_POINT_ORDER, EVENT_UNIT_ISSUED_POINT_ORDER, ctx)
        } else {
            (EVENT_PLAYER_UNIT_ISSUED_ORDER, EVENT_UNIT_ISSUED_ORDER, ctx)
        }
    }

    pub fn order_unit(&mut self, h: u32, order: i32, x: f32, y: f32, target: u32) {
        // Shadowmeld's order leaves a hidden unit as it is (measured: an archer hidden at
        // night stays hidden and reads no order).
        if order == crate::natives::basic::order_id("ambush") && self.ability_with_code(h, codes::ability::SHADOW_MELD).is_some_and(|(a, _)| self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| b.ability == a))) {
            return;
        }
        // An order ends a new unit's first wait to look for targets (measured: a Demon
        // Hunter and a Blademaster ordered to Immolation and Bladestorm as they are made
        // strike the footman beside them 0.35 seconds in).
        // (Not an autocast toggle, nor a harvest of gold the unit cannot take: neither
        // stops its look, read: the quiet test map's priests turned to heal off and its
        // computer's ghoul told to harvest gold every quarter second each look once a
        // half second.)
        let name = crate::natives::basic::order_name(order).unwrap_or_default();
        let toggle = self.unit(h).is_some_and(|u| u.abilities.iter().any(|&(a, _)| self.objects.abilities.get(a).is_some_and(|o| o.str("Orderon").eq_ignore_ascii_case(name) || o.str("Orderoff").eq_ignore_ascii_case(name))));
        let no_gold = name == "autoharvestgold" && self.ability_with_code(h, codes::ability::HARVEST).is_none();
        if !toggle && !no_gold {
            if let Some(u) = self.unit_mut(h) {
                u.extra.acquired = 0;
                u.acquire_timer = 0.0;
            }
        }
        let smart = crate::natives::basic::order_id("smart");
        let mv = crate::natives::basic::order_id("move");
        let attack = crate::natives::basic::order_id("attack");
        let aimove = crate::natives::basic::order_id("AImove");
        let stop = crate::natives::basic::order_id("stop");
        let hold = crate::natives::basic::order_id("holdposition");
        let patrol = crate::natives::basic::order_id("patrol");
        let setrally = crate::natives::basic::order_id("setrally");
        let owner_of = |w: &World, t: u32| w.unit(t).map(|u| u.owner);
        // Transports: unloading one unit, everyone, or at a point; loading a unit.
        let unload = crate::natives::basic::order_id("unload");
        if order == unload && target != 0 && self.unit(target).is_some_and(|t| t.extra.transport == h) {
            self.disembark(target);
            return;
        }
        // Unloading everyone at a point goes there first (measured: a zeppelin told to
        // unload 400 away flies there with its cargo aboard).
        let unloadall = crate::natives::basic::order_id("unloadall");
        let at_point = order == unloadall && (x, y) != (0.0, 0.0);
        if order == unloadall && !at_point || order == crate::natives::basic::order_id("unloadallinstant") || (order == crate::natives::basic::order_id("standdown") && self.ability_with_code(h, codes::ability::CARGO_HOLD_BURROW).is_some()) {
            self.unload_all(h);
            return;
        }
        if (order == unload || at_point) && self.cargo_hold(h).is_some() {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Unload { x, y };
                u.path.clear();
                u.work_timer = 0.0;
            }
            return;
        }
        if (order == crate::natives::basic::order_id("load") || order == smart) && target != 0 && self.ability_with_code(h, codes::ability::LOAD).is_some() && self.can_board(h, target) {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Load { target };
                u.path.clear();
            }
            return;
        }
        // Battle Stations (Abtl): the owner's UnitID workers within Area head inside;
        // Chaos Cargo Load (Achl) makes them its UnitID instead (FUN_6f4c9ce0).
        if order == crate::natives::basic::order_id("battlestations") {
            if let Some((a, l)) = self.ability_with_code(h, codes::ability::BATTLESTATIONS) {
                let (area, mut id) = self.objects.abilities.get(a).map_or((0.0, [0; 4]), |o| (o.level_real("Area", l), crate::objects::id_from_str(o.level_str("UnitID", l).trim())));
                if let Some(o) = self.ability_with_code(h, codes::ability::CHAOS_CARGO_LOAD).and_then(|(c, cl)| self.objects.abilities.get(c).map(|o| (o, cl))) {
                    id = crate::objects::id_from_str(o.0.level_str("UnitID", o.1).trim());
                }
                let (owner, bx, by) = self.unit(h).map(|u| (u.owner, u.x, u.y)).unwrap_or_default();
                let workers: Vec<u32> = self.unit_order.iter().copied().filter(|&o| self.unit(o).is_some_and(|w| !w.dead && !w.hidden && w.owner == owner && w.type_id == id && (w.x - bx).hypot(w.y - by) <= area)).collect();
                for wk in workers {
                    self.leave_mine(wk);
                    if let Some(w) = self.unit_mut(wk) {
                        w.action = Action::Board { transport: h };
                        w.path.clear();
                    }
                }
            }
            return;
        }
        // Uprooting and rooting (Aroo).
        if order == crate::natives::basic::order_id("unroot") {
            self.uproot(h);
            return;
        }
        if order == crate::natives::basic::order_id("root") {
            self.root_at(h, x, y);
            return;
        }
        // entangleinstant: Entangle Gold Mine at once, without its casting time, the
        // order's objects made first (an order with its target and nine tasks: traced, a
        // night elf start's Tree of Life over LAN with the original, before its
        // entangled mine), freed as the next turn starts: its first block and its last
        // four tasks, the last first, before the entangled mine's two tasks, the rest
        // after them (read: the turn's first frees).
        if order == crate::natives::basic::order_id("entangleinstant") && self.ability_with_code(h, codes::ability::ENTANGLE).is_some() {
            const ENTANGLE_ORDER: [&crate::tags::Code; 10] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK_T, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK];
            let slots: Vec<u32> = ENTANGLE_ORDER.iter().map(|c| self.tags.take(c)).collect();
            for k in [1, 9, 8, 7, 6] {
                self.tags.doom(slots[k]);
            }
            self.entangle(h, target, true);
            for k in [5, 4, 3, 2, 0] {
                self.tags.doom(slots[k]);
            }
            return;
        }
        let me = match self.unit(h) {
            Some(u) if !u.dead && !u.is_building => u.owner,
            // Structures cast their abilities (Entangle Gold Mine).
            Some(u) if !u.dead && u.is_building && u.constructing.is_none() && self.ability_for_order(h, order).is_some() => u.owner,
            Some(u) if !u.dead && u.is_building && (order == smart || order == setrally) => {
                // Structures take a smart order as their rally point.
                let p = self.widget_pos(target).unwrap_or((x, y));
                let _ = u;
                let target = if self.widget_pos(target).is_some() { target } else { 0 };
                let u = self.unit_mut(h).unwrap();
                u.rally = Some(p);
                u.extra.rally_target = target;
                return;
            }
            _ => return,
        };
        // Defend on and off; Sacrifice at a sacrificial pit.
        if order == crate::natives::basic::order_id("defend") || order == crate::natives::basic::order_id("undefend") {
            if self.defend_order(h, order == crate::natives::basic::order_id("defend")) {
                return;
            }
        }
        if order == crate::natives::basic::order_id("sacrifice") && target != 0 {
            if self.ability_with_code(h, codes::ability::SACRIFICE_ACOLYTE).is_some() && self.sacrifice(h, target) {
                return;
            }
            if self.ability_with_code(h, codes::ability::SACRIFICE_SACRIFICIAL_PIT).is_some() && self.sacrifice(target, h) {
                return;
            }
        }
        // Autocast toggles, morphs (Bear Form and the like) by their order or unorder,
        // and toggled abilities turned off by their unorder.
        if let Some(name) = crate::natives::basic::order_name(order) {
            if self.autocast_order(h, name) || self.morph_order(h, name) || self.toggle_off_order(h, name) {
                return;
            }
        }
        // An attack on a unit only a weapon's minimum range keeps it from is refused
        // ("Target is inside minimum range."), the unit free to choose another.
        // The unit keeps the attack as its order, choosing its own targets meanwhile
        // (measured: a Mortar Team so ordered at a footman beside it shells another and
        // reads attack).
        if (order == attack || order == smart) && target != 0 && self.inside_min_range(h, target) {
            self.order_error = Some("UnderRange");
            if order == attack {
                if let Some(u) = self.unit_mut(h) {
                    u.order = Order { id: order, target_x: x, target_y: y, target };
                }
            }
            return;
        }
        let action = if order == stop {
            Action::Idle
        } else if order == hold {
            Action::Hold
        } else if (order == attack || order == aimove) && target != 0 {
            Action::Attack { target }
        } else if order == attack || order == aimove {
            // A computer's captain moves its units by AImove, as an attack (FUN_6f9d44d0).
            Action::AttackMove { x, y }
        } else if crate::natives::basic::order_name(order) == Some("attackground") {
            // Only a unit with an artillery weapon shells the ground.
            let artillery = self.unit(h).is_some_and(|u| unit_weapons(self, u).iter().any(|(_, w)| w.weapon_type == "artillery" || w.weapon_type == "aline"));
            if !artillery {
                return;
            }
            Action::AttackGround { x, y }
        } else if order == patrol {
            let p = self.unit(h).map(|u| (u.x, u.y)).unwrap_or_default();
            Action::Patrol { a: p, b: (x, y) }
        } else if (order == crate::natives::basic::order_id("repair") || order == smart) && target != 0 && self.unit(target).is_some_and(|t| t.extra.needs_builder && t.constructing.is_some()) && self.unit(h).is_some_and(|u| self.unit(target).is_some_and(|t| t.owner == u.owner)) && self.ability_with_code(h, codes::ability::REPAIR_ORC).is_some() {
            // A peasant helps build a peasant's structure.
            Action::Construct { target }
        } else if (order == crate::natives::basic::order_id("repair") || order == smart) && target != 0 && self.can_repair(h, target) {
            Action::Repair { target }
        } else if order == smart && target != 0 && self.unit(target).is_some_and(|t| t.extra.waygate.is_some_and(|g| g.2)) {
            // Into an active waygate.
            let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
            Action::Move { x: tx, y: ty }
        } else if (order == smart || order == crate::natives::basic::order_id("board")) && target != 0 && self.cargo_hold(target).is_some() && self.can_board(target, h) {
            Action::Board { transport: target }
        } else if (order == smart || order == crate::natives::basic::order_id("harvest")) && target != 0 && self.can_harvest(h, target) {
            if self.mine_held_by_other(h, target) {
                return;
            }
            Action::Harvest { target }
        } else if order == crate::natives::basic::order_id("returnresources") {
            Action::ReturnResources
        } else if order == crate::natives::basic::order_id("autoharvestgold") || order == crate::natives::basic::order_id("autoharvestlumber") {
            // The nearest gold mine, or tree, within 2000 of the worker (CAbilityHarvest's
            // FUN_6f426830 and FUN_6f426870, DAT_6fd6b7dc 2000; measured: HumanX02's
            // workers so ordered mine, its gold 690 where ours stayed at 500).
            let gold = order == crate::natives::basic::order_id("autoharvestgold");
            const RANGE: f32 = 2000.0;
            let found = if gold {
                let Some(u) = self.unit(h) else { return };
                let (ux, uy) = (u.x, u.y);
                // Any gold the worker can take: a mine, or the haunted or entangled
                // mine its kind works (measured: NightElfX02's wisps so ordered go to
                // their entangled mine).
                self.unit_order
                    .iter()
                    .copied()
                    // Only a mine its owner sees (FUN_6f439910 asks the fog; measured: a
                    // peasant so ordered takes a visible mine, and none fogged or masked;
                    // Human02's skipped intro orders its gold peasants so while the mine
                    // lies masked, and they stand idle in the original).
                    .filter(|&m| m != h && self.can_harvest(h, m) && !self.mine_held_by_other(h, m) && self.unit(m).is_some_and(|t| t.is_building && self.fog_state(t.x, t.y, u.owner) == crate::fog::FOG_VISIBLE))
                    .filter_map(|m| self.unit(m).map(|t| ((t.x - ux).hypot(t.y - uy), m)))
                    .filter(|(d, _)| *d <= RANGE)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|x| x.1)
            } else if [codes::ability::WISP_HARVEST, codes::ability::HARVEST, codes::ability::HARVEST_LUMBER].iter().any(|c| self.ability_with_code(h, c).is_some()) {
                // Ghouls too (measured: UndeadX03's ghouls so ordered gather lumber).
                // (The cheapest by the original's path query: `World::auto_tree`.)
                self.auto_tree(h).filter(|&t| self.can_harvest(h, t))
            } else {
                None
            };
            match found {
                Some(target) => Action::Harvest { target },
                None => return,
            }
        } else if (order == smart || order == crate::natives::basic::order_id("getitem")) && self.item(target).is_some_and(|i| i.owner_unit == 0) && self.inventory_size(h) > 0 {
            Action::PickUp { item: target }
        } else if (order == smart || order == mv) && target != 0 {
            match owner_of(self, target) {
                Some(o) if !is_ally(self, me, o) && order == smart => Action::Attack { target },
                Some(_) => Action::Follow { target },
                None => {
                    let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
                    Action::Move { x: tx, y: ty }
                }
            }
        } else if order == smart || order == mv {
            Action::Move { x, y }
        } else if let Some(action) = self.militia_order(h, order) {
            match action {
                Some(a) => a,
                None => return,
            }
        } else if let Some(a) = self.ability_for_order(h, order) {
            if self.targeting(a) == crate::ability::Targeting::Passive {
                return;
            }
            // Targets the ability does not allow are refused with their error.
            if target != 0 {
                if let Some(e) = self.cast_target_error(h, a, target) {
                    self.order_error = Some(e);
                    return;
                }
            }
            // Raise Dead wants its corpse; Cannibalize, without one, a corpse within its
            // reach (measured: the original refuses an immediate raisedead, and an
            // immediate cannibalize once the corpse beside the ghoul is eaten).
            let code = self.ability_code(a);
            if target == 0 && (x, y) == (0.0, 0.0) {
                if code == codes::ability::RAISE_DEAD {
                    self.order_error = Some("Targetcorpse");
                    return;
                }
                if code == codes::ability::CANNIBALIZE {
                    let (ux, uy, r) = self.unit(h).map_or((0.0, 0.0, 0.0), |u| (u.x, u.y, u.collision));
                    let lvl = self.ability_level(h, a);
                    if self.corpses_in(ux, uy, self.data(a, "Rng", lvl) + r + 32.0).is_empty() {
                        self.order_error = Some("Cantfindcorpse");
                        return;
                    }
                }
            }
            Action::Cast { ability: a, target, x, y }
        } else {
            return;
        };
        // The same cast again, at the same target, goes on as it was (measured: a
        // Paladin ordered to Holy Light the same footman every tenth of a second
        // casts at 0.60 seconds, then each 5.6, in the original), but not at a corpse:
        // that starts over (measured: a Necromancer, cast point 0.5, ordered to Raise
        // Dead at a footman's corpse every 0.5 seconds never raises it).
        let at_corpse = target != 0 && self.unit(target).is_some_and(|t| t.dead);
        if !at_corpse && self.unit(h).is_some_and(|u| u.action == action && matches!(action, Action::Cast { .. })) {
            return;
        }
        // Sent to the nearest tree, it is under a harvest order at that tree (measured:
        // NightElfX04's wisps so ordered read "harvest" in the original).
        let auto = order == crate::natives::basic::order_id("autoharvestlumber") || order == crate::natives::basic::order_id("autoharvestgold");
        let (order, target) = match action {
            Action::Harvest { target: t } if auto => (crate::natives::basic::order_id("harvest"), t),
            _ => (order, target),
        };
        self.leave_mine(h);
        // A unit takes up an attack order after a random delay of up to ReactionDelay
        // ("max random reaction delay"; measured: footmen ordered to attack strike 0.07
        // to 0.28 seconds later than their swing alone allows, one by one, where ones
        // that take up the same target on their own strike on time).
        let fresh_attack = matches!(action, Action::Attack { .. }) && self.unit(h).is_some_and(|u| !(u.order.id == order && u.order.target == target));
        let reaction = if fresh_attack { self.rng.real(0.0, self.misc_f32("ReactionDelay", 0.25)) } else { 0.0 };
        let harvest = match action {
            Action::Harvest { target } => Some(target),
            _ => None,
        };
        if let Some(u) = self.unit_mut(h) {
            u.action = action;
            u.extra.reaction = reaction;
            // A new order cuts off an attack or a cast's swing (and a channel, below),
            // unless the unit was already under the same attack order (measured: a Mortar
            // Team re-ordered to attack its target mid-swing fires on time, but one
            // swinging on its own, idle or on hold, starts over when so ordered).
            u.casting = None;
            let same_order = u.order.id == order && u.order.target == target && target != 0;
            if !(same_order && matches!((&u.action, &u.swing), (Action::Attack { target: t }, Some(s)) if *t == s.target)) {
                // A swing cut off before its damage point gives back its cooldown
                // (FUN_6f49d5b0 cancels both timers).
                if u.swing.as_ref().is_some_and(|s| !s.done) {
                    u.cooldown = u.cooldown.min(0.0);
                }
                u.swing = None;
            }
            u.path.clear();
            u.repath_timer = 0.0;
            u.order = Order { id: order, target_x: x, target_y: y, target };
        }
        // A gold mine's harvest runs by the original's engine (`crate::goldrun`).
        match harvest {
            Some(target) if self.gold_run_eligible(h, target) => {
                let player = self.ordering_player;
                self.run_auto = auto;
                self.start_gold_run(h, target, player);
                self.run_auto = false;
            }
            _ => {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.gold_run = None;
                }
            }
        }
        // So does a player's order to walk to a point.
        let walks = self.unit(h).is_some_and(|u| matches!(u.action, Action::Move { .. }) && !u.extra.local_paths);
        if walks && self.ordering_player && self.pathing.is_some() && self.move_kind(h) == crate::pathing::Move::Foot {
            if self.group_walk_members.is_some() {
                self.start_group_member(h);
            } else {
                self.start_player_walk(h);
            }
        } else if let Some(u) = self.unit_mut(h) {
            u.extra.player_walk = false;
        }
    }

    /// A player's order to walk to a point (traced: a peon sent across the ground): its
    /// objects (`Tags::player_move_issued`), and the engine's walk from the next step,
    /// its way searched 700 nodes deep.
    pub(crate) fn start_player_walk(&mut self, h: u32) {
        let serial = self.tags.born;
        let replaced = self.unit(h).filter(|u| u.extra.player_walk).map(|u| u.extra.order_tags.clone());
        let after = self.after_steps.then(|| self.unit(h).map_or(0, |u| u.extra.walk_serial));
        let slots = self.tags.player_move_issued(replaced.as_deref(), after);
        // A walk ordered to the accelerator cell its own path already goes for keeps that
        // path's ways: the steering resets them only for a new goal's cell
        // (`FUN_6f16fbd0`, `FUN_6f167e40`: the path's goal, +0x1c and +0x20, against the
        // cluster's point, each floored and halved), so it searches no more (read:
        // Mafarazzo TD's creep on WalkMid3's edge, sent anew to WalkMid34's middle as it
        // enters it again every few steps, walking on; the port's searching again, ten
        // steps apart, stood seven steps and gave one order fewer by turn 358).
        let origin = self.pathing.as_ref().map(|p| p.origin);
        let same = replaced.is_some() && self.unit(h).zip(origin).is_some_and(|(u, o)| {
            let Action::Move { x, y } = u.action else { return false };
            let g = (((x - o[0]) / 64.0).floor(), ((y - o[1]) / 64.0).floor());
            u.extra.coarse.as_ref().is_some_and(|c| c.index == 0) && u.extra.move_coarse.as_ref().is_some_and(|m| (m.points[0].0.floor(), m.points[0].1.floor()) == g)
        });
        if let Some(u) = self.unit_mut(h) {
            u.extra.order_tags = slots;
            u.extra.walk_serial = serial;
            u.extra.player_walk = true;
            u.extra.mover = None;
            u.path.clear();
            u.extra.kept_way = same;
            // Its cluster is new, its own path's ways dropped (`FUN_6f168740`) but for
            // that.
            u.extra.coarse = None;
            if !same {
                u.extra.move_coarse = None;
            }
            u.extra.plan_at[0] = None;
            u.extra.coarse_done = false;
        }
    }

    /// Where a script finds a unit (GetUnitX, GetUnitY, GetUnitLoc): a walker's place at
    /// its last step carried on by its velocity to the main clock now (`FUN_6f161040`
    /// on its position, `motion::Mover::at`; read: Mafarazzo TD's creeps pinged by a
    /// timer at 21 seconds, each where its velocity would have taken it by 20.9975, the
    /// main clock then, a third to a whole step past its last step's place).
    pub fn place_now(&self, h: u32) -> Option<(f32, f32)> {
        use crate::efloat as ef;
        let u = self.unit(h)?;
        let (Some(m), Some(clock), Some(p)) = (u.extra.mover, self.script_clock, self.pathing.as_ref()) else { return Some((u.x, u.y)) };
        if m.vx & 0x7fff_ffff == 0 && m.vy & 0x7fff_ffff == 0 || !u.steps_walker() {
            return Some((u.x, u.y));
        }
        let (x, y) = m.at(clock);
        let world = |c: u32, o: f32| f32::from_bits(ef::add(ef::mul(c, crate::pathing::CELL.to_bits()), o.to_bits()));
        Some((world(x, p.origin[0]), world(y, p.origin[1])))
    }

    pub fn widget_pos(&self, h: u32) -> Option<(f32, f32)> {
        match self.handles.get(h) {
            Some(Obj::Unit(u)) => Some((u.x, u.y)),
            Some(Obj::Destructable(d)) => Some((d.x, d.y)),
            Some(Obj::Item(i)) => Some((i.x, i.y)),
            _ => None,
        }
    }

    /// Ghost and Ghost (Visible) (CAbilityGhost, CAbilityGhostVisible: Agho, Aeth) take
    /// a unit's collision with other units away (FUN_6f623bd0).
    pub fn is_ghost(&self, u: &Unit) -> bool {
        u.abilities.iter().any(|&(a, _)| matches!(self.ability_code(a).as_str(), codes::ability::GHOST | codes::ability::GHOST_VISIBLE))
    }

    /// How a unit moves, from its type's movetp (an uprooted ancient walks).
    pub fn move_kind(&self, h: u32) -> crate::pathing::Move {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(crate::pathing::Move::Foot, |o| crate::pathing::Move::from_movetp(&o.str("movetp")))
    }

    pub(crate) fn path_to(&mut self, h: u32, goal: (f32, f32)) {
        let t0 = self.profile.is_some().then(std::time::Instant::now);
        let capped_before = self.pathing.as_ref().map_or(0, |p| p.search_stats().2);
        self.path_to_inner(h, goal);
        self.prof_add("path_to", t0);
        // A search that gave up before reaching its goal is not tried again for three
        // seconds: the unit follows the way it found meanwhile.
        let capped = self.pathing.as_ref().map_or(0, |p| p.search_stats().2) > capped_before;
        if capped {
            if std::env::var("W3CAPLOG").is_ok() {
                if let Some(u) = self.unit(h) {
                    let t = match u.action { Action::Attack { target } => target, _ => 0 };
                    let tu = self.unit(t);
                    eprintln!("CAP {} {} at ({:.0},{:.0}) -> {} at ({:.0},{:.0}) d {:.0} flying {} path {} t={:.1}", u.owner, crate::objects::id_str(u.type_id), u.x, u.y, tu.map_or("?".into(), |t| crate::objects::id_str(t.type_id)), goal.0, goal.1, (goal.0 - u.x).hypot(goal.1 - u.y), tu.is_some_and(|t| t.fly_height > 0.0), u.path.len(), self.time);
                }
            }
            if let Some(u) = self.unit_mut(h) {
                u.repath_timer = 3.0;
            }
        }
        if self.profile.is_some() && capped {
            let kind: &'static str = match self.unit(h).map(|u| &u.action) {
                Some(Action::Attack { .. }) => "capped:attack",
                Some(Action::AttackMove { .. }) => "capped:attackmove",
                Some(Action::Move { .. }) => "capped:move",
                Some(Action::Follow { .. }) => "capped:follow",
                Some(Action::Harvest { .. }) => "capped:harvest",
                Some(Action::Build { .. }) => "capped:build",
                Some(Action::Cast { .. }) => "capped:cast",
                _ => "capped:other",
            };
            if let Some(p) = self.profile.as_mut() {
                *p.entry(kind).or_insert(0.0) += 0.001;
            }
        }
    }

    /// Adds the time since `t0` to a W3PROF entry.
    pub(crate) fn prof_add(&mut self, name: &'static str, t0: Option<std::time::Instant>) {
        if let (Some(t0), Some(p)) = (t0, self.profile.as_mut()) {
            *p.entry(name).or_insert(0.0) += t0.elapsed().as_secs_f64();
        }
    }

    /// A wander ability given to a unit sets how it searches: Neutral Passive's search
    /// near at hand only, 32 nodes deep, others as before (CAbilityWander's
    /// `FUN_6f5a6d40`; measured: Booty Bay's sheep and pigs search 32 deep even with
    /// the ability taken away again, player 0's sheep and Neutral Passive's footmen
    /// 700).
    pub(crate) fn wander_paths(&mut self, h: u32) {
        if self.ability_with_code(h, codes::ability::WANDER).is_none() {
            return;
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.local_paths = u.owner == crate::types::PLAYER_NEUTRAL_PASSIVE;
        }
    }

    /// The way of a unit searching near at hand only (`UnitExtra::local_paths`): the
    /// original's long-range search (`lrpath`) straight toward the goal, 32 nodes deep,
    /// other ground units' regions refusing their cells; the cells' middles to the goal,
    /// or to the node reached nearest it (searched again from there on reaching its
    /// cell), or none.
    /// The cells other ground units' collision regions cover, as a search or a line
    /// test sees them: on each axis the cells from floor(p - 0.5 - r) to
    /// floor(p - 0.5 + r), p the middle of the cell the unit stands in and r its
    /// collision in cells (measured with Booty Bay's sheep, and its two forest trolls
    /// by the original's cells: a troll at 378.28, 122.25 covering 377 to 379 and 121
    /// to 123, one at 383.77, 121.60 covering 382 to 384 and 120 to 122).  A moving unit's region is excused (its flags hold
    /// 0x20000000, which the test does not count: read in the original, a pig's search
    /// past two moving pigs and one standing; a sheep whose walk had begun but who had
    /// not yet moved, turning, still blocked).
    fn regions_taken(&self, h: u32) -> std::collections::HashSet<(i32, i32)> {
        let mut taken = std::collections::HashSet::new();
        let (Some(u), Some(p)) = (self.unit(h), self.pathing.as_ref()) else { return taken };
        let mask = u.region_mask();
        for &o in &self.unit_order {
            let Some(v) = self.unit(o).filter(|v| o != h && !v.dead && !v.hidden && !v.is_building && v.collision > 0.0 && (v.x - u.x).abs() < 2048.0 && (v.y - u.y).abs() < 2048.0) else { continue };
            if v.engine_walker() && v.extra.mover.is_some_and(|m| m.vx & 0x7fff_ffff != 0 || m.vy & 0x7fff_ffff != 0) || v.region_flags() & mask == 0 {
                continue;
            }
            if self.move_kind(o) == crate::pathing::Move::Fly {
                continue;
            }
            let (px, py) = (((v.x - p.origin[0]) / crate::pathing::CELL).floor() + 0.5, ((v.y - p.origin[1]) / crate::pathing::CELL).floor() + 0.5);
            let r = v.collision / crate::pathing::CELL;
            for cy in (py - 0.5 - r).floor() as i32..=(py - 0.5 + r).floor() as i32 {
                for cx in (px - 0.5 - r).floor() as i32..=(px - 0.5 + r).floor() as i32 {
                    taken.insert((cx, cy));
                }
            }
        }
        // The computers' captains stand as pathing objects too (`Captain::place`), their
        // regions held in the map's cells whose middles they cover (read: the defense
        // captain at (352.5, 503.5), radius half a cell, in (352, 503)'s list from 0.0,
        // not in (353, 504)'s at 0.12, refusing (352, 503) to the ghoul's search at
        // 1.02; `FUN_6f9d3047` placing it as CreateCaptains runs).
        if mask & 0x02 != 0 {
            taken.extend(self.captain_cells((u.x, u.y)));
        }
        taken
    }

    /// The cells the computers' captains' regions are held in near a place (within 2048
    /// on each axis): those whose middles a captain covers (`World::regions_taken`).
    fn captain_cells(&self, (ux, uy): (f32, f32)) -> Vec<(i32, i32)> {
        let Some(p) = self.pathing.as_ref() else { return Vec::new() };
        let places: Vec<(f32, f32)> = self.ai.values().flat_map(|st| [st.attack.place, st.defense.place]).flatten().collect();
        let r = crate::ai::CAPTAIN_COLLISION / crate::pathing::CELL * 0.5;
        let mut cells = Vec::new();
        for (x, y) in places {
            if (x - ux).abs() >= 2048.0 || (y - uy).abs() >= 2048.0 {
                continue;
            }
            let (cx, cy) = ((x - p.origin[0]) / crate::pathing::CELL, (y - p.origin[1]) / crate::pathing::CELL);
            for ty in (cy - r - 0.5).floor() as i32..=(cy + r - 0.5).ceil() as i32 {
                for tx in (cx - r - 0.5).floor() as i32..=(cx + r - 0.5).ceil() as i32 {
                    if (tx as f32 + 0.5 - cx).hypot(ty as f32 + 0.5 - cy) <= r {
                        cells.push((tx, ty));
                    }
                }
            }
        }
        cells
    }

    /// The waypoints left once a walker at `from` (in cells) picks its target
    /// (`FUN_6f167bf0`): the next waypoint, then each one after it while a straight line
    /// from it to that one is clear (`line_clear`), stopping at the first that is not.
    fn way_ahead(&self, h: u32, from: (u32, u32), way: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
        let (Some(u), Some(p)) = (self.unit(h), self.pathing.as_ref()) else { return way };
        if way.len() < 2 {
            return way;
        }
        let class = crate::lrpath::class_of(u.collision / crate::pathing::CELL);
        let kind = self.move_kind(h);
        // A harvest walking past units sees through those its mask does not meet (read: a
        // peon home from the mine heading for its way's far end past an idle peon;
        // `Unit::region_blocks`).
        let taken = self.regions_taken(h);
        let open = |c: (i32, i32)| p.passable(c, kind) && !taken.contains(&c);
        let cell = |w: f32, o: f32| ((w - o) / crate::pathing::CELL).to_bits();
        let mut k = 0;
        for (j, w) in way.iter().enumerate().skip(1) {
            if !line_clear(from, (cell(w.0, p.origin[0]), cell(w.1, p.origin[1])), class, &open) {
                break;
            }
            k = j;
        }
        way[k..].to_vec()
    }

    /// A walker stopped by a new order between steps: its place moved on by its
    /// velocity for the time since its last step, to the main clock `clock`, as a
    /// change of velocity does (`FUN_6f15f7e0`; read: Forestwalk's undead computer's
    /// ghoul stopped by its wood order at 1.10, two thirds of a step past 1.08).
    pub(crate) fn stop_walker_at(&mut self, h: u32, clock: u32) {
        use crate::efloat as ef;
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return };
        let Some(m) = self.unit(h).and_then(|u| u.extra.mover) else { return };
        if m.vx & 0x7fff_ffff == 0 && m.vy & 0x7fff_ffff == 0 {
            return;
        }
        let (x, y) = m.at(clock);
        let Some(u) = self.unit_mut(h) else { return };
        u.x = f32::from_bits(ef::add(ef::mul(x, crate::pathing::CELL.to_bits()), origin[0].to_bits()));
        u.y = f32::from_bits(ef::add(ef::mul(y, crate::pathing::CELL.to_bits()), origin[1].to_bits()));
        u.extra.cell_at = Some((x, y, u.x, u.y));
        // Its next walk turns from the facing it had, not an older walk's end (read:
        // the human computer's barracks' peasant, stopped walking home at 18.1, three
        // steps turning from its heading before it moved on; the port's, from the
        // heading it last arrived with, two).
        u.extra.walk_heading = None;
        if let Some(m) = u.extra.mover.as_mut() {
            (m.x, m.y, m.vx, m.vy, m.placed_at) = (x, y, 0, 0, clock);
        }
        // Its velocity so set to none, its region is no longer a mover's to others'
        // searches (`FUN_6f1606e0` clears its +0x40's 0x20000000; read: Twisted Meadows'
        // night elf computer's wisp stopped by its AI's order at 0.1 turning another's
        // search at 0.12 aside, round the north of the mine).
    }

    pub(crate) fn local_path(&self, h: u32, goal: (f32, f32)) -> Option<(Vec<(f32, f32)>, Option<(i32, i32)>, u32)> {
        let u = self.unit(h).filter(|u| u.engine_walker())?;
        let to_structure = u.extra.gold_run.is_some() || u.ai_harvest_walk();
        let p = self.pathing.as_ref()?;
        let kind = self.move_kind(h);
        if kind == crate::pathing::Move::Fly {
            return None;
        }
        // Once a worker has been in the mine its harvest's walks search past other units
        // (its orders' task 0xd0150 sets the search's mask, the path's +0x9c, to
        // 0x10000010 from 0x02000002, and units' regions, +0x34 0xca, do not meet it;
        // read: a peon's way home along row 265 past an idle peon whose region covers
        // 672, 264, where its first walk's search was refused by them), but for workers
        // stopped at a mine (`Unit::region_blocks`).
        let taken = self.regions_taken(h);
        struct Local<'a> {
            p: &'a crate::pathing::PathingMap,
            kind: crate::pathing::Move,
            taken: std::collections::HashSet<(i32, i32)>,
        }
        impl crate::lrpath::Cells for Local<'_> {
            fn open(&self, x: i32, y: i32) -> bool {
                self.p.passable((x, y), self.kind) && !self.taken.contains(&(x, y))
            }
        }
        let cells = Local { p, kind, taken };
        let class = crate::lrpath::class_of(u.collision / crate::pathing::CELL);
        let (start, end) = (p.cell_of(u.x, u.y), p.cell_of(goal.0, goal.1));
        let (out, refused, cost) = match self.gold_run_search(h, &cells, start, end, class) {
            Some(r) => r,
            None => crate::lrpath::search_costing(&cells, start, end, class, if u.extra.local_paths || u.extra.captain_walk.is_some() { 32 } else { 700 }),
        };
        log::debug!("search for {h} from {start:?} to {end:?}: {:?} refused {refused} cost {cost}", match &out { crate::lrpath::Outcome::Found(c) => format!("found {}", c.len()), crate::lrpath::Outcome::Nearest(c) => format!("nearest {:?}", c.last()), crate::lrpath::Outcome::Stay => "stay".into() });
        log::trace!("search for {h}: cells {:?}", match &out { crate::lrpath::Outcome::Found(c) | crate::lrpath::Outcome::Nearest(c) => c.clone(), crate::lrpath::Outcome::Stay => Vec::new() });
        // A search that met no refused cell sends the walker straight to its way's end
        // (`FUN_6f166e90`: the way's index set to its end, else to its first node).
        let straight = |way: Vec<(f32, f32)>| if refused { way } else { way.last().map(|&w| vec![w]).unwrap_or_default() };
        Some(match out {
            // A way to a structure ends at its last node's middle.
            crate::lrpath::Outcome::Found(c) if to_structure => (straight(c[1..].iter().map(|&c| p.center(c)).collect()), None, cost),
            crate::lrpath::Outcome::Found(c) => {
                let mut way: Vec<(f32, f32)> = c[1..].iter().map(|&c| p.center(c)).collect();
                way.pop();
                way.push(goal);
                (straight(way), None, cost)
            }
            crate::lrpath::Outcome::Nearest(c) => (straight(c[1..].iter().map(|&c| p.center(c)).collect()), c.last().copied(), cost),
            // Nowhere nearer: it turns toward the goal where it stands (`Mover::face`).
            crate::lrpath::Outcome::Stay => (vec![goal], Some((i32::MIN, i32::MIN)), cost),
        })
    }

    /// An AI's worker walking to its mine steers by its path's coarse way
    /// (`crate::accelpath`): planned over the accelerator pyramid as its walk begins
    /// (5000 pops, an order path's), its target the first point some 10 accelerator
    /// units on, or the mine itself when none is (read: Forestwalk's undead computer's
    /// fourth acolyte, made at (346.5, 483.5) and sent to its mine at (350, 516),
    /// heading straight for (349.5, 505.5) at 17.128).  A unit sent alone to a point
    /// steers so too (`World::cluster_goal`).  None while a plan is refused.
    ///
    /// A flyer's plans over the pyramid's last layer, its cells' no-fly flag (0x04,
    /// `accel::LAYERS`), and it heads straight for each point, no local search between
    /// (read: Mafarazzo TD's flying peasant sent to build at (-3392, 1536) from (0,
    /// -3264), heading at 91.2 degrees for (191.5, 113.5), the way's ninth point from
    /// the goal, where the straight line was at 125.2; each of its 219 pings, a tenth of
    /// a second apart, the original's to 0.001).
    fn coarse_goal(&mut self, h: u32, goal: (f32, f32)) -> Option<(f32, f32)> {
        if self.cluster_walker(h) {
            return self.cluster_goal(h, goal);
        }
        let wood = self.unit(h).is_some_and(|u| u.ai_wood_walk());
        if !wood && !self.unit(h).is_some_and(|u| u.ai_harvest_walk() || u.ai_build_walk()) {
            return Some(goal);
        }
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return Some(goal) };
        let acc = |w: f32, o: f32| (w - o) / 64.0;
        if self.unit(h).is_some_and(|u| u.extra.coarse.is_none()) {
            let Some(u) = self.unit(h) else { return Some(goal) };
            let pos = (acc(u.x, origin[0]), acc(u.y, origin[1]));
            let target = match self.unit(h).map(|u| &u.action) {
                Some(Action::Harvest { target }) => *target,
                _ => 0,
            };
            let Some(mut pyr) = self.accel_pyramid_with(Some(target)) else { return Some(goal) };
            // A walk to a tree plans with the tree's region lifted (below), and picks its
            // point by the way's legs (`accelpath::pick`, 10 on, as a cluster's).
            if wood {
                self.lift_region(&mut pyr, target);
            }
            let layer = if self.move_kind(h) == crate::pathing::Move::Fly { 3 } else { 0 };
            let way = crate::accelpath::plan(&pyr, layer, pos, (acc(goal.0, origin[0]), acc(goal.1, origin[1])), 5000);
            let index = if wood { crate::accelpath::pick(&way.points, way.points.len() - 1, 10.0) } else { crate::accelpath::next_target(&way.points, way.points.len() - 1, pos, 10.0) };
            let now = self.step_index + self.step_range.0 as u64;
            log::debug!("coarse way of {h}: {:?}, target {index}", way.points);
            if let Some(u) = self.unit_mut(h) {
                u.extra.coarse = Some(crate::accelpath::Coarse { points: way.points, index, searched_at: now });
            }
        }
        Some(match self.unit(h).and_then(|u| u.extra.coarse.as_ref()) {
            Some(c) if c.index > 0 => {
                let p = c.points[c.index];
                (p.0 * 64.0 + origin[0], p.1 * 64.0 + origin[1])
            }
            _ => goal,
        })
    }

    /// A destructable's pathing region lifted out of a pyramid for a plan to it, as a
    /// coarse plan lifts its path's target's region (its +0xa4, the cluster's target's,
    /// `FUN_6f16afc0`, set by `FUN_6f16ce10`'s `FUN_6f168bd0`) and its walker's own
    /// (+0xa0) for its search (`FUN_6f166c30`'s `FUN_6f15d360`, `accel::Pyramid::lift`):
    /// the region's cells, x0 to x1 and y0 to y1 past its last, halved and floored.
    /// Read: Twisted Meadows' human computer's wood workers, its first at 0.12 from
    /// (165.04, 190.15) heading straight for its tree at (186, 174), its point the tree
    /// (the tree's cells left in, the way ends short of it, its point 10 on (185.5,
    /// 179.5)); its farm peasant at 60.1, from (171.04, 187.47) to its tree at (190,
    /// 174), for (189.5, 179.5).  (Provisional: the region the tree's cells' bounds.)
    fn lift_region(&self, pyr: &mut crate::accel::Pyramid, d: u32) {
        let Some(crate::types::Obj::Destructable(d)) = self.handles.get(d) else { return };
        let Some(x0) = d.pathing_cells.iter().map(|c| c.0).min() else { return };
        let x1 = d.pathing_cells.iter().map(|c| c.0).max().unwrap_or(x0) + 1;
        let y0 = d.pathing_cells.iter().map(|c| c.1).min().unwrap_or(0);
        let y1 = d.pathing_cells.iter().map(|c| c.1).max().unwrap_or(y0) + 1;
        pyr.lift(x0.div_euclid(2) as i64, y0.div_euclid(2) as i64, x1.div_euclid(2) as i64, y1.div_euclid(2) as i64);
    }

    /// Whether it walks as a cluster of one (a move order's `^clu`, its path `l^pa`):
    /// a unit sent alone to a point by a player's order or a script's.  Its path plans
    /// over the accelerator (its +0x88 0x200000, set for every path,
    /// `NIpse::CLrPath::vf3_6f166060`, but Neutral Passive's wanderers',
    /// `FUN_6f5a6d40`).  So does a computer's unit attack-moving back to its guard post
    /// (`FUN_6f9c0b00`, its order's `^clg`, `^clu`, and `l^pa` a move's; read: Mafarazzo
    /// TD's fort runner, its walk back bending east of the straight line to its post).
    pub(crate) fn cluster_walker(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.extra.player_walk && u.extra.group_walk.is_none() && (matches!(u.action, Action::Move { .. }) || matches!(u.action, Action::AttackMove { .. }) && u.extra.ai_idle_ord.is_some()))
    }

    /// A walker's place now in cells: its mover's at the step's clock, else where it
    /// stands.
    fn cell_place(&self, h: u32) -> Option<(u32, u32)> {
        use crate::efloat as ef;
        let origin = self.pathing.as_ref()?.origin;
        let u = self.unit(h)?;
        let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
        if let Some(m) = u.extra.mover {
            let clock = self.step_clocks.get(self.step_range.0).copied().unwrap_or(self.last_step_clock);
            return Some(m.at(clock));
        }
        Some(match u.extra.cell_at {
            Some((cx, cy, wx, wy)) if wx == u.x && wy == u.y => (cx, cy),
            _ => (cell(u.x, origin[0]), cell(u.y, origin[1])),
        })
    }

    /// The point a cluster walker heads for: its cluster's point on the order's coarse
    /// way, and on its own path's way to that, the point its local search goes for.
    ///
    /// The cluster's path (`FUN_6f169840`: its accelerator pops limited to 5000) plans
    /// as its walk begins (`FUN_6f16c150`, `FUN_6f16ce10`, `FUN_6f167120`,
    /// `FUN_6f166c30`), from the member's place to the order's point, and picks its
    /// point (`FUN_6f1697a0`, `FUN_6f165e30`: `accelpath::pick`, 10 on), the member's
    /// target (`FUN_6f167d70`: the point times 2, in cells).  The member's own path
    /// (its limit 400) then plans from its place to that target (`FUN_6f165ae0`,
    /// `FUN_6f165b60`) and picks its own point the same way, which its local search
    /// goes for (`FUN_6f165c60`, `FUN_6f167ce0`, `FUN_6f167d70`).  Each plan is spaced
    /// (`FUN_6f168910`, kind 1: ten steps after one of 32 pops or more) and budgeted
    /// (`crate::budget`: the cluster's by its 5000, the member's by its 400); refused,
    /// the walker holds and asks again the next step.  Read: Mafarazzo TD's creeps,
    /// sent each alone 2600 to 3600 down their lanes, walking straight for the cell
    /// middles 10 to 16 accelerator units on (the first made at (-3872, 2528) bending
    /// for (-3856, 1520)), their starts staggered by their local searches' budget.
    fn cluster_goal(&mut self, h: u32, goal: (f32, f32)) -> Option<(f32, f32)> {
        use crate::budget::Kind;
        use crate::efloat as ef;
        let origin = self.pathing.as_ref()?.origin;
        let owner = self.unit(h)?.owner;
        let now = self.step_index + self.step_range.0 as u64;
        let at = self.cell_place(h)?;
        let half = |c: u32| f32::from_bits(ef::mul(c, 0.5f32.to_bits()));
        let pos = (half(at.0), half(at.1));
        let to_acc = |w: f32, o: f32| half(ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits()));
        let world = |a: f32, o: f32| f32::from_bits(ef::add(ef::mul(ef::mul(a.to_bits(), 2.0f32.to_bits()), crate::pathing::CELL.to_bits()), o.to_bits()));
        if self.unit(h).is_some_and(|u| u.extra.coarse.is_none()) {
            let pyr = self.accel_pyramid_with(Some(0))?;
            let way = self.budgeted_plan(h, owner, Kind::Order, now, &pyr, pos, (to_acc(goal.0, origin[0]), to_acc(goal.1, origin[1])), 5000)?;
            let index = crate::accelpath::pick(&way.points, way.points.len() - 1, 10.0);
            log::debug!("cluster way of {h}: {:?}, pops {}, target {index}", way.points, way.pops);
            let u = self.unit_mut(h)?;
            u.extra.coarse = Some(crate::accelpath::Coarse { points: way.points, index, searched_at: now });
            u.extra.move_coarse = None;
        }
        let c = self.unit(h)?.extra.coarse.as_ref()?;
        let (target, target_acc) = if c.index > 0 {
            let p = c.points[c.index];
            ((world(p.0, origin[0]), world(p.1, origin[1])), p)
        } else {
            (goal, (to_acc(goal.0, origin[0]), to_acc(goal.1, origin[1])))
        };
        if self.unit(h).is_some_and(|u| u.extra.move_coarse.is_none()) {
            let pyr = self.accel_pyramid_with(Some(0))?;
            let way = self.budgeted_plan(h, owner, Kind::Coarse, now, &pyr, pos, target_acc, 400)?;
            let index = crate::accelpath::pick(&way.points, way.points.len() - 1, 10.0);
            log::debug!("own way of {h}: {:?}, pops {}, target {index}", way.points, way.pops);
            self.unit_mut(h)?.extra.move_coarse = Some(crate::accelpath::Coarse { points: way.points, index, searched_at: now });
        }
        let m = self.unit(h)?.extra.move_coarse.as_ref()?;
        Some(if m.index > 0 {
            let p = m.points[m.index];
            (world(p.0, origin[0]), world(p.1, origin[1]))
        } else {
            target
        })
    }

    /// A coarse plan as `FUN_6f166c30` makes it: spaced (`FUN_6f168910`, kind 1, the
    /// path's +0x80: refused within ten steps of one that took 32 pops or more), then
    /// budgeted (`crate::budget`, its pops added after); refused, none.
    #[allow(clippy::too_many_arguments)]
    fn budgeted_plan(&mut self, h: u32, owner: u8, kind: crate::budget::Kind, now: u64, pyr: &crate::accel::Pyramid, from: (f32, f32), to: (f32, f32), limit: u32) -> Option<crate::accelpath::Way> {
        let slot = if kind == crate::budget::Kind::Order { 0 } else { 1 };
        let last = self.unit(h)?.extra.plan_at[slot];
        if last.is_some_and(|t| now.wrapping_sub(t) < 10) {
            return None;
        }
        let mut b = std::mem::take(&mut self.budgets);
        let ok = b.allow(owner, kind, now, h, |u| self.unit(u).is_some_and(|v| !v.dead && v.steps_walker()));
        self.budgets = b;
        if !ok {
            log::trace!("plan of {h} ({kind:?}) refused at step {now}");
            self.unit_mut(h)?.extra.plan_at[slot] = None;
            return None;
        }
        let way = crate::accelpath::plan(pyr, 0, from, to, limit);
        let spent = self.budgets.spend(owner, kind, now, way.pops);
        log::trace!("plan of {h} ({kind:?}) at step {now}: {} pops, {spent} spent", way.pops);
        self.unit_mut(h)?.extra.plan_at[slot] = (way.pops >= 32).then_some(now);
        Some(way)
    }

    /// The point a walker's arrival is judged at: a cluster walker's cluster's point,
    /// else its order's goal.
    pub(crate) fn walk_target(&self, h: u32) -> Option<(f32, f32)> {
        use crate::efloat as ef;
        let goal = self.walk_goal(h)?;
        if !self.cluster_walker(h) {
            return Some(goal);
        }
        let origin = self.pathing.as_ref()?.origin;
        let world = |a: f32, o: f32| f32::from_bits(ef::add(ef::mul(ef::mul(a.to_bits(), 2.0f32.to_bits()), crate::pathing::CELL.to_bits()), o.to_bits()));
        Some(match self.unit(h)?.extra.coarse.as_ref() {
            Some(c) if c.index > 0 => (world(c.points[c.index].0, origin[0]), world(c.points[c.index].1, origin[1])),
            _ => goal,
        })
    }

    /// A walker steering for its coarse way's point short of its order's target is
    /// there, and stops, once within its order's reach of it (the two units' sizes and
    /// the ability's range, at least 0.49: `FUN_6f05a5c0` setting the steering's +0xb0
    /// from the two sizes, the target's with the range; `FUN_6f16e910` testing
    /// it at its place in the step); its group then picks its way's next target
    /// (`FUN_6f16c4f0`, `FUN_6f1697a0`), planning it anew from there, and it walks on
    /// in the next step (read: Forestwalk's undead computer's fourth acolyte, its reach
    /// 8.3125 to its haunted mine, stopped at 19.198 8.1874 from (349.5, 505.5), and
    /// walking for the mine's edge from 19.228).
    pub(crate) fn coarse_step_arrive(&mut self, h: u32, k: usize) -> bool {
        let Some(c) = self.unit(h).and_then(|u| u.extra.coarse.as_ref()).filter(|c| c.index > 0) else { return false };
        let Some(m) = self.unit(h).and_then(|u| u.extra.mover) else { return false };
        let Some(&clock) = self.step_clocks.get(k) else { return false };
        let target = c.points[c.index];
        // (A build's site, a point: StructureFollowRange, as its arrival there; read:
        // the altar's acolyte stopping at (352.8942, 501.1347), 3.076 from its point,
        // not a step before, 3.276.)
        // (A walk to a tree, its harvest's: read, the human computer's farm peasant
        // stopping 4.01 cells from its point, Ahar's 116 and its 16 over 32 being 4.125.)
        let wood = self.unit(h).is_some_and(|u| u.ai_wood_walk());
        let (t, range) = match self.unit(h).map(|u| u.action.clone()) {
            Some(Action::Harvest { target: t }) if wood => (t, self.ability_with_code(h, codes::ability::HARVEST).or_else(|| self.ability_with_code(h, codes::ability::HARVEST_LUMBER)).map_or(116.0, |(a, l)| self.ability_real(a, "Rng", l))),
            Some(Action::Harvest { target: t }) => (t, self.ability_with_code(h, codes::ability::ACOLYTE_HARVEST).map_or(200.0, |(a, l)| self.ability_real(a, "Rng", l))),
            Some(Action::Build { .. }) => (0, self.misc_f32("StructureFollowRange", 100.0) - self.unit(h).map_or(0.0, |u| u.collision)),
            _ => return false,
        };
        let reach = ((self.unit(h).map_or(0.0, |u| u.collision) + self.unit(t).map_or(0.0, |v| v.collision) + range) / crate::pathing::CELL).max(0.49);
        let (x, y) = m.at(clock);
        let (dx, dy) = (f32::from_bits(x) - target.0 * 2.0, f32::from_bits(y) - target.1 * 2.0);
        if (dx * dx + dy * dy).sqrt() > reach {
            return false;
        }
        // Arrived only with its heading within 0.2 of the point's bearing; till then it
        // stops there and turns, a turn a step (`FUN_6f16e910`; read: the altar's
        // acolyte, in reach of its coarse point at 35.488712 14.2 degrees off it,
        // stopped then and walking on from 35.548679).
        self.stop_walker_at(h, clock);
        let Some(mut mv) = self.unit(h).and_then(|u| u.extra.mover) else { return false };
        let turn = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(0.5, |o| o.real("turnRate"));
        let gait = crate::motion::Gait { top: 0, grow: 0, turn: turn.to_bits(), window: 0, reach: 0.49f32.to_bits() };
        if mv.face(&gait, ((target.0 * 2.0).to_bits(), (target.1 * 2.0).to_bits()), clock) != crate::motion::Step::Reached {
            log::debug!("coarse target of {h} in reach at step {k}, turning: ({:.4}, {:.4})", f32::from_bits(x), f32::from_bits(y));
            if let Some(u) = self.unit_mut(h) {
                u.facing = f32::from_bits(mv.heading).to_degrees();
                u.extra.mover = Some(mv);
            }
            return true;
        }
        log::debug!("coarse target of {h} reached at step {k}: ({:.4}, {:.4}), reach {reach}", f32::from_bits(x), f32::from_bits(y));
        self.stop_walker_at(h, clock);
        // The cluster keeps its way, picking its next point on it as the next step
        // begins (`FUN_6f16c4f0`, `FUN_6f1697a0`: `accelpath::pick`, 10 on; read: the
        // flying peasant, stopped a step 3.1 cells short of its first point, heading next
        // for the way's eighth, (177.5, 129.5), not a new plan's (175.5, 129.5)).
        // Provisional: a ground walk's planned anew there, as the port had it.
        let keep = self.move_kind(h) == crate::pathing::Move::Fly;
        if let Some(u) = self.unit_mut(h) {
            if keep {
                if let Some(c) = u.extra.coarse.as_mut() {
                    c.index = crate::accelpath::pick(&c.points, c.index, 10.0);
                }
                u.extra.mover = None;
                u.path.clear();
                u.extra.path_end_cell = None;
                return true;
            }
            u.extra.coarse = None;
            u.extra.mover = None;
            u.path.clear();
            u.extra.path_end_cell = None;
        }
        true
    }

    fn path_to_inner(&mut self, h: u32, goal: (f32, f32)) {
        // A walker's local search waits ten steps after its last while its owner's
        // budget is spent past 64 passes (`FUN_6f166e90`, `FUN_6f168910`: read, sheep
        // 1050122's search at step 0x54c took the budget to 66, and its tries at 0x553
        // to 0x555 were refused, at 0x556 allowed), and while the budget is spent past
        // its cap or others wait for it (`crate::budget`); refused, its way stays
        // dropped and it tries again the next step.  So does one whose coarse plan is
        // refused.
        let refuse = |w: &mut Self| {
            if let Some(u) = w.unit_mut(h) {
                u.path.clear();
                u.extra.path_end_cell = None;
                u.extra.facing_goal = false;
                u.extra.walk_research = true;
            }
        };
        let Some(goal) = self.coarse_goal(h, goal) else {
            refuse(self);
            return;
        };
        let now = self.step_index + self.step_range.0 as u64;
        // (The kept way, the port searching it anew from the walker's place without its
        // budget or spacing; provisional: the original's own goes on as it was.)
        let kept = self.unit_mut(h).is_some_and(|u| std::mem::take(&mut u.extra.kept_way));
        if kept {
            if let Some((way, end, _)) = self.local_path(h, goal) {
                if let Some(u) = self.unit_mut(h) {
                    u.path = way;
                    u.extra.facing_goal = end == Some((i32::MIN, i32::MIN));
                    u.extra.path_end_cell = end.filter(|&e| e != (i32::MIN, i32::MIN));
                    u.repath_timer = f32::INFINITY;
                }
                return;
            }
        }
        let local = self.unit(h).is_some_and(|u| u.engine_walker()) && self.move_kind(h) != crate::pathing::Move::Fly;
        if local && self.unit(h).and_then(|u| u.extra.search_at).is_some_and(|t| now.wrapping_sub(t) < 10) {
            refuse(self);
            return;
        }
        let owner = self.unit(h).map_or(0, |u| u.owner);
        if local && self.pathing.is_some() {
            let mut b = std::mem::take(&mut self.budgets);
            let ok = b.allow(owner, crate::budget::Kind::Local, now, h, |u| self.unit(u).is_some_and(|v| !v.dead && v.steps_walker()));
            self.budgets = b;
            if !ok {
                refuse(self);
                return;
            }
        }
        if let Some((way, end, cost)) = self.local_path(h, goal) {
            let spent = self.budgets.spend(owner, crate::budget::Kind::Local, now, cost);
            log::trace!("local search of {h} at step {now}: {cost} pops, {spent} spent");
            let spaced = spent >= 64;
            if let Some(u) = self.unit_mut(h) {
                u.extra.search_at = spaced.then_some(now);
                u.path = way;
                u.extra.facing_goal = end == Some((i32::MIN, i32::MIN));
                u.extra.path_end_cell = end.filter(|&e| e != (i32::MIN, i32::MIN));
                u.repath_timer = f32::INFINITY;
            }
            return;
        }
        let Some(u) = self.unit(h) else { return };
        // Paths keep a unit's center a whole cell clear of what blocks it, at least one
        // (measured: a peasant, a footman, and a knight, collision 16, 31, and 32, all
        // round the end of a wall of farms 48 out, through the next cell's center).
        // Where that cannot reach the goal (a tree to cut, a gap between trees), half
        // the unit's collision does.
        let (from, radius) = ((u.x, u.y), (u.collision / crate::pathing::CELL).ceil().max(1.0) * crate::pathing::CELL * 2.0);
        let tight = u.collision;
        let kind = self.move_kind(h);
        let free = u.extra.no_pathing;
        let path = match &self.pathing {
            // Flyers go straight over everything but the map's no-fly cells, as does a
            // unit whose pathing is off (SetUnitPathing).
            Some(_) if kind == crate::pathing::Move::Fly || free => vec![goal],
            Some(p) => {
                let reaches = |path: &Vec<(f32, f32)>, to: (f32, f32)| path.last().is_some_and(|e| (e.0 - to.0).hypot(e.1 - to.1) < 64.0);
                let wide = p.find_path_kind(from, goal, radius * 0.5, 20000, kind);
                let direct = if reaches(&wide, goal) { wide } else { p.find_path_kind(from, goal, tight * 0.5, 20000, kind) };
                if reaches(&direct, goal) {
                    direct
                } else {
                    // Where the goal cannot be walked to, a waygate whose far side can
                    // reach it (the pathing's warps).
                    let mut via = None;
                    for (gate, dest) in self.waygate_routes() {
                        let mut to_gate = p.find_path_kind(from, gate, radius * 0.5, 20000, kind);
                        let near = to_gate.last().is_some_and(|e| (e.0 - gate.0).hypot(e.1 - gate.1) < 250.0);
                        if near && reaches(&p.find_path_kind(dest, goal, radius * 0.5, 20000, kind), goal) {
                            to_gate.push(gate);
                            via = Some(to_gate);
                            break;
                        }
                    }
                    via.unwrap_or(direct)
                }
            }
            None => vec![goal],
        };
        if let Some(u) = self.unit_mut(h) {
            u.path = path;
            u.repath_timer = 1.0;
        }
    }

    /// The type a unit shows as: a Polymorph's or Hex's critter while it lasts, else
    /// its own.
    pub fn shown_type(&self, h: u32) -> Option<Id> {
        let u = self.unit(h)?;
        Some(u.buffs.iter().find_map(|b| b.shown_as).unwrap_or(u.type_id))
    }

    /// Steps along the unit's path; returns true when it has arrived.
    /// How fast a unit moves now: its speed with items, upgrades, and buffs, kept
    /// within its type's minSpd and maxSpd, or where those are 0 MinUnitSpeed and
    /// MaxUnitSpeed (MinBldgSpeed and MaxBldgSpeed for a structure), each held to 1 to
    /// 522 (FUN_6f5fc900, FUN_6f68b0e0).
    pub fn current_move_speed(&self, h: u32) -> f32 {
        self.move_speed_at(h, None)
    }

    /// How fast a unit moves at main clock `at` (seconds since the game began): as
    /// `current_move_speed`, without the buffs whose timers have run out by then
    /// (`Buff::ends_at`), their speed's modifiers taken off as they run out, before the
    /// turn's buffs are counted down (read: Mafarazzo TD's fort runner, its Slow Poison
    /// ending at 10.44 seconds, walking at its full speed from the next step).
    pub fn move_speed_at(&self, h: u32, at: Option<f64>) -> f32 {
        let Some(u) = self.unit(h) else { return 0.0 };
        if u.move_speed <= 0.0 {
            return 0.0;
        }
        // A unit shown as a critter moves at the critter's speed.
        if let Some(v) = u.buffs.iter().map(|b| b.move_set).find(|&v| v > 0.0) {
            return v;
        }
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        let factor = match at {
            Some(t) => u.buffs.iter().filter(|b| b.move_factor != 0.0 && !b.ends_at.is_some_and(|e| e <= t)).fold(1.0f32, |m, b| m * (1.0 + b.move_factor)).max(0.0),
            None => self.buff_mods(h).0,
        };
        let speed = (u.move_speed + u.item_bonus.move_speed + ub.move_speed) * (1.0 + ub.move_pct) * factor;
        const LIMITS: (f32, f32) = (1.0, 522.0);
        // An uprooted ancient keeps a structure's limits (measured: an Ancient of War
        // walks at its 40, where a unit's least is 150).
        let (min_key, max_key) = if u.is_building || u.extra.uprooted { ("MinBldgSpeed", "MaxBldgSpeed") } else { ("MinUnitSpeed", "MaxUnitSpeed") };
        let o = self.objects.units.get(u.type_id);
        let own = |k: &str| o.map_or(0.0, |o| o.real(k));
        let global_min = self.misc_f32(min_key, 150.0).max(LIMITS.0);
        let global_max = self.misc_f32(max_key, 400.0).min(LIMITS.1).max(global_min);
        let min = match own("minSpd") {
            0.0 => global_min,
            v => v.clamp(LIMITS.0, LIMITS.1),
        };
        let max = match own("maxSpd") {
            0.0 => global_max,
            v => v.clamp(LIMITS.0, LIMITS.1),
        };
        speed.max(min).min(max)
    }

    /// Units ordered to a point together move in formation (Tip39 to Tip41): each at
    /// the speed of the slowest still on its way.
    pub fn form_group(&mut self, units: &[u32]) {
        let units: Vec<u32> = units.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.is_building && u.move_speed > 0.0)).collect();
        if units.len() < 2 {
            return;
        }
        for &h in &units {
            self.leave_formation(h);
        }
        let id = self.next_formation.max(1);
        self.next_formation = id + 1;
        for &h in &units {
            if let Some(u) = self.unit_mut(h) {
                u.extra.formation = Some(id);
            }
        }
        self.formations.insert(id, units);
    }

    /// Units sent together to one point take places in a formation there, as the
    /// original lays one out: about the square root of their number abreast across the
    /// way, the front rows filled first by formation rank (UnitData formation: melee
    /// before ranged before workers), then by who is ahead, each row left to right as
    /// they stand; places 4 sizes and 60 apart abreast and 3 sizes and 50 apart in
    /// depth, by the largest size among them (measured: nine footmen, size 31, end in
    /// a 3 by 3 grid 186 abreast and 143 deep, each where it stood among the rest;
    /// four tauren, 48, 250 and 195; four peasants or sorceresses, 16, 123 and 106;
    /// three footmen and three riflemen, the footmen in front).
    pub fn take_formation_places(&mut self, units: &[u32], goal: (f32, f32)) {
        let units: Vec<u32> = units.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.is_building && u.move_speed > 0.0)).collect();
        let n = units.len();
        if n < 2 {
            return;
        }
        let pos: Vec<(f32, f32)> = units.iter().map(|&h| self.unit(h).map(|u| (u.x, u.y)).unwrap()).collect();
        let (cx, cy) = (pos.iter().map(|p| p.0).sum::<f32>() / n as f32, pos.iter().map(|p| p.1).sum::<f32>() / n as f32);
        let (dx, dy) = (goal.0 - cx, goal.1 - cy);
        let len = dx.hypot(dy);
        if len < 1.0 {
            return;
        }
        let (fx, fy) = (dx / len, dy / len);
        let (px, py) = (-fy, fx);
        let size = units.iter().filter_map(|&h| self.unit(h)).map(|u| u.collision).fold(0.0, f32::max);
        let (abreast, deep) = (4.0 * size + 60.0, 3.0 * size + 50.0);
        let cols = (n as f32).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        let rank = |w: &World, h: u32| w.unit(h).and_then(|u| w.objects.units.get(u.type_id)).map_or(0, |o| o.int("formation"));
        let mut order: Vec<(i32, f32, f32, u32)> = units.iter().zip(&pos).map(|(&h, p)| (rank(self, h), (p.0 - cx) * fx + (p.1 - cy) * fy, (p.0 - cx) * px + (p.1 - cy) * py, h)).collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        for (r, row) in order.chunks(cols).enumerate() {
            let mut row = row.to_vec();
            row.sort_by(|a, b| a.2.total_cmp(&b.2));
            let m = row.len();
            let depth = ((rows - 1) as f32 / 2.0 - r as f32) * deep;
            for (k, &(_, _, _, h)) in row.iter().enumerate() {
                let side = (k as f32 - (m - 1) as f32 / 2.0) * abreast;
                let (x, y) = self.clamp_to_map(goal.0 + fx * depth + px * side, goal.1 + fy * depth + py * side);
                if let Some(u) = self.unit_mut(h) {
                    match &mut u.action {
                        Action::Move { x: ax, y: ay } | Action::AttackMove { x: ax, y: ay } => {
                            *ax = x;
                            *ay = y;
                            u.path.clear();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// A unit given another order leaves its formation.
    pub fn leave_formation(&mut self, h: u32) {
        let Some(id) = self.unit_mut(h).and_then(|u| u.extra.formation.take()) else { return };
        if let Some(m) = self.formations.get_mut(&id) {
            m.retain(|&x| x != h);
            if m.len() < 2 {
                for x in std::mem::take(m) {
                    if let Some(u) = self.unit_mut(x) {
                        u.extra.formation = None;
                    }
                }
                self.formations.remove(&id);
            }
        }
    }

    /// A unit's speed in its formation: the slowest of those still moving.
    fn formation_speed(&self, h: u32) -> f32 {
        let own = self.current_move_speed(h);
        let Some(id) = self.unit(h).and_then(|u| u.extra.formation) else { return own };
        self.formations.get(&id).map_or(own, |m| {
            m.iter().filter(|&&x| self.unit(x).is_some_and(|u| !u.dead && !u.path.is_empty())).map(|&x| self.current_move_speed(x)).fold(own, f32::min)
        })
    }

    /// A walker on the ground goes around a unit standing in its way rather than
    /// through it (measured: a footman sent past an idle enemy footman bows some 70
    /// out around it and leaves it where it stood; ours shoved it 267 aside): a turn
    /// beside the nearest one ahead, both sizes and 8 off, on the side the walker
    /// already leans to.
    fn steer_around(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let Some(&next) = u.path.first() else { return };
        if u.extra.no_pathing || self.is_ghost(u) || self.move_kind(h) == crate::pathing::Move::Fly {
            return;
        }
        let (ux, uy, r) = (u.x, u.y, u.collision);
        let (dx, dy) = (next.0 - ux, next.1 - uy);
        let len = dx.hypot(dy);
        if len < 1.0 {
            return;
        }
        let (fx, fy) = (dx / len, dy / len);
        let aim = match u.action {
            Action::Attack { target } | Action::Follow { target } | Action::Harvest { target } | Action::Repair { target } | Action::Construct { target } => target,
            Action::Cast { target, .. } => target,
            _ => 0,
        };
        let ahead = len.min(r * 2.0 + 64.0);
        let mut block: Option<(f32, f32, f32, f32)> = None;
        for o in self.units_near(ux, uy, ahead + r + 448.0) {
            if o == h || o == aim {
                continue;
            }
            let Some(v) = self.unit(o) else { continue };
            if v.dead || v.hidden || v.is_building || v.collision <= 0.0 || !v.path.is_empty() || v.extra.no_pathing || self.is_ghost(v) {
                continue;
            }
            let reach = r + v.collision;
            if (v.x - ux).abs() > ahead + reach || (v.y - uy).abs() > ahead + reach {
                continue;
            }
            if self.move_kind(o) == crate::pathing::Move::Fly {
                continue;
            }
            let (ox, oy) = (v.x - ux, v.y - uy);
            let along = ox * fx + oy * fy;
            let across = ox * fy - oy * fx;
            if along <= 0.0 || along > ahead || across.abs() >= reach {
                continue;
            }
            if block.is_none_or(|b| along < b.0) {
                block = Some((along, across, reach, 0.0));
                block.as_mut().unwrap().3 = o as f32;
            }
        }
        let Some((_, across, reach, o)) = block else { return };
        let Some(v) = self.unit(o as u32) else { return };
        let (vx, vy) = (v.x, v.y);
        // Pass on the side away from it (to the left of the way when it stands square
        // in it, as measured); the other side if the ground there will not have the
        // walker.
        let side = if across < 0.0 { 1.0 } else { -1.0 };
        let off = reach + 8.0;
        let at = |s: f32| (vx + fy * off * s, vy - fx * off * s);
        let fits = |p: (f32, f32)| self.pathing.as_ref().is_none_or(|m| m.fits(m.cell_of(p.0, p.1), r * 0.5));
        let turn_at = if fits(at(side)) { at(side) } else if fits(at(-side)) { at(-side) } else { return };
        if let Some(u) = self.unit_mut(h) {
            if u.path.first().is_some_and(|&p| (p.0 - turn_at.0).hypot(p.1 - turn_at.1) < 1.0) {
                return;
            }
            u.path.insert(0, turn_at);
            u.extra.detour = Some(turn_at);
        }
    }

    /// Walks a unit through the engine's movement steps of this tick, 30 ms each (the
    /// original's NIpse steps), noting when it arrives at the last of them.
    pub(crate) fn walk(&mut self, h: u32, dt: f32) -> bool {
        let t0 = self.profile.is_some().then(std::time::Instant::now);
        let r = if self.unit(h).is_some_and(|u| u.extra.local_paths || u.extra.player_walk) { self.walk_steps(h) } else { self.walk_inner(h, dt) };
        self.prof_add("walk", t0);
        r
    }

    /// The first movement step of the tick for units walking already, before the tick's
    /// logic.
    pub(crate) fn first_steps(&mut self, vm: &mut Vm) {
        if let Some(&t) = self.step_ticks.first() {
            self.fire_mana_listeners(Some(t));
            // The turn's first main tick's blight checks, by its game time (read: the
            // undead start's due at 2.0, made at 1.999998 on the main clock), then its
            // timers.
            if !self.tick_clocks.1.is_empty() {
                self.blight_checks_to(self.tick_clocks.0 as f64 * 0.005);
            }
            self.carried_region_triggers(vm);
            self.script_clock = self.tick_clocks.1.first().copied();
            let before = self.tags.doomed.len();
            self.timers_at_turn_start(vm);
            self.script_clock = None;
            self.time_of_day_to(vm, Some(t));
            self.timed_frees_to(Some(t));
            // What the turn's timers released in its first main tick goes in the next,
            // before a first step in a later tick, after the timed frees due by it
            // (measured: maftr's New Round, its timer due at 20.0 in turn 201, whose
            // first step is two main ticks on: its creeps' old orders, `+tex`, and
            // `wsed` freed before that step's look and the walk ended at the turn
            // before's last step, the original's next free slot after the turn that
            // walk's `^clu`; its leaderboard's timer at 2.0 in turn 21, its `+tex`
            // after the hall's build order's leftovers; provisional: whether every
            // release of the first tick goes so).
            if t > self.tick_clocks.0 && self.tags.doomed.len() > before {
                let mine: Vec<u32> = self.tags.doomed.drain(before..).collect();
                for s in mine {
                    self.tags.release(s);
                }
            }
            self.ai_threads_to_tick(Some(t));
            // Constructions and upgrades due by the first step's tick are done in it, as in
            // the later steps, before its walks' ended paths go (traced: the Tower of
            // Vengence's second upgrade, due 34.897661, its objects freed in turn 350
            // before a creep's old walk's).
            self.constructions_to(vm, Some(t));
            self.upgrades_to(vm, Some(t));
            self.mine_payouts_to(vm, Some(t));
        }
        // The paths of the walks ended in the last step go in this one, in the walkers'
        // order (`Tags::walk_ended`).
        let mut carried = if self.step_clocks.is_empty() { Vec::new() } else { self.tags.take_step_paths() };
        self.tags.begin_first_step();
        self.step_range = (0, self.step_clocks.len().min(1));
        if self.wrap_step.is_some_and(|(k, _)| k == 0 && !self.step_clocks.is_empty()) {
            self.wrap_clocks(&mut []);
        }
        // The detectors' looks due before the step (`World::plan_detection_looks`), and
        // the structures' damage points (`World::points_before`).
        self.looks_before(self.step_abs(0).map(|s| s.0));
        if let Some((main, due)) = self.step_abs(0) {
            self.structure_events_before(due);
            self.points_before(vm, Some(due));
            self.aimed_landings_before(vm, main);
            // (What they let go as an earlier main tick ended goes before the step:
            // traced, the second flyer game's Arrow Tower's take-up's block at turn 330,
            // freed before the step's walk ends' objects.)
            self.timed_frees_to(self.step_ticks.first().copied());
        }
        let mut walkers: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.steps_walker())).collect();
        // The game's first step (its clock 0) moves no one: the walks ordered as the map
        // is set up are searched and steered from the next (read: the waypoint provider,
        // `FUN_6f165ae0`, first asked at 0.03 for Twisted Meadows' computers' workers,
        // the human's third peasant's way ending at 0.30 and its entry at 0.33).
        if self.tick_clocks.0 == 0 && self.step_index == 0 {
            walkers.clear();
        }
        // The newest walk first (traced: sheep 1050120, given its order after 1050117,
        // held it back before it moved in a step; pig 1050123 likewise held 1050124).
        walkers.sort_by_key(|&h| std::cmp::Reverse(self.unit(h).map_or(0, |u| u.extra.walk_serial)));
        let stepped = walkers.clone();
        // New orders have their ways found in this step, each as its walker's turn in
        // it comes, after the walkers before it have moved: one that moved first is a
        // mover to the search (read: Twisted Meadows' human computer's five peasants'
        // searches at 0.03, newest first, their ways of 3, 7, 3, 9, and 6 points, the
        // fourth's through the cell beside the second, which had just begun to move;
        // the second's turned by the build peasant, which had only turned).
        for h in walkers {
            // The ended walks' paths newer than this walker's go before it.
            let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
            let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut carried).into_iter().partition(|&(_, s)| s > serial);
            self.tags.free_step_paths(now);
            carried = later;
            // A walk to a structure ends in the walkers' order (`World::gold_run_step_end`).
            if self.reach_walk_step(h, 0) || self.gold_run_step_end(h, 0) || self.coarse_step_arrive(h, 0) || self.acolyte_step_arrive(h, 0) || self.build_step_arrive(vm, h, 0) || self.construct_step_arrive(h) || !self.unit(h).is_some_and(|u| u.steps_walker()) {
                continue;
            }
            // One given before this step (this tick's) walks from the next, one given
            // after the last tick's steps from this one (`World::update_wander`).
            let Some(u) = self.unit(h) else { continue };
            if u.extra.mover.is_none() {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.walk_from = None;
                }
            }
            if self.unit(h).is_some_and(|u| u.extra.mover.is_none() && u.path.is_empty()) {
                if let Some(g) = self.walk_goal(h) {
                    self.path_to(h, g);
                }
            }
            let stopped = self.walk_steps(h);
            // (An AI's acolyte in range after its move ends its walk in this step; one
            // stopped short keeps to its harvest, going on by itself.)
            if self.acolyte_step_arrive(h, 0) || self.build_step_arrive(vm, h, 0) {
            } else if stopped && self.unit(h).is_some_and(|u| u.ai_build_walk()) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.build_legacy = true;
                }
            } else if stopped && self.unit(h).is_some_and(|u| u.ai_harvest_walk()) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.ai_order.push(u32::MAX);
                }
            } else if stopped {
                if let Some(u) = self.unit_mut(h) {
                    u.action = Action::Idle;
                    u.order = Order::default();
                }
            }
        }
        self.tags.free_step_paths(carried);
        for h in stepped {
            self.step_regions(vm, h);
        }
        let tick = self.step_ticks.first().copied();
        self.ai_threads_at_tick(tick);
        let mut late: Vec<(usize, u32, u32)> = if tick.is_some() { self.agent_dues().into_iter().filter(|e| e.0 == 1 && self.agent_tick(e.1) == tick).collect() } else { Vec::new() };
        late.sort_by(|a, b| f32::from_bits(a.1).total_cmp(&f32::from_bits(b.1)));
        let trigger = self.region_trigger_due(0);
        let after = self.after_step_events(vm, &late, 0, trigger);
        self.step_tick_frees(0);
        // Orders given at this step's tick free their objects after it, before its walk
        // ends' (read: a wanderer's order's eight, given before a turn's first step,
        // freed before the trees of two walks that arrived in it; Twisted Meadows' turn
        // 1782), as a later step's (`Tags::end_step`).
        self.tags.flush_issued();
        self.tags.end_first_step();
        self.make_pending_pairs();
        self.tags.issued.extend(after);
        self.tags.flush_issued();
        self.later_region_triggers(vm, trigger);
    }

    /// The timed releases falling due in step `k`'s main tick go after its events, before
    /// its end's frees: each is an event on the agent queue, run in the main tick that
    /// reaches it (a missile's removal, `FUN_6f6d4e30` from 0x6f060d12).  Traced: the
    /// second flyer game's Arrow Tower's missile, removed in turn 338 after the step's
    /// region trigger's objects and before its walk ends' frees.
    fn step_tick_frees(&mut self, k: usize) {
        let Some(&t) = self.step_ticks.get(k) else { return };
        let Ok(i) = usize::try_from(t - self.tick_clocks.0 + 1) else { return };
        if let Some(c) = self.tick_abs(i) {
            self.tags.free_timed(self.steps_free_until.map_or(c, |f| c.min(f)));
        }
    }

    /// When a step's region events run their triggers (`World::step_regions`): each
    /// registration's timer is started with no wait, which the agent queue takes as
    /// its least, 0.0001 (`CAgentTimer::vf2_6f060190`, `DAT_6fcd539c`), so it falls due
    /// that much after the step's agent clock, in the main tick that reaches it.  The
    /// due, and whether that tick is the step's.
    fn region_trigger_due(&self, k: usize) -> Option<(u32, bool)> {
        if self.step_region_events.is_empty() {
            return None;
        }
        let due = crate::efloat::add(*self.step_agent.get(k)?, 0.0001f32.to_bits());
        Some((due, self.agent_tick(due) == self.step_ticks.get(k).copied()))
    }

    /// The agent queue's events due after step `k` in its main tick, in time order: the
    /// wanderers' orders (`World::issue_after_step`) and the region events' triggers
    /// when they fall due there, their objects made before the tick's frees and theirs
    /// freed after the step's walk ends' (returned).  Read: Mafarazzo TD's creep
    /// arriving at WalkMid12's middle as it entered it at turn 349's second step, its
    /// trigger due in that tick, the new order's objects made before the old walk's
    /// tree was freed (the original's next free slot after the turn the next trigger's
    /// `+tex`, this one's freed slot).
    fn after_step_events(&mut self, vm: &mut Vm, late: &[(usize, u32, u32)], k: usize, trigger: Option<(u32, bool)>) -> Vec<u32> {
        let Some((at, true)) = trigger else { return self.issue_after_step(late, k) };
        let (sooner, later): (Vec<_>, Vec<_>) = late.iter().copied().partition(|e| self.kept_abs(e.1) < self.kept_abs(at));
        let mut after = if sooner.is_empty() { Vec::new() } else { self.issue_after_step(&sooner, k) };
        after.extend(self.run_step_region_events(vm));
        // What a timer let go in the tick before the triggers ran goes with the tick's
        // frees, after their makings (traced: maftr's peasant's build order's seven
        // objects, due 32.395412 as turn 325 began, its first step in that tick and a
        // creep's region trigger due 32.399323 there: the trigger's new order made, the
        // seven freed, then its old order's).
        self.tags.free_timed(self.kept_abs(at));
        if !later.is_empty() {
            after.extend(self.issue_after_step(&later, k));
        }
        after
    }

    /// The region events' triggers falling due in a later main tick than their step's:
    /// after its tick's frees (read: Mafarazzo TD's two creeps arriving at WalkMid12's
    /// and WalkMid34's middles as they entered them at turn 346's last step, their
    /// triggers due a tick on, their walks' trees freed before the orders took their
    /// objects, the original's next free slot after the turn the second order's `+tex`,
    /// the first's `^clg`); due after the turn's last tick, at the next turn's first
    /// (`World::carried_region_triggers`).
    fn later_region_triggers(&mut self, vm: &mut Vm, trigger: Option<(u32, bool)>) {
        let Some((at, false)) = trigger else { return };
        if self.agent_tick(at).is_none() {
            return;
        }
        for s in self.run_step_region_events(vm) {
            self.tags.release(s);
        }
    }

    /// The triggers due after the last turn's last main tick, at this turn's first, before
    /// its timers: what they release goes as that tick ends, after the turn's first step
    /// when it is taken then (`Tags::flush_issued`).  Provisional: not yet seen.
    fn carried_region_triggers(&mut self, vm: &mut Vm) {
        if self.step_region_events.is_empty() {
            return;
        }
        let freed = self.run_step_region_events(vm);
        if self.step_ticks.first() == Some(&self.tick_clocks.0) {
            self.tags.issued.extend(freed);
        } else {
            for s in freed {
                self.tags.release(s);
            }
        }
    }

    /// The tick's movement steps after its first for wanderers, step by step, each
    /// walker in turn: a new order's way found in its first step (its search in the
    /// step after its order, traced), then its steps.
    pub(crate) fn step_walkers(&mut self, vm: &mut Vm) {
        let n = self.step_clocks.len();
        let mut due = std::mem::take(&mut self.wander_due);
        for k in 1..n {
            // The orders due before this step are given first (traced: a pig's walk ended
            // in a turn's second step, before an order due after it).
            self.tags.mark_step(k);
            self.looks_before(self.step_abs(k).map(|s| s.0));
            if let Some((main, due)) = self.step_abs(k) {
                self.structure_events_before(due);
                self.points_before(vm, Some(due));
                // (Those due after the damage points, which land those due before them.)
                self.aimed_landings_before(vm, main);
            }
            let now: Vec<(usize, u32, u32)> = due.iter().copied().filter(|e| e.0 == k).collect();
            let next = self.step_ticks.get(k).copied();
            // The payouts by a wanderer's main tick go ahead of its order (read: a night
            // elf start's entangled mine, due 125.848091, its pair made before the
            // objects of a wanderer's order due 125.863121 before the turn's second step
            // were freed; and due 56.616413 in the main tick of a wanderer's due
            // 56.620937, likewise).
            let mut i = 0;
            while i < now.len() {
                let t = self.agent_tick(now[i].1);
                let j = now[i..].iter().position(|e| self.agent_tick(e.1) != t).map_or(now.len(), |p| i + p);
                if let Some(t) = t {
                    self.mine_payouts_to(vm, Some(t));
                }
                self.issue_wanders(&now[i..j], next);
                i = j;
            }
            if self.wrap_step.is_some_and(|(w, _)| w == k) {
                self.wrap_clocks(&mut due);
            }
            self.fire_mana_listeners(next);
            self.time_of_day_to(vm, next);
            self.timed_frees_to(next);
            self.ai_threads_to_tick(next);
            self.trainings_to(vm, next);
            self.constructions_to(vm, next);
            self.upgrades_to(vm, next);
            self.mine_payouts_to(vm, next);
            // The paths of the walks ended in the last step go in this one, in the walkers'
            // order (`Tags::walk_ended`).
            let mut carried = self.tags.take_step_paths();
            self.step_range = (k, k + 1);
            let mut walkers: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.steps_walker())).collect();
        // The newest walk first (traced: sheep 1050120, given its order after 1050117,
        // held it back before it moved in a step; pig 1050123 likewise held 1050124).
        walkers.sort_by_key(|&h| std::cmp::Reverse(self.unit(h).map_or(0, |u| u.extra.walk_serial)));
            let stepped = walkers.clone();
            // Its walk ends' objects wait for its end as a first step's do (traced: two
            // give-ups in a turn's second step make their new objects, then free all).
            self.tags.begin_step();
            // Each walker's search comes in its turn in the step, before its movement (read:
            // a sheep about to move in the step still blocks another's search in it).
            for h in walkers {
                let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
                let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut carried).into_iter().partition(|&(_, s)| s > serial);
                self.tags.free_step_paths(now);
                carried = later;
                if self.reach_walk_step(h, k) || self.gold_run_step_end(h, k) || self.coarse_step_arrive(h, k) || self.acolyte_step_arrive(h, k) || self.build_step_arrive(vm, h, k) || self.construct_step_arrive(h) || !self.unit(h).is_some_and(|u| u.steps_walker()) {
                    continue;
                }
                let Some(u) = self.unit(h) else { continue };
                if u.extra.mover.is_none() && u.extra.walk_from.is_some_and(|f| f > k) {
                    continue;
                }
                if self.unit(h).is_some_and(|u| u.extra.mover.is_none() && u.path.is_empty()) {
                    if let Some(g) = self.walk_goal(h) {
                        self.path_to(h, g);
                    }
                }
                let stopped = self.walk_steps(h);
                // (An AI's acolyte in range after its move ends its walk in this step; one
                // stopped short keeps to its harvest, going on by itself.)
                if self.acolyte_step_arrive(h, k) || self.build_step_arrive(vm, h, k) {
                } else if stopped && self.unit(h).is_some_and(|u| u.ai_build_walk()) {
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.build_legacy = true;
                    }
                } else if stopped && self.unit(h).is_some_and(|u| u.ai_harvest_walk()) {
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.ai_order.push(u32::MAX);
                    }
                } else if stopped {
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        u.order = Order::default();
                    }
                }
            }
            // As the step ends: the paths of the walks older than its walkers, then the
            // objects of orders given at this step's tick, then its walk ends' (read: a
            // wanderer's order's eight freed after a peon's path and before a give-up's
            // tree in the same step), then those of orders due after it in its tick.
            self.tags.free_step_paths(carried);
            for h in stepped {
                self.step_regions(vm, h);
            }
            let tick = self.step_ticks.get(k).copied();
            self.ai_threads_at_tick(tick);
            let (late, rest): (Vec<_>, Vec<_>) = due.iter().copied().partition(|e| e.0 == k + 1 && self.agent_tick(e.1) == tick);
            due = rest;
            let trigger = self.region_trigger_due(k);
            let after = self.after_step_events(vm, &late, k, trigger);
            self.step_tick_frees(k);
            self.tags.end_step();
            self.make_pending_pairs();
            self.tags.issued.extend(after);
            self.tags.flush_issued();
            self.later_region_triggers(vm, trigger);
        }
        self.wrap_clocks(&mut due);
        let rest: Vec<(usize, u32, u32)> = due.iter().copied().filter(|e| e.0 >= n.max(1)).collect();
        // The payouts due after the last step by the tick's next-to-last main tick
        // (read: Forestwalk's undead computer's mine, due at 8.07 after turn 81's last
        // step, paying in that turn; those by its last, `World::last_tick_payouts`),
        // those in main ticks before a wanderer's ahead of it (read: the mine's due
        // 26.980217, in the tick before a wanderer's due 26.987633 after turn 270's
        // last step, its two objects made before the wanderer's order's eight were
        // freed).
        let last = self.tick_clocks.1.len().checked_sub(2).map(|i| self.tick_clocks.0 + i as i64);
        let mut i = 0;
        while i < rest.len() {
            let t = self.agent_tick(rest[i].1);
            let j = rest[i..].iter().position(|e| self.agent_tick(e.1) != t).map_or(rest.len(), |p| i + p);
            if let (Some(t), Some(l)) = (t, last) {
                self.mine_payouts_to(vm, Some((t - 1).min(l)));
            }
            self.issue_wanders(&rest[i..j], None);
            i = j;
        }
        self.fire_mana_listeners(None);
        self.trainings_to(vm, None);
        self.constructions_to(vm, None);
        self.upgrades_to(vm, None);
        // Build orders ending after the last step by the turn's last main tick end in
        // it (read: Forestwalk's undead computer's crypt begun at 1.979998, its order
        // ending at 2.979974 in turn 30, after its last step at 2.969974).
        if let Some(end) = self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i)) {
            self.build_ends_to(end);
        }
        self.mine_payouts_to(vm, last);
        // Looks due after the last step that its agent queue reaches as the turn ends,
        // as it is brought to the next main tick's clock (provisional: where among the
        // rest); the others wait for the next turn's (read: the maftw game's flying
        // peasant's look due 155.0, after turn 1551's last step at 154.97177 and past its
        // agent queue's reach, its detection made as turn 1552 began).
        let reach = self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i));
        self.looks_before(reach);
        if let Some(r) = reach.filter(|_| n > 0) {
            self.structure_events_before(r);
        }
        if let Some(left) = self.looks_due.as_mut() {
            self.looks_carried = std::mem::take(left);
        }
        self.step_range = (1, n);
    }

    /// Whether a walker is held back this step (the waypoint provider, `FUN_6f165ae0`):
    /// while it waits it counts its wait down; else, short of its waypoint (beyond 0.49),
    /// it looks at the cells it moves into (`FUN_6f166140`) for other walkers moving
    /// there and settles who gives way (`FUN_6f168360`): it waits at least 4 steps for
    /// one no slower than it (or of another size class), else that one waits at least
    /// 20.  Blocked by a unit standing there or a wall, and not waiting, it counts down
    /// (`FUN_6f167290`, `FUN_6f1689d0`: from 2 when its goal is within 12 cells, else 7
    /// to 9) and then searches again from where it is (the provider's answer 4).
    fn held_back(&mut self, h: u32, m: &crate::motion::Mover, to: (u32, u32), clock: u32) -> bool {
        use crate::efloat as ef;
        let Some(u) = self.unit_mut(h) else { return false };
        if u.extra.walk_wait > 0 {
            u.extra.walk_wait -= 1;
            return true;
        }
        let (x, y) = m.at(clock);
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let d2 = f32::from_bits(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let reach = f32::from_bits(ef::mul(0.49f32.to_bits(), 0.49f32.to_bits()));
        u.extra.yields_to = None;
        // A worker whose harvest searches past units walks past them too (read: a peon
        // home from the mine along row 265 never held by the idle peon whose region
        // covers 672, 264, nor drawing for a block; `Unit::region_mask`).
        let mask = u.region_mask();
        // (Its path's goal: a cluster walker's, its cluster's point.)
        let goal = self.walk_target(h);
        let Some(u) = self.unit_mut(h) else { return false };
        // Within reach of the end of a way short of its goal, it is as one blocked: it
        // holds and then searches again (traced: a sheep at 0.42 from its way's end held
        // a step at 11.70 and searched at 11.73).
        let short_end = u.path.len() == 1 && u.extra.path_end_cell.is_some();
        if d2 <= reach && !short_end {
            return false;
        }
        let class = crate::lrpath::class_of(u.collision / crate::pathing::CELL);
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return false };
        // The cells a step ahead: the place at most a cell toward the waypoint
        // (`FUN_6f168280`).
        let (dx, dy) = unit_dir(ef::sub(to.0, x), ef::sub(to.1, y));
        let (ax, ay) = (ef::add(x, dx), ef::add(y, dy));
        let (cx, cy) = (f32::from_bits(x).floor() as i32, f32::from_bits(y).floor() as i32);
        let (tx, ty) = (f32::from_bits(ax).floor() as i32, f32::from_bits(ay).floor() as i32);
        let cells = edge_cells(class, (cx, cy), (tx, ty));
        let kind = self.move_kind(h);
        let wall = self.pathing.as_ref().is_some_and(|p| cells.iter().any(|&c| !p.passable(c, kind)));
        let mine = m.speed2();
        // Each other walker whose region lies on those cells, in their order.
        let mut seen = Vec::new();
        for &c in &cells {
            for &o in &self.unit_order {
                if o == h || seen.contains(&o) {
                    continue;
                }
                // (A flyer holds no other back, as it takes no cells from the searches,
                // `regions_taken`; provisional: a flying walker had stood for good
                // behind a wisp beside it, Mafarazzo TD's flying peasant sent past one.)
                let Some(v) = self.unit(o).filter(|v| !v.dead && !v.hidden && !v.is_building && v.collision > 0.0 && v.region_flags() & mask != 0) else { continue };
                if self.move_kind(o) == crate::pathing::Move::Fly {
                    continue;
                }
                // A walker held after moving holds the cells of both its last places (read:
                // a held sheep's region three cells wide, another's still on a row it had
                // just left; a pig moving on steadily held only its place).
                // Its region is at the middle of its cell (`regions_taken`).
                let (px, py) = (((v.x - origin[0]) / crate::pathing::CELL).floor() + 0.5, ((v.y - origin[1]) / crate::pathing::CELL).floor() + 0.5);
                let (qx, qy) = (px, py);
                let r = v.collision / crate::pathing::CELL;
                let on = |a: f32, b: f32, c: i32| (a.min(b) - 0.5 - r).floor() as i32 <= c && c <= (a.max(b) - 0.5 + r).floor() as i32;
                if on(px, qx, c.0) && on(py, qy, c.1) {
                    seen.push(o);
                }
            }
        }
        // A computer's captain's region on those cells blocks it as a standing unit's
        // (`FUN_6f166140`'s collection, `FUN_6f148e30`: read, Twisted Meadows' night elf
        // computer's wisp at (165.3233, 189.1246) at 0.33 turning west for its way's end,
        // the one region it collected the defense captain's, its `` `pcu ``, in (163,
        // 188); its way dropped, it searched again from there at 0.42).
        let (ux, uy) = self.unit(h).map_or((0.0, 0.0), |u| (u.x, u.y));
        let captain = mask & 0x02 != 0 && self.captain_cells((ux, uy)).iter().any(|c| cells.contains(c));
        let blocked = wall || captain || !seen.is_empty() || d2 <= reach;
        if blocked {
            log::trace!("held {h}: wall {wall} captain {captain} seen {seen:?} near {} cells {cells:?}", d2 <= reach);
        }
        for o in seen {
            let Some(v) = self.unit(o) else { continue };
            let Some(theirs) = v.extra.mover.filter(|_| v.engine_walker()).map(|m| m.speed2()) else { continue };
            if theirs & 0x7fff_ffff == 0 || v.extra.yields_to.is_some_and(|w| self.unit(w).is_some_and(|w| !w.dead)) {
                continue;
            }
            let other_class = crate::lrpath::class_of(v.collision / crate::pathing::CELL);
            if !(f32::from_bits(theirs) < f32::from_bits(mine)) || other_class != class {
                let u = self.unit_mut(h).unwrap();
                u.extra.walk_wait = u.extra.walk_wait.max(4);
                u.extra.yields_to = Some(o);
                return true;
            }
            log::trace!("held: {o} waits for {h}");
            let v = self.unit_mut(o).unwrap();
            v.extra.walk_wait = v.extra.walk_wait.max(20);
            v.extra.yields_to = Some(h);
        }
        if !blocked {
            // A free step clears the count (`FUN_6f1687e0`).
            if let Some(u) = self.unit_mut(h) {
                u.extra.walk_blocked = 0;
            }
            return false;
        }
        if self.unit(h).is_some_and(|u| u.extra.walk_blocked == 0) {
            let near = goal.is_some_and(|g| {
                let (gx, gy) = ((g.0 - origin[0]) / crate::pathing::CELL, (g.1 - origin[1]) / crate::pathing::CELL);
                let (px, py) = (f32::from_bits(x), f32::from_bits(y));
                (gx - px) * (gx - px) + (gy - py) * (gy - py) <= 144.0
            });
            // A far goal: 7 or 8, drawn on the main generator (`FUN_6f1d62b0(2)`: the
            // draw's mantissa as a single in 1 to 2, less 1, times 2, truncated; the
            // generator is the object at DAT_6fd53a48, read: the wanderers' waits are
            // drawn on it too).
            let k = if near {
                2
            } else {
                let r = f32::from_bits((self.rng.next_u32() & 0x7f_ffff) | 0x3f80_0000) - 1.0;
                7 + (r * 2.0) as u32
            };
            if let Some(u) = self.unit_mut(h) {
                u.extra.walk_blocked = k;
            }
        }
        let Some(u) = self.unit_mut(h) else { return false };
        // Blocked again just after searching again (the provider's answer 4): it holds,
        // to search once more (what the original does then is not yet read).
        if u.extra.walk_blocked > 1 {
            u.extra.walk_blocked -= 1;
        }
        // Its way is dropped, to be searched again as the next step starts (read: index
        // -1 and count 0 at 9.42 after a block at 9.39, the search then, and a move in
        // that step when the new way is clear).
        u.extra.walk_research = true;
        true
    }

    /// Walks a unit through this tick's movement steps as the original's engine does
    /// (`motion`): toward each waypoint in turn, reached within 0.49 cells with its
    /// heading on it; at the end of a way short of the goal it searches again, and
    /// stops when nothing nearer is found.  Notes whether it stopped in the last step.
    /// Its walk as the steps begin is kept for a slow landing among them, with whether
    /// they only moved it on (`World::slowed_among_steps`).
    fn walk_steps(&mut self, h: u32) -> bool {
        let tick = self.tick_clocks.0;
        let step = self.step_range.0;
        let walk = |u: &Unit| (u.path.len(), u.path.first().map(|p| (p.0.to_bits(), p.1.to_bits())), u.extra.coarse.as_ref().map(|c| c.index), u.extra.move_coarse.as_ref().map(|c| c.index), u.extra.walk_research, u.extra.coarse_done, u.extra.facing_goal, u.extra.face_hold, u.extra.walk_blocked, u.extra.walk_wait, u.extra.order_tags.len(), u.extra.walk_serial);
        let before = self.unit(h).map(|u| (StepMark { tick, step, mover: u.extra.mover, place: (u.x, u.y, u.facing), cell_at: u.extra.cell_at, step_from: u.extra.step_from, plain: false }, walk(u)));
        let stopped = self.walk_steps_inner(h);
        if let (Some((mut mark, was)), Some(u)) = (before, self.unit_mut(h)) {
            if u.extra.step_marks.first().is_some_and(|m| m.tick != tick) {
                u.extra.step_marks.clear();
            }
            mark.plain = !stopped && mark.mover.is_some() && u.extra.mover.is_some() && walk(u) == was;
            u.extra.step_marks.push(mark);
        }
        stopped
    }

    /// A walker slowed by a missile landing at `at` (the main clock, seconds since the
    /// game began) among this tick's movement steps, which the port ran before the
    /// landing.  The buff's slow (`FUN_6f63d510`) changes its move ability's factor
    /// (`FUN_6f5fb7c0`): a walker slowed from unslowed leaves its cluster for one of
    /// its own (`World::leaves_cluster`), then its position's top speed is set
    /// (`FUN_6f6005d0`, `SmartPosition::vf6_6f05c5c0` → `FUN_6f15ff40`), its velocity
    /// held to the new top at once, its place carried on to then (`Mover::lower_top`).
    /// So its steps after the landing are walked again from there (when they only moved
    /// it on; a landing after its last step holds its mover for the next).  Measured:
    /// Mafarazzo TD's fort runner, the Slow Poison's buff landing in the main tick
    /// 12.603886 (breakpoints on `FUN_6f15ff40` and `FUN_6f16c880` under the missile's
    /// landing, `FUN_6f6d4e30` → `CMissileSlowPoison::vf51_6f6227b0` → ... →
    /// `CBuffSlowPoisonStackInfo::vf213_6f6236e0` → `FUN_6f63d510`), its pings from then
    /// at 150 a second, the port's at its 280 to the turn's end.
    pub(crate) fn slowed_among_steps(&mut self, h: u32, at: f64, was_slowed: bool) {
        let slowed = self.buff_mods(h).0 < 1.0 - 1e-6;
        if !was_slowed && slowed {
            self.leaves_cluster(h);
        }
        let Some(u) = self.unit(h).filter(|u| u.extra.mover.is_some() && u.steps_walker()) else { return };
        // (By the main clock of the tick it lands in: read, the runner's pings each the
        // original's to 0.001 so, 0.15 off by the port's landing's own moment.)
        let n = self.tick_clocks.1.len();
        let Some(i) = (0..n).filter(|&i| self.tick_abs(i).is_some_and(|c| c < at - 1e-7)).last() else { return };
        let clock = self.tick_clocks.1[i];
        let top = (self.gold_run_speed(h).unwrap_or_else(|| self.current_move_speed(h)) / crate::pathing::CELL).to_bits();
        let tick = self.tick_clocks.0;
        let after = u.extra.step_marks.iter().position(|m| m.tick == tick && self.step_abs(m.step).is_some_and(|(_, a)| a > at));
        log::trace!("{h} slowed at {at:.6} (clock {:.6}), its top {:.4}: steps after {after:?} of {:?}", f32::from_bits(clock), f32::from_bits(top) * 32.0, u.extra.step_marks.iter().map(|m| (m.tick, m.step, m.plain)).collect::<Vec<_>>());
        match after {
            None => {
                if let Some(u) = self.unit_mut(h) {
                    if u.extra.mover.as_mut().is_some_and(|m| m.lower_top(clock, top)) {
                        u.extra.placed_late = true;
                    }
                }
            }
            Some(i) if u.extra.step_marks[i..].iter().all(|m| m.plain) => {
                let mark = u.extra.step_marks[i].clone();
                let Some(mut m) = mark.mover else { return };
                let held = m.lower_top(clock, top);
                if let Some(u) = self.unit_mut(h) {
                    u.extra.step_marks.truncate(i);
                    u.extra.mover = Some(m);
                    (u.x, u.y, u.facing) = mark.place;
                    u.extra.cell_at = mark.cell_at;
                    u.extra.step_from = mark.step_from;
                    u.extra.placed_late = held;
                }
                let range = self.step_range;
                self.step_range = (mark.step, self.step_clocks.len());
                let stopped = self.walk_steps_inner(h);
                self.step_range = range;
                if stopped {
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        u.order = Order::default();
                    }
                }
            }
            Some(_) => {}
        }
    }

    /// A cluster walker slowed leaves its cluster for one of its own (`FUN_6f5fb7c0`, its
    /// factor passing under 1, → `FUN_6f5fcd50` while its count at +0x80 is 0 →
    /// `FUN_6f058bc0` → `FUN_6f16e7c0`, `FUN_6f1694f0`, `FUN_6f169550`): a cluster group
    /// made with the member and the old one's goal (`^clg`), started as a walk's
    /// (`FUN_6f16bcf0`: its cluster, `^clu`, linked at the list's head, `FUN_6f16bdb0`,
    /// `FUN_6f14ef80`, `FUN_6f16c880`, and its path, `l^pa`), the group freed at once,
    /// then the old path and the old cluster, left empty; its ways planned anew as its
    /// next step begins, newest first.  Traced: Mafarazzo TD's fort runner walking back
    /// at turn 127, `Bssi`, `Bssd`, then `^clg`, `^clu`, `l^pa`, the `^clg`, the old
    /// `l^pa`, and the old `^clu` freed.  (Provisional: a group's other members, which
    /// keep the old cluster, and one walking alone otherwise, are not read.)
    fn leaves_cluster(&mut self, h: u32) {
        if !self.cluster_walker(h) {
            return;
        }
        let Some(u) = self.unit(h).filter(|u| u.extra.mover.is_some()) else { return };
        let n = u.extra.order_tags.len();
        let kind = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        if n < 2 || kind(u.extra.order_tags[n - 1]) != Some(*codes::class::PATH) || kind(u.extra.order_tags[n - 2]) != Some(*codes::class::CLUSTER) {
            return;
        }
        let (path, cluster) = (u.extra.order_tags[n - 1], u.extra.order_tags[n - 2]);
        let group = self.tags.take(codes::class::CLUSTER_GOAL);
        let new_cluster = self.tags.take(codes::class::CLUSTER);
        let new_path = self.tags.take(codes::class::PATH);
        for s in [group, path, cluster] {
            self.tags.release(s);
        }
        let serial = self.tags.born;
        if let Some(u) = self.unit_mut(h) {
            u.extra.order_tags[n - 2] = new_cluster;
            u.extra.order_tags[n - 1] = new_path;
            u.extra.walk_serial = serial;
            u.extra.coarse = None;
            u.extra.move_coarse = None;
            u.extra.coarse_done = false;
            u.extra.plan_at = [None; 2];
            u.extra.walk_research = true;
        }
    }

    fn walk_steps_inner(&mut self, h: u32) -> bool {
        use crate::efloat as ef;
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return true };
        let f = |v: f32| v.to_bits();
        let cell = |w: f32, o: f32| ef::div(ef::sub(f(w), f(o)), f(crate::pathing::CELL));
        let Some(u) = self.unit(h) else { return true };
        let top = self.gold_run_speed(h).unwrap_or_else(|| self.current_move_speed(h)) / crate::pathing::CELL;
        let o = self.objects.units.get(u.type_id);
        let turn = o.map_or(0.5, |o| o.real("turnRate"));
        let window = o.map_or(60.0, |o| o.real("propWin")).to_radians();
        let mut gait = crate::motion::Gait { top: f(top), grow: f(top), turn: f(turn), window: f(window), reach: f(0.49) };
        // (A buff run out within the tick changes its speed from the steps after.)
        let ending = self.gold_run_speed(h).is_none() && u.buffs.iter().any(|b| b.move_factor != 0.0 && b.ends_at.is_some());
        let start = if self.step_clocks.is_empty() { self.last_step_clock } else { self.step_base };
        let (cx, cy) = match u.extra.cell_at {
            Some((cx, cy, wx, wy)) if wx == u.x && wy == u.y => (cx, cy),
            _ => (cell(u.x, origin[0]), cell(u.y, origin[1])),
        };
        let mut m = u.extra.mover.unwrap_or(crate::motion::Mover { x: cx, y: cy, heading: u.extra.walk_heading.unwrap_or(f(u.facing.to_radians())), placed_at: start, ..Default::default() });
        let to_structure = u.extra.gold_run.is_some() || u.ai_harvest_walk();
        let goal = self.walk_goal(h);
        if u.extra.mover.is_none() && u.extra.group_walk.is_some() {
            self.group_member_steps(h);
        }
        let Some(u) = self.unit(h) else { return true };
        if u.extra.mover.is_none() {
            log::debug!("walk of {h} from {:.2}, {:.2} facing {:.2} by {:?} to {goal:?}, time {:.3}, tsk. {:?}", u.x, u.y, u.facing, u.path, self.time, u.extra.order_tags.get(7));
            // A goal within reach as it starts ends the walk without its usual objects
            // (`Tags::move_done_in_place`) when it lies near the walker's heading; else it
            // turns to it and arrives as any walk (read: a sheep 0.81 off, its turn 0.5,
            // freed at its first step; sheep 0.915 and 3.0 off and a pig 1.40 off, their
            // trees freed as their turns ended, their paths the step after).  Where the
            // engine parts them between 0.81 and 0.915 is not yet read: taken as 0.86.
            const NEAR_HEADING: f32 = 0.86;
            let behind = goal.is_some_and(|g| {
                let bearing = (g.1 - u.y).atan2(g.0 - u.x);
                let d = (bearing - f32::from_bits(m.heading)).rem_euclid(std::f32::consts::TAU);
                log::trace!("in place? {h}: off {:.4} heading {:.4} distance {:.3}", d.min(std::f32::consts::TAU - d), f32::from_bits(m.heading), (g.0 - u.x).hypot(g.1 - u.y) / crate::pathing::CELL);
                d.min(std::f32::consts::TAU - d) > NEAR_HEADING
            });
            let moves = behind || goal.is_none_or(|g| (g.0 - u.x).hypot(g.1 - u.y) > 0.49 * crate::pathing::CELL);
            if let Some(u) = self.unit_mut(h) {
                u.extra.walk_moved = moves;
            }
        }
        let Some(_) = self.unit(h) else { return true };
        let clocks = self.step_clocks.clone();
        let n = clocks.len();
        let (from, until) = self.step_range;
        let from = if self.unit(h).is_some_and(|u| u.extra.mover.is_none()) { from.max(self.unit_mut(h).and_then(|u| u.extra.walk_from.take()).unwrap_or(0)) } else { from };
        // (One whose velocity a slow held after the step before keeps its place as of
        // then, `Mover::lower_top`.)
        let late = from < until && self.unit_mut(h).is_some_and(|u| std::mem::take(&mut u.extra.placed_late));
        if let Some(&c) = clocks.get(from.wrapping_sub(1)).filter(|_| from > 0 && !late) {
            // The step before was by the clock before it was set back (`World::wrap_clocks`).
            let back = self.step_wraps.get(from - 1).zip(self.step_wraps.get(from)).is_some_and(|(a, b)| a < b);
            m.placed_at = if back { ef::sub(c, crate::motion::WRAP) } else { c };
        }
        if let Some(&c) = clocks.get(from).filter(|_| from < until) {
            log::trace!("walker {h} at {:.6} {:.6} v {:.6} {:.6} hd {:.6} clock {:.6}", f32::from_bits(m.x), f32::from_bits(m.y), f32::from_bits(m.vx), f32::from_bits(m.vy), f32::from_bits(m.heading), f32::from_bits(c));
        }
        let mut done = None;
        let mut faced = None;
        // A goal within reach as the walk starts: its order's objects go at its first
        // step (traced: before the turn's logic), and it turns toward the goal until
        // its heading is on it, the walk then done (read: a sheep 0.81 off its goal's
        // bearing drew its next wait two steps later, as the order ended).
        let in_place = self.unit(h).is_some_and(|u| !u.extra.walk_moved && u.extra.order_tags.len() == 18);
        // One whose first step's turn leaves it within 0.2 of the bearing (done at its
        // second step) frees them as it ends instead (read: a sheep 0.636 off, its turn
        // 0.5, freed at its second step, its end; one 0.813 off at its first, done at
        // its third).  Provisional: the engine's test is not yet read.
        let quick = goal.zip(self.unit(h)).is_some_and(|(g, u)| {
            let d = ((g.1 - u.y).atan2(g.0 - u.x) - f32::from_bits(m.heading)).rem_euclid(std::f32::consts::TAU);
            d.min(std::f32::consts::TAU - d) <= turn + 0.2
        });
        if in_place && !quick && from < until {
            if let Some(slots) = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.order_tags)) {
                self.tags.move_done_in_place(&slots);
            }
        }
        // A new walk whose search found nothing nearer turns toward its goal in its
        // search step and the next, judged facing it from the step after (read: a pig
        // searched at 18.117960, on its goal's bearing at 18.147955 but still walking,
        // done at 18.177950; a walker searching again is judged the step after).
        if self.unit(h).is_some_and(|u| u.extra.mover.is_none() && u.extra.facing_goal) && from < until {
            if let Some(u) = self.unit_mut(h) {
                u.extra.face_hold = 2;
            }
        }
        for (k, &clock) in clocks.iter().enumerate().take(until).skip(from).take(if done.is_some() { 0 } else { usize::MAX }) {
            if ending {
                let top = f(self.move_speed_at(h, self.step_abs(k).map(|s| s.0)) / crate::pathing::CELL);
                (gait.top, gait.grow) = (top, top);
            }
            // A cluster walker stopped at its cluster's point: as this step begins the
            // cluster, its member done, picks its next point (`FUN_6f16c4f0`,
            // `FUN_6f16b120`, `FUN_6f1697a0`) and drops its member's ways (`FUN_6f16d6a0`),
            // which plans and searches anew (read: Mafarazzo TD's creeps, each a step
            // short at its cluster's points, stopped within 0.49 of them).
            if self.unit(h).is_some_and(|u| u.extra.coarse_done) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.coarse_done = false;
                    if let Some(c) = u.extra.coarse.as_mut() {
                        c.index = crate::accelpath::pick(&c.points, c.index, 10.0);
                    }
                    u.extra.move_coarse = None;
                    u.extra.walk_research = true;
                }
            }
            // Its own path's way picks its next point once within 0.49 of its last (in
            // accelerator units; `FUN_6f165f10`), and searches toward it at once.
            if let Some(mc) = self.unit(h).and_then(|u| u.extra.move_coarse.as_ref()).filter(|c| c.index > 0 && self.cluster_walker(h)) {
                let (x, y) = m.at(clock);
                let p = mc.points[mc.index];
                let half = |c: u32| ef::mul(c, 0.5f32.to_bits());
                let (dx, dy) = (ef::sub(p.0.to_bits(), half(x)), ef::sub(p.1.to_bits(), half(y)));
                if f32::from_bits(ef::add(ef::mul(dx, dx), ef::mul(dy, dy))) <= f32::from_bits(ef::mul(0.49f32.to_bits(), 0.49f32.to_bits())) {
                    let index = crate::accelpath::pick(&mc.points, mc.index, 10.0);
                    if let Some(u) = self.unit_mut(h) {
                        if let Some(c) = u.extra.move_coarse.as_mut() {
                            c.index = index;
                        }
                        u.extra.walk_research = true;
                    }
                }
            }
            // (A cluster walker's arrival, holds, and turns are toward its cluster's
            // point, its own path's goal.)
            let target = self.walk_target(h);
            // A dropped way is searched for again first; finding nothing nearer, the
            // walker only turns toward its goal in this step (traced: the first sheep
            // turned 4.2315 to 4.0887 in its search step at 5.37 and ended in turn 56).
            if self.unit(h).is_some_and(|u| u.extra.walk_research) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.walk_research = false;
                    u.path.clear();
                    u.extra.path_end_cell = None;
                }
                if let Some(g) = goal {
                    self.path_to(h, g);
                }
                // Refused its search, it holds toward its goal (read: sheep 1050122,
                // refused three steps, turned toward its goal meanwhile).
                let waits = self.unit(h).is_some_and(|u| u.extra.facing_goal || u.extra.walk_research);
                if let (true, Some(g)) = (waits, self.walk_target(h)) {
                    m.hold(&gait, (cell(g.0, origin[0]), cell(g.1, origin[1])), clock);
                    continue;
                }
            }
            let goal_then = goal;
            let goal = target;
            if let (true, Some(g)) = (self.unit(h).is_some_and(|u| u.extra.facing_goal && u.extra.face_hold > 0), goal) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.face_hold -= 1;
                }
                m.hold(&gait, (cell(g.0, origin[0]), cell(g.1, origin[1])), clock);
                continue;
            }
            let Some(&next) = self.unit(h).and_then(|u| u.path.first()) else {
                // A walk to the mine whose search found its walker beside the mine's
                // footprint (a way of its own cell alone) is in contact from its first
                // step (the provider's 3, `FUN_6f167290`, the walker's +0xd8 0x10000 from
                // `FUN_6f171060`): it turns toward the mine's middle, a turn a step, and
                // is there once its heading is within 0.2 of it as a step begins
                // (`FUN_6f16e910`), going in then (read: Twisted Meadows' night elf
                // computer's first wisp, re-ordered to its mine at 0.1 heading 0.2292,
                // 2.644 off the mine's bearing, on it after the turn at 0.24, in at 0.27;
                // a night elf start's nearest wisp sent by script, 2.29 off, searched at
                // 2.01, in at 2.13).
                if let (true, Some(g)) = (self.unit(h).and_then(|u| u.extra.gold_run.as_ref()).is_some_and(|r| r.phase == crate::goldrun::Phase::ToMine && !r.lumber), goal) {
                    let to = (cell(g.0, origin[0]), cell(g.1, origin[1]));
                    if m.face(&gait, to, clock) == crate::motion::Step::Reached || m.faces(to, clock) {
                        faced = Some(k);
                        break;
                    }
                    continue;
                }
                done = Some(k);
                break;
            };
            let facing = self.unit(h).is_some_and(|u| u.extra.facing_goal);
            let mut to = (cell(next.0, origin[0]), cell(next.1, origin[1]));
            // A waypoint short of the last, within 0.49, gives way to the next target at
            // once, the walker not stopping (`FUN_6f167070`, `FUN_6f165e60`).
            if !facing && self.unit(h).is_some_and(|u| u.path.len() > 1) {
                let (x, y) = m.at(clock);
                let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
                let d2 = f32::from_bits(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
                if d2 <= f32::from_bits(ef::mul(0.49f32.to_bits(), 0.49f32.to_bits())) {
                    let rest = self.unit_mut(h).map(|u| u.path.split_off(1)).unwrap_or_default();
                    let way = self.way_ahead(h, (x, y), rest);
                    let Some(u) = self.unit_mut(h) else { return true };
                    u.path = way;
                    let next = u.path[0];
                    to = (cell(next.0, origin[0]), cell(next.1, origin[1]));
                }
            }
            let counting = self.unit(h).is_some_and(|u| u.extra.walk_wait > 0);
            let held = !facing && self.held_back(h, &m, to, clock);
            let yielding = self.unit(h).is_some_and(|u| u.extra.yields_to.is_some());
            // A new search there gives a new waypoint.
            let Some(u) = self.unit(h) else { return true };
            let facing = u.extra.facing_goal;
            let Some(&next) = u.path.first() else {
                done = Some(k);
                break;
            };
            let to = (cell(next.0, origin[0]), cell(next.1, origin[1]));
            if held {
                // Held while its wait counts down, it turns toward its order's goal (read:
                // a held pig's heading the goal's bearing, 2.0233, its waypoint's 2.886);
                // held otherwise (as it yields, or blocked), toward its waypoint (the
                // provider has put that in its target: a blocked sheep kept its heading).
                let toward = if counting || !yielding { goal.map_or(to, |g| (cell(g.0, origin[0]), cell(g.1, origin[1]))) } else { to };
                m.hold(&gait, toward, clock);
                continue;
            }
            // The last waypoint of a way short of its goal is arrived at facing the goal,
            // as is a structure walked to.
            let last = self.unit(h).is_some_and(|u| u.path.len() == 1);
            // On the last leg of a way short of its goal (its search stopped by its budget
            // at the goal's cell, the way ending at that cell's middle), the walker is there
            // once the goal itself is within reach: it steers for it and its walk ends
            // (`FUN_6f16fbd0` tests arrival, `FUN_6f16e910`, against a point apart from the
            // waypoint it steers by; read: a Gnoll Wood sheep, its search refused 33 passes
            // short with its best node the goal's cell, as the original's, ending its walk
            // a turn before the port's walk to the middle and search again).  Provisional:
            // which point the engine tests, read only so.
            let goal_near = !facing && !to_structure && last && self.unit(h).is_some_and(|u| u.extra.path_end_cell.is_some()) && goal.is_some_and(|g| {
                let (x, y) = m.at(clock);
                let (gx, gy) = (ef::sub(cell(g.0, origin[0]), x), ef::sub(cell(g.1, origin[1]), y));
                f32::from_bits(ef::sqrt(ef::add(ef::mul(gx, gx), ef::mul(gy, gy)))) <= f32::from_bits(gait.reach)
            });
            let to = match goal {
                Some(g) if goal_near => (cell(g.0, origin[0]), cell(g.1, origin[1])),
                _ => to,
            };
            let r = match goal {
                _ if facing => m.face(&gait, to, clock),
                Some(g) if to_structure && last => m.arrive(&gait, to, (cell(g.0, origin[0]), cell(g.1, origin[1])), clock),
                _ => m.step(&gait, to, clock),
            };
            log::trace!("walk of {h} step {k}: {:?} heading {:.4} to {next:?}", r, f32::from_bits(m.heading));
            if r == crate::motion::Step::Moving {
                continue;
            }
            let (x, y) = m.at(clock);
            let cluster = self.cluster_walker(h);
            let Some(u) = self.unit_mut(h) else { return true };
            u.x = f32::from_bits(ef::add(ef::mul(x, crate::pathing::CELL.to_bits()), origin[0].to_bits()));
            u.y = f32::from_bits(ef::add(ef::mul(y, crate::pathing::CELL.to_bits()), origin[1].to_bits()));
            u.path.remove(0);
            if !u.path.is_empty() {
                continue;
            }
            let partial = u.extra.path_end_cell.take().is_some() && !goal_near;
            // A cluster walker at its cluster's point short of its order's goal stops
            // there, its member done (`FUN_6f16e910`'s arrival, `FUN_6f16fbd0`).
            if !partial && cluster && u.extra.coarse.as_ref().is_some_and(|c| c.index > 0) {
                u.extra.coarse_done = true;
                continue;
            }
            match goal_then.filter(|_| partial) {
                Some(goal) => {
                    self.path_to(h, goal);
                }
                None => {
                    done = Some(k);
                    break;
                }
            }
        }
        let last = clocks.get(until.wrapping_sub(1)).copied().filter(|_| until > from).unwrap_or(m.placed_at);
        let (x, y) = m.at(last);
        let Some(u) = self.unit_mut(h) else { return true };
        let was = (u.x, u.y);
        u.x = f32::from_bits(ef::add(ef::mul(x, crate::pathing::CELL.to_bits()), origin[0].to_bits()));
        u.y = f32::from_bits(ef::add(ef::mul(y, crate::pathing::CELL.to_bits()), origin[1].to_bits()));
        u.extra.cell_at = Some((x, y, u.x, u.y));
        if (u.x, u.y) != was {
            u.extra.step_from = Some(was);
        }
        u.facing = f32::from_bits(m.heading).to_degrees();
        // (Turned onto its mine beside it: there as the next step begins, above.)
        if let Some(k) = faced {
            let next = self.step_index + k as u64 + 1;
            log::debug!("walk of {h} faces its mine at step {k} of {n}, there at step {next}");
            if let Some(u) = self.unit_mut(h) {
                u.extra.mover = None;
                u.extra.step_from = None;
                u.path.clear();
                u.extra.arrive_step = Some(next);
                set_anim(u, "stand");
            }
            return false;
        }
        if let Some(k) = done {
            log::debug!("walk of {h} done at step {k} of {n}, time {:.3}, tsk. {:?}", self.time, self.unit(h).and_then(|u| u.extra.order_tags.get(7).copied()));
            // Another walk to the mine done with its way used up ends there too, arrived
            // in the next tick's second step (provisional; an own cell's turns, above).
            if self.unit(h).and_then(|u| u.extra.gold_run.as_ref()).is_some_and(|r| r.phase == crate::goldrun::Phase::ToMine && !r.lumber) {
                let next = self.step_index + n as u64 + 1;
                if let Some(u) = self.unit_mut(h) {
                    u.extra.mover = None;
                    u.extra.step_from = None;
                    u.path.clear();
                    u.extra.arrive_step = Some(next);
                    set_anim(u, "stand");
                }
                return false;
            }
            // An AI's unit's walk with its look queued after looks at once
            // (`World::ai_walk_ended`), and may walk on.
            if self.unit(h).is_some_and(|u| u.extra.ai_idle_ord.is_some()) && self.ai_walk_ended(h) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.mover = None;
                    u.extra.step_from = None;
                    u.path.clear();
                }
                return false;
            }
            // The order's objects go as its walk ends, at its step (traced).
            let moved = self.unit_mut(h).is_some_and(|u| std::mem::take(&mut u.extra.walk_moved));
            let gave_up = self.unit(h).is_some_and(|u| u.extra.facing_goal);
            let slots = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.order_tags)).unwrap_or_default();
            if slots.len() == 14 {
                self.group_member_done(h, &slots, moved);
            } else if slots.len() == 18 || slots.len() == 17 {
                // An in-place walk still holding its objects here (its first step's turn
                // left it on the bearing) frees them as an arrival does: its path as the
                // next step starts (read: a sheep 0.636 off, its path freed the step after
                // its end).
                if !moved {
                    let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
                    self.tags.move_arrived(&slots, serial);
                } else if gave_up {
                    let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
                    self.tags.move_done(&slots, serial);
                } else {
                    let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
                    self.tags.move_arrived(&slots, serial);
                }
            }
            // A wanderer left without an order draws its wait anew, counted from this
            // step (`FUN_6f5af350` on the order's end, event 0xd0148: read, the first
            // sheep's walk ended at 5.399847 and the main generator drew for it there,
            // between its order's draw and the next wanderer's).
            if self.ability_with_code(h, codes::ability::WANDER).is_some() {
                let next = self.rng.real(5.0, 10.0);
                let n = self.tags.start_timer("5aa910");
                log::debug!("wander redraw: {h} at step {k}, clock {:.6}, next {next:.6}", f32::from_bits(clocks.get(k).copied().unwrap_or(0)));
                let at = self.step_agent.get(k).copied().unwrap_or(self.agent_step);
                self.keep_wander_timer(h, n, f32::from_bits(at), next);
                if let Some(u) = self.unit_mut(h) {
                    u.extra.wander_at = Some(crate::efloat::add(at, next.to_bits()));
                }
            }
            self.budgets.leave(h);
            let Some(u) = self.unit_mut(h) else { return true };
            u.extra.mover = None;
            u.extra.step_from = None;
            u.extra.facing_goal = false;
            u.extra.captain_walk = None;
            if u.extra.player_walk {
                u.extra.coarse = None;
                u.extra.move_coarse = None;
                u.extra.coarse_done = false;
            }
            u.path.clear();
            set_anim(u, "stand");
            return true;
        }
        u.extra.mover = Some(m);
        set_anim(u, "walk");
        false
    }

    fn walk_inner(&mut self, h: u32, dt: f32) -> bool {
        self.steer_around(h);
        let Some(u) = self.unit_mut(h) else { return true };
        let Some(&next) = u.path.first() else {
            set_anim(u, "stand");
            return true;
        };
        let want = angle_to((u.x, u.y), next);
        let left = turn(u, want, dt);
        if left.abs() > u.prop_window.max(1.0) {
            return false;
        }
        let step = self.formation_speed(h) * dt;
        let ghost = self.unit(h).is_some_and(|u| self.is_ghost(u));
        let Some(u) = self.unit_mut(h) else { return true };
        let (dx, dy) = (next.0 - u.x, next.1 - u.y);
        let d = (dx * dx + dy * dy).sqrt();
        set_anim(u, "walk");
        // A turn on the way is passed once the unit is within its own size of it: units
        // bound for the same turn would otherwise crowd it, each held off by the others.
        let detour = u.extra.detour == Some(next);
        if u.path.len() > 1 && d <= u.collision * 2.0 && !detour {
            u.path.remove(0);
            return false;
        }
        // Likewise the end of a move to a point, when another unit already stands on it
        // (a worker bound for a mine or a building site goes all the way).
        let to_point = matches!(u.action, Action::Move { .. } | Action::AttackMove { .. } | Action::Patrol { .. });
        // A unit bound for a point is there once within 15.6875 of it, whatever its
        // speed or size (measured: footmen and knights ordered 15.68 away stay put, 15.69
        // away step toward it; ones sent 500 away stop up to 15 short).
        if to_point && u.path.len() == 1 && d <= 15.6875 {
            u.path.clear();
            set_anim(u, "stand");
            return true;
        }
        if to_point && u.path.len() == 1 && d > step && d <= u.collision * 2.0 && !ghost {
            let size = u.collision;
            // (one standing there, not one passing by on its own way)
            let taken = self.unit_order.iter().any(|&o| o != h && self.unit(o).is_some_and(|v| !v.dead && !v.hidden && !v.is_building && v.path.is_empty() && !self.is_ghost(v) && (v.x - next.0).hypot(v.y - next.1) < v.collision + size));
            if taken {
                let Some(u) = self.unit_mut(h) else { return true };
                u.path.clear();
                set_anim(u, "stand");
                return true;
            }
        }
        let Some(u) = self.unit_mut(h) else { return true };
        if d <= step {
            u.x = next.0;
            u.y = next.1;
            u.path.remove(0);
            if u.path.is_empty() {
                set_anim(u, "stand");
                return true;
            }
        } else {
            u.x += dx / d * step;
            u.y += dy / d * step;
        }
        false
    }

    /// Living units in the 512 squares within `reach` of a point, in a fixed order, from
    /// a grid rebuilt once a game moment.
    pub(crate) fn units_near(&self, x: f32, y: f32, reach: f32) -> Vec<u32> {
        if self.unit_grid.borrow().0 != self.time {
            let mut grid: crate::HashMap<(i32, i32), Vec<u32>> = Default::default();
            for &t in &self.unit_order {
                if let Some(tu) = self.unit(t).filter(|tu| !tu.dead) {
                    grid.entry(((tu.x / 512.0).floor() as i32, (tu.y / 512.0).floor() as i32)).or_default().push(t);
                }
            }
            *self.unit_grid.borrow_mut() = (self.time, grid);
        }
        let (cx0, cx1) = (((x - reach) / 512.0).floor() as i32, ((x + reach) / 512.0).floor() as i32);
        let (cy0, cy1) = (((y - reach) / 512.0).floor() as i32, ((y + reach) / 512.0).floor() as i32);
        let grid = self.unit_grid.borrow();
        (cy0..=cy1).flat_map(|cy| (cx0..=cx1).map(move |cx| (cx, cy))).filter_map(|c| grid.1.get(&c)).flatten().copied().collect()
    }

    /// An enemy a unit would acquire now, within its acquisition range.
    pub(crate) fn acquirable_target(&self, h: u32) -> Option<u32> {
        let range = self.unit(h)?.acquire_range;
        self.find_target(h, range)
    }

    /// Whether one of a unit's weapons can attack `t`.
    pub(crate) fn can_attack_unit(&self, h: u32, t: u32) -> bool {
        let (Some(u), Some(tu)) = (self.unit(h), self.unit(t)) else { return false };
        !u.dead && unit_weapons(self, u).iter().any(|(_, wp)| can_target(self, wp, u, tu))
    }

    /// Idle creeps near one that has taken up a target take it up too.
    fn creeps_join(&mut self, h: u32, t: u32) {
        let Some((x, y)) = self.unit(h).map(|u| (u.x, u.y)) else { return };
        let range = self.misc_f32("CreepCallForHelp", 600.0);
        let friends: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&f| f != h)
            .filter(|&f| {
                self.unit(f).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE && !u.dead && !u.hidden && !u.paused && !u.extra.sleeping && matches!(u.action, Action::Idle) && (u.x - x).hypot(u.y - y) <= range)
            })
            .collect();
        for f in friends {
            if self.unit(f).is_some_and(|u| !unit_weapons(self, u).is_empty()) && self.unit(t).is_some_and(|tu| self.unit(f).is_some_and(|u| weapon_for(self, u, tu).is_some())) {
                self.unit_mut(f).unwrap().action = Action::Attack { target: t };
                self.acquired(f, t);
            }
        }
    }

    /// A look for targets that finds none looks again half a second on (CAbilityAttack's
    /// look timer, 0xd01c1, runs 1/2 without a target, DAT_6fd6bf40; measured: the
    /// gnolls of a camp a ghoul walks into take it up at 3.47, 3.66, or 3.97 seconds as
    /// it comes earlier or later, ours as it came within reach).
    fn acquire_miss(&mut self, h: u32, dt: f32) {
        // One passed over only as our fog, refreshed each tenth of a second, has yet to
        // show it is looked for again at once: the original's sight has no such lag.
        if self.fog_passed.get() {
            return;
        }
        self.look_restarted(h, dt, 0.5);
    }

    /// The look's timer started again as it falls due (`FUN_6f493ff0`, before its
    /// search): `period` on, 1/2 without a target (DAT_6fd6bf40), 1 with one
    /// (DAT_6fd6bf44).  (Provisional: not a worker's.)
    pub(crate) fn look_restarted(&mut self, h: u32, dt: f32, period: f32) {
        if let Some(u) = self.unit_mut(h) {
            u.acquire_timer = u.acquire_timer.max(-dt) + period;
        }
        if !self.unit(h).is_some_and(|u| self.unit_class(u, "peon")) {
            let n = self.tags.start_timer("493ff0");
            let now = self.time;
            if let Some(u) = self.unit_mut(h) {
                // (Due its period after the last look's due time, in the engine's
                // arithmetic: the timer fired then.)
                let next = |d: f32| f32::from_bits(crate::efloat::add(d.to_bits(), period.to_bits()));
                let due = match u.extra.look_timer {
                    Some((_, d, _)) if next(d) > now => next(d),
                    _ => now + period,
                };
                u.extra.look_timer = Some((n, due, period));
            }
        }
    }

    /// A structure attacking what it took up looks again as its look's timer falls due,
    /// swinging or not (event 0xd01c1, `FUN_6f493ff0`: the timer started again a
    /// second on, a target held, before the search, `FUN_6f493d20`, whose find it takes
    /// up, `FUN_6f4943f0`).  Measured: the flyer game's Arrow Tower, its looks at 21.662
    /// (taking up a creep), 22.162, and on; the port's had waited for its swings to end.
    fn structure_look(&mut self, h: u32, dt: f32) {
        if !self.attacks_in_place(h) {
            return;
        }
        let Some(u) = self.unit(h) else { return };
        let Action::Attack { target } = u.action else { return };
        if u.extra.acquired != target || u.acquire_timer > 0.0 || u.extra.sleeping || u.extra.reach_walk.is_some() || unit_weapons(self, u).is_empty() || self.unit(target).is_none_or(|t| t.dead) {
            return;
        }
        let (range, creeps) = (u.acquire_range, u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
        // (Its target held in reach scores its rank and more, `FUN_6f49d680`'s 0x1000000
        // over its in-reach 0x4000000, and the search begins with it, a find taking its
        // place only by a better score, `FUN_6f493b20`: so only one of a better kind, by
        // the port's ranks, going for the structure or a unit over a structure.)
        let rank = |w: &World, t: u32| w.unit(t).map(|tu| (!(matches!(tu.action, Action::Attack { target } if target == h) || tu.swing.as_ref().is_some_and(|s| s.target == h)), tu.is_building));
        let held = self.unit_order.contains(&target) && self.find_target_among(h, range, creeps, Some(target));
        let better = self.find_target_ex(h, range, creeps).filter(|&t| t != target && (!held || rank(self, t) < rank(self, target)));
        log::trace!("{h} looks again at {:.4} (timer {:?}) on {target} (held {held}): better {better:?}", self.time, self.unit(h).map(|u| u.acquire_timer));
        if let Some(t) = better {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Attack { target: t };
            }
            self.acquired(h, t);
        }
        self.look_restarted(h, dt, 1.0);
    }

    fn find_target(&self, h: u32, range: f32) -> Option<u32> {
        self.find_target_ex(h, range, true)
    }

    /// The structures' idle looks and reaction waits falling due before `until` (an
    /// agent due, seconds since the game began), taken among the movement steps in the
    /// order they fall due: each is an event on the agent queue (the look's timer,
    /// event 0xd01c1, `FUN_6f493ff0`; the reaction's, 0xd01af), as are the steps, and
    /// each main tick runs that queue's events in turn (`FUN_6f04c1a0`).  Measured: the
    /// upgrade game's Mega Tower of Vengence's look due 47.909035, after the turn's first
    /// step (47.908592), its objects before the next step's region trigger's; its look
    /// due 48.248577 between the turn's second and third.  The rest are the units'
    /// logic's (`World::update_units`).
    pub(crate) fn structure_events_before(&mut self, until: f64) {
        let dt = self.tick_dt;
        // (0 a look, 1 a reaction's end, 2 a first try.)
        let mut due: Vec<(f64, u32, u8)> = Vec::new();
        let begun = self.time_exact - dt as f64 - 1e-6;
        for &h in &self.unit_order {
            if !self.attacks_in_place(h) {
                continue;
            }
            let Some(u) = self.unit(h).filter(|u| !u.dead && !u.paused && u.constructing.is_none() && !u.extra.sleeping) else { continue };
            if let Some(crate::tasks::ReachWalk::React { due: d }) = u.extra.reach_walk {
                if d < until {
                    due.push((d, h, 1));
                }
                continue;
            }
            if let (Action::Attack { .. }, None, Some(f)) = (&u.action, u.swing.as_ref(), u.extra.first_swing_at) {
                if f < until && f > begun {
                    due.push((f, h, 2));
                }
                continue;
            }
            // (Its try as its cooldown runs out, `FUN_6f49a4f0`, an event on the agent queue
            // too: its cooldown as the last tick left it.)
            if matches!(u.action, Action::Attack { .. }) && u.swing.as_ref().is_none_or(|s| s.done) && u.extra.first_swing_at.is_none() && !u.extra.attack_tasks.is_empty() && u.cooldown > 0.0 {
                let f = self.time_exact - dt as f64 + u.cooldown as f64;
                if f < until && f > begun {
                    due.push((f, h, 2));
                }
                continue;
            }
            // (One whose target died looks again as the turn ends, after the death's frees,
            // `World::looks_after_deaths`.)
            if !matches!(u.action, Action::Idle) || u.extra.look_after_death || u.acquire_timer - dt > 0.0 || unit_weapons(self, u).is_empty() {
                continue;
            }
            let Some((_, d, _)) = u.extra.look_timer else { continue };
            if (d as f64) < until {
                due.push((d as f64, h, 0));
            }
        }
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (d, h, kind) in due {
            match kind {
                1 => self.reaction_ended(h, d),
                2 => self.first_try(h, d),
                _ => self.structure_idle_look(h, dt),
            }
        }
    }

    /// The aimed missiles landing in a main tick before `until` (a step's main clock), in
    /// the order they fall due: each lands in the behavior dispatch of the first main
    /// tick at or past its flight's due (`World::bullet_aimed`), after that tick's agent
    /// queue events and before a later tick's step.  Traced: the upgrade game's h00X's
    /// shell due 51.116651, landing in the main tick of 51.120121, its five deaths
    /// between the turn's second step's region trigger and its third step's; the port's
    /// had waited for the units' logic after the steps.
    pub(crate) fn aimed_landings_before(&mut self, vm: &mut Vm, until: f64) {
        let ticks: Vec<f64> = (0..self.tick_clocks.1.len()).filter_map(|i| self.tick_abs(i)).collect();
        let mut due: Vec<(f64, usize)> = Vec::new();
        for (i, m) in self.missiles.iter().enumerate() {
            let Some(d) = m.bullet.as_ref().and_then(|b| b.due) else { continue };
            if m.stampede.is_some() || m.rock.is_some() || (self.unit(m.target).is_none() && m.impact.is_none()) {
                continue;
            }
            if ticks.iter().copied().find(|&c| c >= d - 1e-7).is_some_and(|c| c < until - 1e-7) {
                due.push((d, i));
            }
        }
        if due.is_empty() {
            return;
        }
        let mut order = due.clone();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut taken: Vec<usize> = due.iter().map(|&(_, i)| i).collect();
        taken.sort_unstable_by(|a, b| b.cmp(a));
        let mut by_index: std::collections::HashMap<usize, Missile> = Default::default();
        for i in taken {
            by_index.insert(i, self.missiles.remove(i));
        }
        let mut hits: Vec<Missile> = order.into_iter().filter_map(|(_, i)| by_index.remove(&i)).collect();
        // (Each as of its landing's main tick, as `World::fly_bullets` has it.)
        let off = ticks.last().map_or(0.0, |c| c - self.time_exact);
        for m in hits.iter_mut() {
            if let Some(b) = m.bullet.as_mut() {
                let d = b.due.unwrap_or(0.0);
                b.at = ticks.iter().copied().find(|&c| c >= d - 1e-7).unwrap_or(d) - off + 2e-7;
            }
        }
        self.missiles_land(vm, hits);
    }

    /// A structure's first try on a target it took up, as it falls due among the steps
    /// (`FUN_6f49d130`'s timer, `FUN_6f495180`), or its try as its cooldown runs out
    /// (`FUN_6f49a4f0`): on one out of reach it gives up then (`World::try_given_up`);
    /// else its attack begins with the units' logic.  Traced: the second flyer game's
    /// Arrow Tower, its take-up at turn 330 given up early in 331, its creep past its
    /// reach, the nine tasks' slots taken by the next step's; the third flyer game's,
    /// its attack at turn 336 given up at 344's start, its slots then the next order's.
    fn first_try(&mut self, h: u32, at: f64) {
        let Some((u, t)) = self.unit(h).zip(self.unit(h).and_then(|u| match u.action { Action::Attack { target } => self.unit(target).map(|t| (target, t)), _ => None })) else { return };
        let (target, t) = t;
        if t.dead {
            return;
        }
        let Some((_, wp)) = weapon_for(self, u, t) else { return };
        let reach = wp.range + self.upgrade_bonus(u.owner, u.type_id).range;
        let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - self.reach_radius(target);
        if self.reaches_walker(h, target, reach, at).unwrap_or(d <= reach) {
            return;
        }
        self.try_given_up(h, at);
    }

    /// An idle structure's look (as the units' logic's, `World::update_units`), its timer
    /// counted down for the tick first, as the units' logic will find it.
    fn structure_idle_look(&mut self, h: u32, dt: f32) {
        let before = self.unit(h).and_then(|u| u.extra.look_timer).map(|l| l.1);
        if let Some(u) = self.unit_mut(h) {
            u.acquire_timer -= dt;
        }
        let Some(u) = self.unit(h) else { return };
        let (range, creeps) = (u.acquire_range, u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
        if let Some(t) = self.find_target_ex(h, range, creeps) {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Attack { target: t };
            }
            self.acquired(h, t);
            self.spell_ai(h, t, h);
            self.look_restarted(h, dt, 0.5);
            if creeps {
                self.creeps_join(h, t);
            }
        } else {
            self.acquire_miss(h, dt);
        }
        // (Its timer from its look's new due, as the units' logic will count it down; a
        // look carried from the last tick, `World::update_units`, ran later than its due.)
        let now = self.time_exact;
        if let Some(u) = self.unit_mut(h) {
            match u.extra.look_timer.map(|l| l.1).filter(|&d| Some(d) != before) {
                Some(d) => u.acquire_timer = (d as f64 - now) as f32 + dt,
                None => u.acquire_timer += dt,
            }
        }
    }

    /// Whether the search would find `only` (`find_target_ex`'s tests on it alone).
    fn find_target_among(&self, h: u32, range: f32, creeps: bool, only: Option<u32>) -> bool {
        self.find_target_inner(h, range, creeps, only).is_some()
    }

    /// `creeps`: whether Neutral Hostile's units may be taken up (an idle unit leaves
    /// them be).
    pub(crate) fn find_target_ex(&self, h: u32, range: f32, creeps: bool) -> Option<u32> {
        if self.profile.is_none() {
            return self.find_target_inner(h, range, creeps, None);
        }
        let t0 = std::time::Instant::now();
        let r = self.find_target_inner(h, range, creeps, None);
        self.prof_find.set(self.prof_find.get() + t0.elapsed().as_secs_f64());
        r
    }

    fn find_target_inner(&self, h: u32, range: f32, creeps: bool, only: Option<u32>) -> Option<u32> {
        // An invisible unit takes up no targets, nor one fading out (measured: a
        // Blademaster 0.5 seconds into Wind Walk's 0.6 transition leaves a footman shown
        // beside it be until ordered to attack).
        if self.has_buff_flag(h, |b| b.invisible) {
            return None;
        }
        // Nor does a hidden one (measured: a Blademaster hidden by Mirror Image strikes
        // nothing until it shows).
        let u = self.unit(h).filter(|u| !u.hidden)?;
        self.fog_passed.set(false);
        let mut best: Option<(f32, u32, bool, bool)> = None;
        // Only the squares within reach (the largest structure's half-width and a
        // moment's walk to spare).
        let near: Vec<u32> = self.units_near(u.x, u.y, range + u.collision + 448.0).into_iter().filter(|&t| t != h && self.unit(t).is_some_and(|tu| !is_ally(self, u.owner, tu.owner))).collect();
        if near.is_empty() {
            return None;
        }
        let weapons = unit_weapons(self, u);
        if weapons.is_empty() {
            return None;
        }
        let in_place = self.attacks_in_place(h);
        // (One that can move all the same takes up what is out of its reach,
        // `World::moves_to_attack`.)
        let reaches = in_place && !self.moves_to_attack(h);
        let reach_bonus = self.upgrade_bonus(u.owner, u.type_id).range;
        // (An idle structure's look: its due, and the main clock of the last step before
        // it, this tick's or the last.)
        let look: Option<(u32, f64)> = (in_place && matches!(u.action, Action::Idle | Action::Hold | Action::Attack { .. })).then_some(()).and(u.extra.look_timer).map(|l| {
            let due = l.1 as f64;
            let k = (0..self.step_clocks.len()).rev().find(|&k| self.step_abs(k).is_some_and(|(_, a)| a < due));
            (k.map_or(self.step_base, |k| self.step_clocks[k]), due)
        });
        for t in near.into_iter().filter(|&t| only.is_none_or(|o| o == t)) {
            let Some(tu) = self.unit(t) else { continue };
            // Out of range first: the other tests cost more.  The range runs edge to
            // edge (measured: a footman, acquire 500, takes up a grunt 550 away center
            // to center, the two collisions 63 between, but not one 575 away).
            let edges = u.collision + self.reach_radius(t);
            if (tu.x - u.x).abs() > range + edges || (tu.y - u.y).abs() > range + edges {
                continue;
            }
            if is_ally(self, u.owner, tu.owner) || !weapons.iter().any(|(_, wp)| can_target(self, wp, u, tu)) {
                continue;
            }
            // A creep only once it goes for this unit.
            if !creeps && tu.owner == PLAYER_NEUTRAL_AGGRESSIVE && !(matches!(tu.action, Action::Attack { target } if target == h) || tu.swing.as_ref().is_some_and(|s| s.target == h)) {
                continue;
            }
            // Neither are invisible nor ethereal ones (Neutral Passive is passive toward
            // every player from the start).
            if (self.is_invisible(t) && !self.detected_by(t, u.owner)) || self.is_ethereal(t) {
                continue;
            }
            // Nor a sleeping one (measured: a Dreadlord leaves the sorceress it put to
            // sleep alone).
            if tu.extra.sleeping || tu.buffs.iter().any(|b| b.sleep) {
                continue;
            }
            let mut d = (tu.x - u.x).hypot(tu.y - u.y) - edges;
            // A structure's look finds a walker by its place as last set, at the last
            // step before the look (the search's test, `FUN_6f05ce60`: the position's
            // +0x78 and +0x7c, not carried on, within the radius and the searcher's
            // collision, `FUN_6f059c10`, `FUN_6f05aa20`, and the walker's, each at
            // least 1), then by its reach with both carried on to the look's moment
            // (`FUN_6f4959f0`, `World::reaches_walker`).  Read: the Plague Tower's look
            // due 11.3057 not finding the fort runner, its place at its step at 11.279
            // 1075.5 off against 1000, 72, and 1, though carried on to 11.3057 it was
            // 1068.2 off; the look due 11.8057 took it up.
            let walker = look.zip(tu.extra.mover.filter(|_| tu.steps_walker()));
            if let Some(((clock, due), m)) = walker {
                let (px, py) = m.at(clock);
                let o = self.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
                let (px, py) = (f32::from_bits(px) * crate::pathing::CELL + o[0], f32::from_bits(py) * crate::pathing::CELL + o[1]);
                d = (px - u.x).hypot(py - u.y) - u.collision.max(1.0) - self.reach_radius(t).max(1.0);
                if reaches && !weapons.iter().any(|(_, wp)| can_target(self, wp, u, tu) && self.reaches_walker(h, t, wp.range + reach_bonus, due) != Some(false)) {
                    continue;
                }
            }
            // One that cannot walk takes up only what it reaches (`FUN_6f4959f0`: the
            // weapon's range for a unit `FUN_6f66f030` finds no move for).
            if reaches && walker.is_none() && !weapons.iter().any(|(_, wp)| can_target(self, wp, u, tu) && d <= wp.range + reach_bonus) {
                continue;
            }
            // Only what the owner can see (a cliff above, the fog) is acquired.
            if d <= range && self.fog_state(tu.x, tu.y, u.owner) != crate::fog::FOG_VISIBLE {
                self.fog_passed.set(true);
                continue;
            }
            // Then units before structures, then the nearest (measured: a footman takes a
            // footman 300 away over a farm 170 away, and a peasant 150 away over a
            // footman 250 away).
            // One going for the unit before the rest (measured: a Rifleman takes an archer
            // 500 away that has taken it up over a paused footman 150 away, but the
            // footman when the archer is paused too).
            let at_me = matches!(tu.action, Action::Attack { target } if target == h) || tu.swing.as_ref().is_some_and(|s| s.target == h);
            let key = (!at_me, tu.is_building, d);
            if d <= range && best.is_none_or(|b| key < (b.3, b.2, b.0)) {
                best = Some((d, t, tu.is_building, !at_me));
            }
        }
        best.map(|b| b.1)
    }

    /// whosyourdaddy (CUnit vf72): a computer's unit does a user's none, a user's unit a
    /// computer's 100 times its damage.
    fn god_damage(&self, source: u32, target: u32, dmg: f32) -> f32 {
        if !self.cheat(crate::cheats::flag::GOD) {
            return dmg;
        }
        let computer = |h: u32| self.unit(h).and_then(|u| self.players.get(u.owner as usize)).is_some_and(|p| p.controller != crate::types::map_control::USER);
        let (Some(_), Some(_)) = (self.unit(source), self.unit(target)) else { return dmg };
        match (computer(source), computer(target)) {
            (false, true) => dmg * 100.0,
            (true, false) => 0.0,
            _ => dmg,
        }
    }

    pub fn deal_damage_pub(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, attack_type: &str) {
        self.deal_damage(vm, source, target, raw, attack_type);
    }

    fn deal_damage(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, attack_type: &str) {
        let Some(t) = self.unit(target) else { return };
        if t.dead || self.unit_invulnerable(t) || self.buff_mods(target).5 {
            return;
        }
        // Ethereal units take EtherealDamageBonus's factor: none from physical attacks,
        // 1.66 from magic ones.
        let ethereal = self.ethereal_factor(source, target, attack_type);
        if ethereal <= 0.0 {
            return;
        }
        if self.evades(target) {
            self.show_miss(source);
            return;
        }
        // Curse: the attacker misses some of its attacks.
        let miss = self.unit(source).map_or(0.0, |u| u.buffs.iter().map(|b| b.miss_chance).fold(0.0, f32::max));
        if miss > 0.0 && attack_type != "spells" && (self.rng.int(1, 100) as f32) <= miss * 100.0 {
            return;
        }
        let now = self.time;
        if let Some(t) = self.unit_mut(target) {
            t.extra.last_attacked = now;
        }
        // An enemy's damage may warn the target's owner and allies.
        let owners = (self.unit(source).map(|u| u.owner), self.unit(target).map_or(0, |u| u.owner));
        if let (Some(from), to) = owners {
            if from != to && !crate::natives::players::is_ally(self, to, from) {
                self.requests.push(Request::Attacked { unit: target });
            }
        }
        // Berserk: the target takes more damage; Defend: less from piercing attacks.
        let mut taken = 1.0 + self.unit(target).map_or(0.0, |u| u.buffs.iter().map(|b| b.damage_taken).sum());
        if attack_type == "pierce" {
            if let Some(p) = self.unit(target).and_then(|u| u.buffs.iter().find(|b| b.pierce_taken > 0.0)).map(|b| b.pierce_taken) {
                taken *= p;
            }
        }
        let raw = raw * taken * ethereal;
        let mut raw = self.on_attack_passives(source, target, raw * self.buff_mods(source).3) + self.buff_damage_bonus(source);
        // Wind Walk: the first strike adds its backstab damage and ends the invisibility,
        // once it has faded in (measured: a Blademaster's hit 0.36 seconds into its 0.6
        // transition does neither; one at 1.54 deals the 40 more and ends it).
        if self.cloaked(source) {
            if let Some(u) = self.unit_mut(source) {
                if let Some(b) = u.buffs.iter().find(|b| b.backstab > 0.0) {
                    raw += b.backstab;
                }
                u.buffs.retain(|b| !b.invisible);
            }
        }
        let Some(t) = self.unit(target) else { return };
        let dmg = final_damage(self, raw, attack_type, t);
        let melee_hit = self.unit(source).and_then(|u| u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| weapon(self, u.type_id))).is_some_and(|w| w.range <= self.misc.get_str("InfoPanel", "MeleeRangeMax").and_then(|v| v.trim().parse().ok()).unwrap_or(128.0));
        let dmg = if attack_type == "spells" { dmg } else { self.hardened_skin(target, melee_hit, dmg) };
        let dmg = self.elunes_grace(target, &attack_type.to_ascii_lowercase(), dmg);
        let dmg = self.illusion_damage(source, target, dmg);
        let dmg = self.god_damage(source, target, dmg);
        // An attack shows the attacker to its victim's owner; the original passes the
        // weapon's cooldown plus one as the radius, so FoggedAttackRevealRadius rules
        // (FUN_6f4946b0, FUN_6f4988d0).
        let cooldown = self.unit(source).and_then(|u| u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| weapon(self, u.type_id))).map_or(0.0, |w| w.cooldown);
        if attack_type != "spells" {
            self.reveal_attacker(source, target, cooldown + 1.0);
        }
        let Some(t) = self.unit(target) else { return };
        let owner = t.owner;
        // The attacker's damage hooks are given the damage, at most the life the target
        // had: they run before the hit takes the life (`FUN_6f69ca80`, then CUnit vf74).
        let took = dmg.min(t.life);
        self.wake(target);
        let linked_dead = self.take_life(target, dmg);
        let dead = self.unit(target).is_some_and(|t| t.life <= 0.405);
        if !dead {
            self.phase_shift(target);
        }
        // A locust counts what it deals toward going back to its caster.
        if let Some(s) = self.unit_mut(source).and_then(|u| u.extra.swarm.as_mut()) {
            s.1 += dmg;
        }
        for d in linked_dead {
            kill_unit(self, vm, d, source);
        }
        // Vampiric Aura and the Mask of Death: the attacker regains a share of the damage.
        let steal: f32 = self.unit(source).map_or(0.0, |u| u.buffs.iter().map(|b| b.life_steal).sum::<f32>() + if attack_type == "spells" { 0.0 } else { u.item_bonus.life_steal });
        if steal > 0.0 {
            if let Some(u) = self.unit_mut(source) {
                u.life = (u.life + dmg * steal).min(u.max_life);
            }
        }
        // Thorns: melee attackers of a thorned unit take damage back.
        let melee = self.unit(source).and_then(|u| u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| weapon(self, u.type_id))).is_some_and(|w| w.range <= self.misc.get_str("InfoPanel", "MeleeRangeMax").and_then(|v| v.trim().parse().ok()).unwrap_or(128.0));
        if melee {
            let thorns: Vec<(f32, bool)> = self.unit(target).map(|u| u.buffs.iter().filter(|b| b.return_damage > 0.0).map(|b| (b.return_damage, b.return_pct)).collect()).unwrap_or_default();
            for (v, pct) in thorns {
                let back = if pct { dmg * v } else { v };
                if let Some(u) = self.unit_mut(source) {
                    u.life -= back;
                }
            }
            let spikes = self.spiked_barricades(target);
            if spikes > 0.0 {
                if let Some(u) = self.unit_mut(source) {
                    u.life -= spikes;
                }
            }
        }
        let ctx = EventCtx { unit: target, other_unit: source, damage: dmg, event_id: EVENT_UNIT_DAMAGED, player: owner, has_player: true, ..Default::default() };
        self.fire(vm, ctx, |_, k| matches!(k, EventKind::Unit { unit, event } if *unit == target && *event == EVENT_UNIT_DAMAGED));
        if attack_type != "spells" {
            self.pillage(source, target, took);
        }
        if dead {
            kill_unit(self, vm, target, source);
        } else if self.unit(target).is_some() {
            self.call_for_help(target, source);
            self.ai_town_attacked(target, source);
            // A computer's or creep's unit hurt by an enemy weighs its spells.
            if self.unit(source).is_some_and(|s| self.unit(target).is_some_and(|t| !is_ally(self, t.owner, s.owner))) {
                self.spell_ai_hurt(vm, target, source, attack_type != "spells");
                self.spell_ai(target, source, target);
            }
            if !self.ai_flees(target, source) {
                self.retaliate(target, source, attack_type == "spells");
            }
        }
    }

    /// Advances every unit by `dt` seconds.
    pub fn update_units(&mut self, vm: &mut Vm, dt: f32) {
        let units = self.unit_order.clone();
        'units: for h in units {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                // UnitSuspendDecay holds the corpse's clock.
                if u.extra.suspend_decay {
                    self.unit_mut(h).unwrap().death_time += dt;
                    continue;
                }
                // A body lasts its death, then: a structure's StructureDecayTime; a
                // decaying corpse's DecayTime and BoneDecayTime; nothing for one that
                // leaves no corpse (measured: a footman, death 3.04, is removed 93.05
                // seconds after it dies, a grunt, 1.75, after 91.80, a farm, 2.34, after
                // 32.35, and a Flying Machine, 2.67, after 2.70).
                let (death, decays) = self.objects.units.get(u.type_id).map_or((0.0, false), |o| (o.real("death"), o.int("deathType") & 2 != 0));
                // (An exploded structure too lies its death and StructureDecayTime: traced,
                // Mafarazzo TD's Freeze Tower exploded by its Sell trigger, its body's
                // objects freed 32.4 seconds on, its death 2.34.)
                let decay = if u.extra.exploded && !u.is_building {
                    0.0
                } else if u.is_building {
                    death + self.misc_f32("StructureDecayTime", 30.0)
                } else if decays {
                    death + self.misc_f32("DecayTime", 2.0) + self.misc_f32("BoneDecayTime", 88.0)
                } else {
                    death
                };
                // iocainepowder.
                let decay = if self.cheat(crate::cheats::flag::FAST_DEATH) { decay / crate::cheats::FAST } else { decay };
                let t = self.time - u.death_time;
                // Once its death has played (UnitData death) a corpse of a decaying kind
                // (deathType 2 or 3) decays: EVENT_PLAYER_UNIT_DECAY (FUN_6f69c000).
                // (Not one reincarnating, which lies as it fell until it rises.)
                if !u.extra.decaying && !u.extra.exploded && !u.hidden && u.reincarnate.is_none() {
                    let o = self.objects.units.get(u.type_id);
                    if o.is_some_and(|o| o.int("deathType") & 2 != 0) && t >= o.map_or(0.0, |o| o.real("death")) {
                        self.unit_mut(h).unwrap().extra.decaying = true;
                        self.queue_unit_event(h, EVENT_PLAYER_UNIT_DECAY, EVENT_UNIT_DECAY, EventCtx::default());
                    }
                }
                let Some(u) = self.unit(h) else { continue };
                // Dead heroes stay, out of sight, to be revived: they dissipate once their
                // death has played (measured: a Paladin, death 1.5, is hidden 4.55 seconds
                // after dying, a Blademaster, 1.9, after 4.95).
                // One reincarnating stays in sight (measured: a Tauren Chieftain lies shown
                // the 7 seconds until it rises).
                if u.is_hero {
                    let gone = if u.extra.exploded { 0.0 } else { death + self.misc_f32("DissipateTime", 3.0) };
                    if t > gone && !u.hidden && u.reincarnate.is_none() {
                        self.unit_mut(h).unwrap().hidden = true;
                    }
                } else if t > decay && u.reincarnate.is_none() {
                    self.body_objects_go(h);
                    crate::natives::units::remove_unit(self, h);
                } else if t + dt > death && u.extra.kill_tasks.len() == 4 {
                    // A structure KillUnit killed lets its kill's fifth and fourth task go
                    // as its death has played, in the tick its due falls in (traced: the
                    // Freeze Tower's, its death 2.34, in the 23rd turn after its kill's).
                    let k = std::mem::take(&mut self.unit_mut(h).unwrap().extra.kill_tasks);
                    self.tags.doom(k[3]);
                    self.tags.doom(k[2]);
                    self.unit_mut(h).unwrap().extra.kill_tasks = k[..2].to_vec();
                }
                continue;
            }
            if u.paused {
                continue;
            }
            if u.is_building {
                self.update_structure(vm, h, dt);
                // An armed structure, once built, fights as a unit does, from where it
                // stands (measured: a Guard Tower made beside enemy units strikes the
                // nearest every 0.9 seconds, its cooldown; ours struck nothing).
                let armed = self.unit(h).is_some_and(|u| u.constructing.is_none() && !u.dead && !unit_weapons(self, u).is_empty());
                if !armed {
                    continue;
                }
            }
            let Some(u) = self.unit(h) else { continue };
            // Stunned units do nothing; an attack in progress is lost.
            if self.buff_mods(h).4 {
                let u = self.unit_mut(h).unwrap();
                u.swing = None;
                u.casting = None;
                continue;
            }
            // An idle unit takes its next queued order.
            if matches!(u.action, Action::Idle) && !u.extra.order_queue.is_empty() {
                let next = self.unit_mut(h).unwrap().extra.order_queue.pop_front();
                match next {
                    Some((crate::aiposts::AI_IDLE_ORDER, ..)) => self.ai_unit_idle(h),
                    Some((order, x, y, target)) => self.order_unit(h, order, x, y, target),
                    None => {}
                }
            }
            let Some(u) = self.unit(h) else { continue };
            let action = u.action.clone();
            let wp = u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| unit_weapons(self, u).into_iter().next().map(|x| x.1));
            let speed = (u.cooldown > 0.0 && u.extra.cooldown_speed.is_some()).then(|| self.attack_speed(h));
            {
                let u = self.unit_mut(h).unwrap();
                // A change in attack speed scales what is left of the cooldown
                // (FUN_6f495ef0; measured: a grunt's first attack under an Endurance
                // Aura taking hold during it is quickened).
                if let (Some(new), Some(old)) = (speed, u.extra.cooldown_speed) {
                    if new != old {
                        u.cooldown *= old / new;
                        u.extra.cooldown_speed = Some(new);
                    }
                }
                // What runs past zero within a step carries into the next attack, so a
                // unit attacking steadily strikes exactly its cooldown apart, as the
                // original's timers do (measured: 1.60 for a grunt, 1.35 for a footman).
                // Only the step it runs out in carries over: a ready unit that waits
                // carries nothing into its next swing.
                u.cooldown = if u.cooldown <= 0.0 { 0.0 } else { (u.cooldown - dt).max(-dt) };
                // The shown facing follows at a third of the turn rate.
                let shown = u.shown_facing.unwrap_or(u.facing);
                let d = angle_diff(shown, u.facing);
                let step = (u.turn_speed.max(0.05) * 10.0).to_degrees() * dt;
                u.shown_facing = Some(if d.abs() <= step { u.facing } else { (shown + step * d.signum()).rem_euclid(360.0) });
                if let Some(z) = u.extra.shown_height {
                    let (d, step) = (u.fly_height - z, u.extra.height_rate * dt);
                    u.extra.shown_height = (d.abs() > step).then(|| z + step * d.signum());
                }
                u.repath_timer -= dt;
                u.acquire_timer -= dt;
            }
            self.structure_look(h, dt);
            // Attack swing in progress.
            if let Some(mut s) = self.unit(h).and_then(|u| u.swing.clone()) {
                let known = wp.is_some();
                let wp = wp.clone().unwrap_or_default();
                // A swing plays at the unit's attack speed (measured: a Blademaster of
                // agility 32, speed 1.64, strikes 0.20 seconds after its swing begins, its
                // damage point 0.33).
                let speed = self.attack_speed(h);
                s.elapsed += dt * speed;
                let (quick, point) = swing_point(&wp, known, speed);
                let mut swings = 0;
                // (A structure's damage point, an event on the agent queue, due after the
                // queue's reach this tick comes in the next, among its steps: traced, the
                // upgrade game's second h00X's first shell, due 50.99565, in turn 511.)
                let reach = self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i)).filter(|_| !self.step_clocks.is_empty() && self.attacks_in_place(h));
                let now = self.time_exact;
                let late = |s: &Swing| reach.is_some_and(|r| !s.landed && now - ((s.elapsed - point).max(0.0) / speed) as f64 >= r);
                loop {
                    // (One taken among the steps lands no more, `World::points_before`.)
                    if !s.done && (s.landed || s.elapsed >= point - STEP_SLACK) && !late(&s) {
                        s.done = true;
                        // (The damage point's moment: as far back in the tick as the
                        // swing has run past it.)
                        let at = self.time_exact - ((s.elapsed - point).max(0.0) / speed) as f64;
                        if !s.landed && !self.swing_lands(vm, h, &s, &wp, at) {
                            continue 'units;
                        }
                        if quick {
                            let past = (s.elapsed - point).max(0.0) / speed;
                            if let Some(u) = self.unit_mut(h) {
                                u.cooldown = u.cooldown.max(COOLDOWN_AFTER_POINT - past);
                            }
                        }
                    }
                    if !quick || !s.done || swings >= QUICK_SWINGS_PER_STEP {
                        break;
                    }
                    let Some(next) = self.next_quick_swing(h, &s, speed, dt) else { break };
                    s = next;
                    swings += 1;
                }
                let Some(u) = self.unit_mut(h) else { continue };
                // The backswing gives way to the next attack once the cooldown is up
                // (measured: a level 6 Blademaster, agility 32, strikes every 1.08 seconds,
                // its cooldown 1.77 / 1.64, though its damage point and backswing make
                // 1.17).
                if (!quick && s.elapsed >= wp.damage_point + wp.backswing - STEP_SLACK) || (s.done && u.cooldown <= STEP_SLACK) {
                    u.swing = None;
                    set_anim(u, "stand");
                } else {
                    u.swing = Some(s);
                }
                continue;
            }
            match action {
                Action::Cast { ability, target, x, y } => self.cast_step(vm, h, ability, target, x, y, dt),
                Action::PickUp { item } => {
                    // Within PickupItemRange of a ground item, take it.
                    let Some(pos) = self.item(item).filter(|i| i.owner_unit == 0 && i.visible).map(|i| (i.x, i.y)) else {
                        self.unit_mut(h).unwrap().action = Action::Idle;
                        continue;
                    };
                    let range = self.misc_f32("PickupItemRange", 150.0);
                    let u = self.unit(h).unwrap();
                    if (u.x - pos.0).hypot(u.y - pos.1) > range {
                        if u.path.is_empty() || u.repath_timer <= 0.0 {
                            self.path_to(h, pos);
                        }
                        self.walk(h, dt);
                        continue;
                    }
                    self.unit_mut(h).unwrap().path.clear();
                    self.give_item(vm, h, item, None);
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        set_anim(u, "stand");
                    }
                }
                // Handing an item over: within GiveItemRange of the target (FUN_6f54efe0),
                // into its inventory (or onto the ground beside it when it has no room).
                Action::GiveItem { item, target } => {
                    // A shop that buys items (Purchase Item, Apit) takes it for gold within
                    // PawnItemRange (CAbilityPurchaseItem).
                    let pawn = self.ability_with_code(target, codes::ability::PURCHASE_ITEM).is_some() && self.unit(target).is_some_and(|t| t.owner == self.unit(h).map_or(0, |u| u.owner) || t.owner >= crate::types::PLAYER_NEUTRAL_AGGRESSIVE);
                    let range = if pawn { self.misc_f32("PawnItemRange", 300.0) } else { self.misc_f32("GiveItemRange", 150.0) };
                    let Some((tx, ty)) = self.unit(target).filter(|t| !t.dead).map(|t| (t.x, t.y)) else {
                        self.unit_mut(h).unwrap().action = Action::Idle;
                        continue;
                    };
                    let u = self.unit(h).unwrap();
                    if (u.x - tx).hypot(u.y - ty) > range + u.collision {
                        if u.path.is_empty() || u.repath_timer <= 0.0 {
                            self.path_to(h, (tx, ty));
                        }
                        self.walk(h, dt);
                        continue;
                    }
                    if pawn && self.unit(h).is_some_and(|u| u.inventory.contains(&item)) {
                        self.pawn_item(vm, h, item, target);
                    } else if self.unit(h).is_some_and(|u| u.inventory.contains(&item)) {
                        self.drop_item(vm, item, tx, ty);
                        if !self.give_item(vm, target, item, None) {
                            // Left where it fell.
                        }
                    }
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        set_anim(u, "stand");
                    }
                }
                Action::DropItem { item, x, y } => {
                    let range = self.misc_f32("DropItemRange", 100.0);
                    let u = self.unit(h).unwrap();
                    if (u.x - x).hypot(u.y - y) > range {
                        if u.path.is_empty() || u.repath_timer <= 0.0 {
                            self.path_to(h, (x, y));
                        }
                        self.walk(h, dt);
                        continue;
                    }
                    self.drop_item(vm, item, x, y);
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        set_anim(u, "stand");
                    }
                }
                Action::Idle | Action::Hold => {
                    // (One walking to its captain keeps its way: the steps walk it.)
                    if let Some(u) = self.unit_mut(h).filter(|u| u.extra.captain_walk.is_none()) {
                        if !u.path.is_empty() {
                            u.path.clear();
                        }
                        if u.anim == "walk" {
                            set_anim(u, "stand");
                        }
                    }
                    // An idle unit takes up what comes in reach at once (measured: idle
                    // footmen beside a target made attackable strike 0.505 seconds later,
                    // their damage point; ours, looking every ReactionDelay, at 0.581).
                    // (A structure's look falls due among the steps, the agent queue's
                    // events, `World::structure_events_before`; one due after the queue's
                    // reach this tick comes in the next, before its first step: traced, the
                    // upgrade game's tower taking up a creep at turn 507's start.)
                    let carried = matches!(action, Action::Idle) && !self.step_clocks.is_empty() && self.attacks_in_place(h) && self.unit(h).is_some_and(|u| u.extra.look_timer.is_some());
                    if wp.is_some() && !carried && self.unit(h).is_some_and(|u| u.acquire_timer <= 0.0 && !u.extra.sleeping) {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        let range = if matches!(action, Action::Hold) { wp.as_ref().map_or(0.0, |w| w.range) } else { range };
                        // A creep is left be unless it goes for the unit: only another creep
                        // takes one up (measured: a user's idle footman 300 from a gnoll camp
                        // leaves the gnolls be, fighting or not, and attacks an enemy player's
                        // footman among them).
                        let creeps = self.unit(h).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
                        if let Some(t) = self.find_target_ex(h, range, creeps) {
                            // A target taken up on its own leaves the unit's order as it
                            // was (measured: GetUnitCurrentOrder reads none for a footman
                            // or Crypt Fiend attacking what it acquired, near or far).
                            if let Some(u) = self.unit_mut(h) {
                                u.action = if matches!(action, Action::Hold) { Action::Hold } else { Action::Attack { target: t } };
                            }
                            self.acquired(h, t);
                            self.spell_ai(h, t, h);
                            // A structure's look starts its timer again as it falls due,
                            // before its search, so half a second on without a target then
                            // (`FUN_6f493ff0`; measured: the flyer game's Arrow Tower taking
                            // up a creep at its look at 21.662 and looking again at 22.162).
                            if self.attacks_in_place(h) {
                                self.look_restarted(h, dt, 0.5);
                            }
                            // A creep that takes one up brings its idle friends within
                            // CreepCallForHelp with it (measured: a camp's three gnolls take
                            // up a ghoul walking in at the same moment, 220, 241, and 341
                            // away, their reach 200).
                            if creeps {
                                self.creeps_join(h, t);
                            }
                            if matches!(action, Action::Hold) {
                                self.try_attack(h, t, dt, true);
                            }
                        } else {
                            self.acquire_miss(h, dt);
                        }
                    }
                }
                // A wanderer walks step by step with the others (`World::step_walkers`).
                Action::Move { .. } if self.unit(h).is_some_and(|u| u.extra.local_paths || u.extra.player_walk) => {}
                // So does an AI's unit walking back to its post (`World::ai_walk_ended`).
                Action::AttackMove { .. } if self.unit(h).is_some_and(|u| u.extra.ai_idle_ord.is_some()) => {}
                Action::Move { x, y } => {
                    if self.unit(h).is_some_and(|u| u.path.is_empty()) {
                        self.path_to(h, (x, y));
                    }
                    if self.walk(h, dt) {
                        if let Some(u) = self.unit_mut(h) {
                            u.action = Action::Idle;
                            u.order = Order::default();
                        }
                    }
                }
                Action::Follow { target } => {
                    let Some((tx, ty)) = self.widget_pos(target) else {
                        self.unit_mut(h).unwrap().action = Action::Idle;
                        continue;
                    };
                    let u = self.unit(h).unwrap();
                    if (u.x - tx).hypot(u.y - ty) < self.misc_f32("FollowRange", 300.0) {
                        let u = self.unit_mut(h).unwrap();
                        u.path.clear();
                        set_anim(u, "stand");
                    } else {
                        if u.path.is_empty() || u.repath_timer <= 0.0 {
                            self.path_to(h, (tx, ty));
                        }
                        self.walk(h, dt);
                    }
                }
                Action::Attack { target } => {
                    let valid = self.unit(target).is_some_and(|t| !t.dead);
                    if !valid || wp.is_none() {
                        let u = self.unit_mut(h).unwrap();
                        u.action = Action::Idle;
                        u.order = Order::default();
                        set_anim(u, "stand");
                        continue;
                    }
                    if let Some(u) = self.unit_mut(h).filter(|u| u.extra.reaction > 0.0) {
                        u.extra.reaction -= dt;
                        continue;
                    }
                    // (A structure waiting its reaction or held in its walk into range,
                    // `World::reach_tried`.)
                    if self.unit(h).is_some_and(|u| u.extra.reach_walk.is_some()) {
                        continue;
                    }
                    // On a target it took up itself it looks again each second (the look
                    // timer runs 1 with a target, DAT_6fd6bf44) and takes a better one
                    // between swings (measured: a footman striking a farm turns on an enemy
                    // footman made beside it at 3 seconds, its next strike at 4.72).
                    // (A structure's comes by its timer, `World::structure_look`.)
                    let own = !self.attacks_in_place(h) && self.unit(h).is_some_and(|u| u.extra.acquired == target && u.swing.is_none() && u.acquire_timer <= 0.0 && wp.is_some());
                    if own {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        let creeps = self.unit(h).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
                        let better = self.find_target_ex(h, range, creeps).filter(|&t| t != target);
                        if let Some(u) = self.unit_mut(h) {
                            u.acquire_timer = u.acquire_timer.max(-dt) + 1.0;
                        }
                        if let Some(t) = better {
                            if let Some(u) = self.unit_mut(h) {
                                u.action = Action::Attack { target: t };
                            }
                            self.acquired(h, t);
                            continue;
                        }
                    }
                    // Turned invisible, it lets a target it took up itself go once its
                    // swing is done (measured: a Blademaster that had taken up a footman
                    // as it windwalked strikes once and attacks no more).
                    if self.cloaked(h) && self.unit(h).is_some_and(|u| u.extra.acquired == target && u.swing.is_none()) {
                        let u = self.unit_mut(h).unwrap();
                        u.action = Action::Idle;
                        u.order = Order::default();
                        set_anim(u, "stand");
                        continue;
                    }
                    self.try_attack(h, target, dt, false);
                }
                Action::AttackGround { x, y } => self.attack_ground_step(h, (x, y), dt),
                Action::AttackMove { x, y } => {
                    if self.unit(h).is_some_and(|u| u.acquire_timer <= 0.0) {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        // Creeps likewise (measured: a footman attack-moving past a gnoll
                        // camp 200 off walks on by).
                        let creeps = self.unit(h).is_some_and(|u| u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
                        if let Some(t) = self.find_target_ex(h, range, creeps) {
                            self.acquired(h, t);
                            self.spell_ai(h, t, h);
                            if self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                                continue;
                            }
                            self.try_attack(h, t, dt, false);
                            continue;
                        }
                        self.acquire_miss(h, dt);
                    }
                    if self.unit(h).is_some_and(|u| u.path.is_empty()) {
                        self.path_to(h, (x, y));
                    }
                    if self.walk(h, dt) {
                        let u = self.unit_mut(h).unwrap();
                        u.action = Action::Idle;
                        // Its order is done (measured: an attack-move that has arrived reads no order).
                        u.order = Order::default();
                    }
                }
                // A gold run walks with the walkers and goes in and out by its timer.
                Action::Harvest { .. } if self.unit(h).is_some_and(|u| u.extra.gold_run.is_some()) => {}
                // An AI's acolyte walks to its mine with the walkers until in range.
                Action::Harvest { .. } if self.pathing.is_some() && self.unit(h).is_some_and(|u| u.ai_harvest_walk()) => {}
                Action::Harvest { target } => self.harvest(vm, h, target, dt),
                Action::Build { id, x, y } => self.build(vm, h, id, x, y, dt),
                Action::Construct { target } => {
                    let done = self.unit(target).is_none_or(|b| b.constructing.is_none() || b.dead);
                    // One consumed stays inside until it goes, or, its structure lost,
                    // is given back to its owner's tech tree and food and comes out
                    // (CAbilityNightElfBuild's vf210, `FUN_6f543ca0`: `FUN_6f25ba00`, as
                    // made anew; provisional: for a structure killed as for one
                    // cancelled).
                    let consumed = self.unit(h).is_some_and(|u| u.extra.consumed);
                    if done && consumed {
                        if self.unit(target).is_some_and(|b| !b.dead && b.constructing.is_none()) {
                            continue 'units;
                        }
                        let (owner, type_id) = self.unit(h).map(|u| (u.owner, u.type_id)).unwrap();
                        self.tech_add(owner, type_id);
                        self.unit_mut(h).unwrap().extra.consumed = false;
                        self.update_food();
                    }
                    let inside = self.unit(h).is_some_and(|u| u.hidden);
                    if done && inside {
                        // Out of the finished (or lost) structure beside it.
                        let r = self.unit(h).map_or(16.0, |u| u.collision);
                        let p = if self.unit(target).is_some() { Some(self.spawn_point(target, r)) } else { None };
                        let u = self.unit_mut(h).unwrap();
                        if let Some((x, y)) = p {
                            u.x = x;
                            u.y = y;
                        }
                        u.hidden = false;
                    }
                    let u = self.unit_mut(h).unwrap();
                    if done {
                        u.action = Action::Idle;
                        u.building = 0;
                        set_anim(u, "stand");
                    } else {
                        set_anim(u, "stand work");
                    }
                }
                Action::ReturnResources => self.return_resources(h, dt),
                Action::Load { target } => self.load_step(h, target, dt),
                Action::Board { transport } => self.board_step(h, transport, dt),
                Action::Unload { x, y } => self.unload_step(h, x, y, dt),
                Action::Repair { target } => self.repair_step(h, target, dt),
                Action::Patrol { a, b } => {
                    if self.unit(h).is_some_and(|u| u.path.is_empty()) {
                        self.path_to(h, b);
                    }
                    if self.walk(h, dt) {
                        self.unit_mut(h).unwrap().action = Action::Patrol { a: b, b: a };
                    }
                }
            }
        }
        self.separate_units(dt);
        self.update_missiles(vm, dt);
    }

    /// Gold, lumber, and food a unit or structure type costs.
    pub fn cost(&self, id: Id) -> (i32, i32, i32) {
        self.objects.units.get(id).map_or((0, 0, 0), |o| (o.int("goldcost"), o.int("lumbercost"), o.int("fused")))
    }

    /// Checks and spends resources, returning the interface error string key on failure.
    /// Pays for `id`: a hero while the player holds a hero token (the melee start's free
    /// hero) costs the token instead of gold and lumber (FUN_6f40c600, FUN_6f5a8f80).
    /// Returns whether a token paid.
    pub fn spend(&mut self, player: u8, id: Id, food: bool) -> Result<bool, &'static str> {
        let token = self.hero_token_pays(player, id);
        let (g, l, f) = self.cost(id);
        let (g, l) = if token { (0, 0) } else { (g, l) };
        let p = &self.players[player as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        if food && f > 0 && !self.cheat(crate::cheats::flag::FOOD) && p.get_state(player_state::RESOURCE_FOOD_USED) + f > p.get_state(player_state::RESOURCE_FOOD_CAP) {
            return Err("Nofood");
        }
        log::debug!("t={:.1} player {player} pays {g} gold {l} lumber for {}", self.time, crate::objects::id_str(id));
        let p = &mut self.players[player as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        if token {
            *p.state.entry(player_state::RESOURCE_HERO_TOKENS).or_insert(0) -= 1;
        }
        Ok(token)
    }

    /// Whether a hero token would pay for `id`: a hero type, with a token in hand.
    pub fn hero_token_pays(&self, player: u8, id: Id) -> bool {
        id[0].is_ascii_uppercase() && self.objects.units.get(id).is_some() && self.players.get(player as usize).is_some_and(|p| p.get_state(player_state::RESOURCE_HERO_TOKENS) > 0)
    }

    /// Cancelled production: a token-paid hero gives its token back, anything else its
    /// cost at `rate`.
    fn refund_queued(&mut self, player: u8, id: Id, rate: f32, token: bool) {
        if token {
            *self.players[player as usize].state.entry(player_state::RESOURCE_HERO_TOKENS).or_insert(0) += 1;
        } else {
            self.refund(player, id, rate);
        }
    }

    pub fn refund(&mut self, player: u8, id: Id, rate: f32) {
        let (g, l) = if self.objects.upgrades.get(id).is_some() {
            // The level being researched is the next one after those finished.
            let level = self.players[player as usize].tech_researched.get(&id).copied().unwrap_or(0);
            self.upgrade_cost(id, level)
        } else {
            let (g, l, _) = self.cost(id);
            (g, l)
        };
        let p = &mut self.players[player as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += (g as f32 * rate) as i32;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += (l as f32 * rate) as i32;
    }

    /// A worker's order to build a structure at a point, paid for at once (measured:
    /// the original takes a barracks' cost as IssueBuildOrder is called, and gives it
    /// all back at once when the peasant is stopped or killed on its way).
    ///
    /// A structure that uses food needs it free, as a unit trained does (Nofood; maps
    /// limit structures so: Mafarazzo TD's HSBC uses the one food its Castle makes;
    /// provisional: whether the start checks again).
    pub fn order_build(&mut self, h: u32, id: Id, x: f32, y: f32) -> Result<(), &'static str> {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return Err("") };
        self.settle_paid_build(h);
        self.spend(owner, id, true)?;
        if let Some(u) = self.unit_mut(h) {
            u.action = Action::Build { id, x, y };
            u.path.clear();
            // Its order is the structure's id (GetUnitCurrentOrder).
            u.order = Order { id: crate::objects::id_to_int(id), target_x: x, target_y: y, target: 0 };
        }
        self.paid_builds.push((h, owner, id));
        Ok(())
    }

    /// Refunds a worker's paid build order whether or not it still stands.
    fn settle_paid_build(&mut self, h: u32) {
        while let Some(k) = self.paid_builds.iter().position(|&(w, _, _)| w == h) {
            let (_, owner, id) = self.paid_builds.remove(k);
            self.refund(owner, id, 1.0);
        }
    }

    /// Refunds paid build orders their workers no longer follow (stopped, ordered
    /// elsewhere, dead, or gone) without having begun the structure.
    pub(crate) fn settle_paid_builds(&mut self) {
        let mut k = 0;
        while k < self.paid_builds.len() {
            let (h, owner, id) = self.paid_builds[k];
            let keeps = self.unit(h).is_some_and(|u| !u.dead && matches!(u.action, Action::Build { id: b, .. } if b == id));
            if keeps {
                k += 1;
            } else {
                self.paid_builds.remove(k);
                self.refund(owner, id, 1.0);
            }
        }
    }

    /// Queues a unit at a structure (a train order).
    pub fn train(&mut self, h: u32, id: Id) -> Result<(), &'static str> {
        self.train_by(h, id, false)
    }

    /// `train`, by a player's order or its AI's (`ai`).
    pub fn train_by(&mut self, h: u32, id: Id, ai: bool) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        if u.dead || u.constructing.is_some() || u.extra.upgrading.is_some() || u.queue.len() >= 7 {
            return Err("");
        }
        let owner = u.owner;
        if !self.missing_requirements(owner, id, self.requirement_level(owner, id)).is_empty() || !self.tech_allowed(owner, id, 1) {
            return Err("");
        }
        let token = self.spend(owner, id, true)?;
        let u = self.unit_mut(h).unwrap();
        let first = u.queue.is_empty();
        u.extra.queue_tokens.resize(u.queue.len(), false);
        u.queue.push(id);
        u.extra.queue_tokens.push(token);
        // Queued, it starts in its owner's tech tree (FUN_6f3f7f70: as it takes a slot).
        self.tech_reg(owner, id).producing += 1;
        self.tech_changed(owner);
        // The order's object, freed as the turn's run starts; to an empty queue, the
        // training's `rcm and `rtl, its time counting from the next turn (traced: a
        // peon ordered at turn 991 is made in turn 1141's run; one queued behind
        // another not yet traced).
        // An AI's order is its own and a block (`FUN_6f9cafe3`, `FUN_6f9cb799`), both
        // going with the tick's orders, the block first (traced: Forestwalk's undead
        // computer's acolyte at its wake).
        let order = self.tags.take(codes::class::ORDER);
        if ai {
            let block = self.tags.take(codes::class::TASK_A);
            self.tags.issued.push(block);
            // A computer's Tree of Life lets its Entangle's listener go as it begins
            // training, between the two (read: Twisted Meadows' night elf computer's
            // first wisp, over LAN with the original; provisional: as its abilities are
            // disabled while it trains, `FUN_6f4076e0`, not read; they are enabled again
            // as its queue empties, `World::update_training`).
            let pils = self.unit_mut(h).map(|u| {
                let (eat, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut u.extra.spell_pils).into_iter().partition(|p| &p.1 == codes::ability::id::ENTANGLE);
                u.extra.spell_pils = keep;
                eat
            });
            self.tags.issued.extend(pils.unwrap_or_default().into_iter().map(|p| p.0));
            self.tags.issued.push(order);
        } else {
            self.tags.doom(order);
        }
        if first {
            let rcm = self.tags.take(codes::class::CONTINUOUS_MODIFIER);
            let rtl = self.tags.take(codes::class::THRESHOLD_LISTENER);
            let u = self.unit_mut(h).unwrap();
            u.extra.training_tags = Some((rcm, rtl));
            // (An AI's order, given by its thread after the structures' update in its
            // tick, counts from the next turn without waiting: read, Forestwalk's undead
            // computer's acolyte, ordered in turn 2 and made in turn 152.)
            u.extra.training_waits = !ai;
            // Its unit is made at the order's tick's main clock and the build time on,
            // in the turn's steps (read: Forestwalk's undead computer's fifth acolyte,
            // ordered at 18.43, made at 33.432907, after turn 335's first step freed a
            // walk's path; the fourth, ordered at 0.1, made at 15.1, as its turn
            // began).
            if ai {
                let build_time = self.objects.units.get(id).map_or(1.0, |o| o.real("bldtm")).max(0.01) as f64;
                let due = self.ai_tick_main(owner).map(|t| t + build_time);
                if let Some(u) = self.unit_mut(h) {
                    u.extra.train_due = due;
                }
            }
        }
        // Then the AI's issue of it (`FUN_6f693450`, as `FUN_6f9cb530` ends) makes an
        // order and a task, going with the tick's orders, the task first (traced: the
        // same acolyte's; the build order begun after it makes neither, as its altar's
        // in turn 45 shows; provisional: a train's alone, the one traced).
        if ai {
            let issue = self.tags.take(codes::class::ORDER);
            let task = self.tags.take(codes::class::TASK);
            self.tags.issued.extend([task, issue]);
        }
        Ok(())
    }

    /// Changes a structure's type as an upgrade starts (`World::upgrade_structure_by`),
    /// keeping its life and mana fractions, and its abilities till the upgrade is done
    /// (`World::replace_type_abilities`).
    pub fn transform_unit(&mut self, h: u32, id: Id) {
        let Some(o) = self.objects.units.get(id) else { return };
        let (hp, mana) = (o.real("HP").max(1.0), o.real("manaN"));
        let hp = self.handicap_life(self.unit(h).map_or(0, |u| u.owner), hp);
        // It acquires at the new type's range, none without a weapon, as one made so
        // (the original keeps the range with the unit's attack, CUnit+0x1e8's +0x244;
        // a Scout Tower upgraded to a Guard Tower attacks what comes near; provisional:
        // whether a range SetUnitAcquireRange gave is kept), and shows at its scale.
        let acquire = if o.int("weapsOn") == 0 { 0.0 } else { o.real("acquire") };
        let scale = o.real("modelScale").max(0.0001);
        let Some(u) = self.unit_mut(h) else { return };
        let lf = if u.max_life > 0.0 { u.life / u.max_life } else { 1.0 };
        let mf = if u.max_mana > 0.0 { u.mana / u.max_mana } else { 0.0 };
        u.type_id = id;
        u.max_life = hp;
        u.life = hp * lf;
        u.max_mana = mana;
        u.mana = mana * mf;
        u.acquire_range = acquire;
        u.scale = scale;
        self.update_food();
    }

    /// A structure's abilities as its upgrade is done, before its rise's objects go
    /// (CAbilityUpgradeInProgress's vf205, `FUN_6f5a75f0`, by `FUN_6f694f00` with the
    /// new type's abilList; the type's change as the upgrade began, `FUN_6f670950`,
    /// replaces none of them): its abilities, the newest first (`FUN_6f48ca70`), each
    /// kept when in the new list (taken from it, the list's last put in its place) or
    /// made permanent (UnitMakeAbilityPermanent; the engine's own, kept too, are not
    /// among the port's), else taken away (`FUN_6f6947e0`,
    /// `FUN_6f48e8a0`); then the list's others added in its order (`FUN_6f4725f0`),
    /// each with its objects, the taken ones' objects freed after them.  (Provisional:
    /// a taken ability's objects freed its own first.)
    pub(crate) fn replace_type_abilities(&mut self, h: u32, to: Id) {
        let Some(o) = self.objects.units.get(to) else { return };
        let mut list: Vec<Id> = o.list("abilList").iter().map(|s| crate::objects::id_from_str(s.trim())).filter(|a| a[0] != 0).collect();
        let Some(u) = self.unit(h) else { return };
        let had: Vec<Id> = u.abilities.iter().map(|a| a.0).collect();
        let permanent: Vec<Id> = u.extra.permanent.iter().filter(|p| p.1 > 0).map(|p| p.0).collect();
        let mut gone = Vec::new();
        for &a in had.iter().rev() {
            match list.iter().rposition(|x| *x == a) {
                Some(k) => {
                    list.swap_remove(k);
                }
                None if permanent.contains(&a) => {}
                None => gone.push(a),
            }
        }
        let code = |w: &World, a: Id| w.objects.abilities.get(a).map(|o| crate::objects::id_from_str(o.str("code").trim()));
        let mut taken: Vec<(u32, u32)> = Vec::new();
        for a in gone {
            let (Some(c), Some(type_id)) = (code(self, a), self.unit(h).map(|u| u.type_id)) else { continue };
            let n = self.ability_tags(type_id, &a).len().max(1);
            let at = self.unit(h).and_then(|u| u.extra.object_slots[2].iter().position(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| e.0 == c && e.1 == b)));
            let u = self.unit_mut(h).unwrap();
            u.abilities.retain(|x| x.0 != a);
            let slots: Vec<(u32, u32)> = at.map(|k| u.extra.object_slots[2].drain(k..(k + n).min(u.extra.object_slots[2].len())).collect()).unwrap_or_default();
            for &(s, _) in &slots {
                u.extra.spell_timers.retain(|&t| t != s);
                u.extra.spell_owners.retain(|&(t, _)| t != s);
            }
            taken.extend(slots);
        }
        // (Its code once, as a unit's making has each.)
        for a in list {
            let Some(c) = code(self, a) else { continue };
            if self.unit(h).is_some_and(|u| u.abilities.iter().any(|x| code(self, x.0) == Some(c))) {
                continue;
            }
            self.learned_objects(h, a);
            if let Some(u) = self.unit_mut(h) {
                u.abilities.push((a, 1));
            }
        }
        // The taken ones' objects go after the new ones are made (traced: the maftw
        // game's Fan Knifes Tower becoming the Assasin Tower, its Critical Strike's
        // object made, then its Moon Glaive's freed, then the rise's).
        for (s, _) in taken {
            self.tags.release(s);
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.books.clear();
        }
        self.sync_spell_books(h);
    }

    /// Starts researching an upgrade at a structure that offers it.
    pub fn research(&mut self, player: u8, id: Id) -> bool {
        let id_s = crate::objects::id_str(id);
        let level = self.players[player as usize].tech_researched.get(&id).copied().unwrap_or(0);
        let max = self.objects.upgrades.get(id).map_or(1, |o| o.int("maxlevel").max(1));
        if level >= max {
            return false;
        }
        let building = self.unit_order.iter().copied().find(|&h| {
            self.unit(h).is_some_and(|u| u.owner == player && u.is_building && !u.dead && u.constructing.is_none() && u.extra.upgrading.is_none() && u.queue.is_empty())
                && self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.list("Researches").iter().any(|r| r.trim() == id_s))
        });
        let Some(b) = building else { return false };
        self.research_at(b, id).is_ok()
    }

    /// Queues the next level of an upgrade at a structure.
    pub fn research_at(&mut self, b: u32, id: Id) -> Result<(), &'static str> {
        let Some(u) = self.unit(b) else { return Err("") };
        let player = u.owner;
        let level = self.players[player as usize].tech_researched.get(&id).copied().unwrap_or(0);
        let queued = self.unit_order.iter().filter(|&&h| self.unit(h).is_some_and(|u| u.owner == player && u.queue.contains(&id))).count() as i32;
        let max = self.objects.upgrades.get(id).map_or(1, |o| o.int("maxlevel").max(1));
        if level + queued >= max || u.queue.len() >= 7 || u.extra.upgrading.is_some() || !self.missing_requirements(player, id, (level + queued + 1) as u32).is_empty() || !self.tech_allowed(player, id, (level + queued + 1) as u32) {
            return Err("");
        }
        let (g, l) = self.upgrade_cost(id, level + queued);
        let p = &mut self.players[player as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        self.unit_mut(b).unwrap().queue.push(id);
        Ok(())
    }

    /// The item at the front of a structure's queue starts, if it has not: research, a
    /// structure upgrade, or training.
    pub(crate) fn start_production(&mut self, vm: &mut Vm, h: u32) {
        let Some(&id) = self.unit(h).filter(|u| !u.queue_started && u.constructing.is_none()).and_then(|u| u.queue.first()) else { return };
        self.unit_mut(h).unwrap().queue_started = true;
        let (pe, ue, ctx) = self.production_event(h, id, 0);
        self.fire_unit_pair(vm, h, pe, ue, ctx);
    }

    /// Gold and lumber for researching level `level` + 1 of an upgrade (base plus
    /// increment per level).
    pub fn upgrade_cost(&self, id: Id, level: i32) -> (i32, i32) {
        self.objects.upgrades.get(id).map_or((0, 0), |o| (o.int("goldbase") + o.int("goldmod") * level, o.int("lumberbase") + o.int("lumbermod") * level))
    }

    /// The start or cancel event of a queued item: research, a structure upgrade, or
    /// training (`which` 0 for start, 1 for cancel).
    fn production_event(&self, h: u32, id: Id, which: usize) -> (i32, i32, EventCtx) {
        let id_s = crate::objects::id_str(id);
        if self.objects.upgrades.get(id).is_some() {
            let ctx = EventCtx { research: crate::objects::id_to_int(id), ..Default::default() };
            [(EVENT_PLAYER_UNIT_RESEARCH_START, EVENT_UNIT_RESEARCH_START, ctx.clone()), (EVENT_PLAYER_UNIT_RESEARCH_CANCEL, EVENT_UNIT_RESEARCH_CANCEL, ctx)][which].clone()
        } else if self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.list("Upgrade").iter().any(|x| x.trim() == id_s)) {
            [(EVENT_PLAYER_UNIT_UPGRADE_START, EVENT_UNIT_UPGRADE_START, EventCtx::default()), (EVENT_PLAYER_UNIT_UPGRADE_CANCEL, EVENT_UNIT_UPGRADE_CANCEL, EventCtx::default())][which].clone()
        } else {
            let ctx = EventCtx { train_unit_type: crate::objects::id_to_int(id), ..Default::default() };
            [(EVENT_PLAYER_UNIT_TRAIN_START, EVENT_UNIT_TRAIN_START, ctx.clone()), (EVENT_PLAYER_UNIT_TRAIN_CANCEL, EVENT_UNIT_TRAIN_CANCEL, ctx)][which].clone()
        }
    }

    /// Cancels the last queued item and refunds it (MiscGame ResearchRefundRate,
    /// UpgradeRefundRate, or TrainRefundRate); a structure being built is cancelled
    /// instead (ConstructionRefundRate) and removed.
    pub fn cancel_last(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let owner = u.owner;
        // A revival stops, its cost back at ReviveRefundRate
        // (EVENT_PLAYER_HERO_REVIVE_CANCEL).
        if let Some((hero, _, _)) = u.revive {
            let (g, l, _) = self.revive_cost(hero);
            let rate = self.misc_f32("ReviveRefundRate", 1.0);
            if let Some(p) = self.players.get_mut(owner as usize) {
                *p.state.entry(crate::types::player_state::RESOURCE_GOLD).or_insert(0) += (g as f32 * rate) as i32;
                *p.state.entry(crate::types::player_state::RESOURCE_LUMBER).or_insert(0) += (l as f32 * rate) as i32;
            }
            self.unit_mut(h).unwrap().revive = None;
            if let Some(x) = self.unit_mut(hero) {
                x.reviving = false;
            }
            self.queue_unit_event(hero, EVENT_PLAYER_HERO_REVIVE_CANCEL, EVENT_UNIT_HERO_REVIVE_CANCEL, EventCtx { unit: hero, ..Default::default() });
            return;
        }
        if u.extra.upgrading.is_some() {
            return self.cancel_upgrade(h);
        }
        if u.constructing.is_some() {
            let id = u.type_id;
            // ConstructionRefundRate, less the share of life the structure lost while
            // being built under ConstructionDamageRefundPenalty
            // (CAbilityBuildInProgress::vf210).
            let mut rate = self.misc_f32("ConstructionRefundRate", 0.75);
            if self.misc_f32("ConstructionDamageRefundPenalty", 0.0) != 0.0 {
                rate *= (1.0 - u.extra.construction_damage / u.max_life.max(1.0)).clamp(0.0, 1.0);
            }
            self.refund(owner, id, rate);
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_CONSTRUCT_CANCEL, EVENT_UNIT_CONSTRUCT_CANCEL, EventCtx::default());
            self.cancelled_constructions.push(h);
            return;
        }
        let Some(&id) = u.queue.last() else { return };
        let (pe, ue, ctx) = self.production_event(h, id, 1);
        let key = match pe {
            EVENT_PLAYER_UNIT_RESEARCH_CANCEL => "ResearchRefundRate",
            EVENT_PLAYER_UNIT_UPGRADE_CANCEL => "UpgradeRefundRate",
            _ => "TrainRefundRate",
        };
        let rate = self.misc_f32(key, 1.0);
        let u = self.unit_mut(h).unwrap();
        let token = u.extra.queue_tokens.get(u.queue.len() - 1).copied().unwrap_or(false);
        u.queue.pop();
        u.extra.queue_tokens.truncate(u.queue.len());
        if u.queue.is_empty() {
            u.queue_progress = 0.0;
            u.queue_started = false;
        }
        if pe == EVENT_PLAYER_UNIT_TRAIN_CANCEL {
            self.tech_reg(owner, id).producing -= 1;
            self.tech_changed(owner);
        }
        self.refund_queued(owner, id, rate, token);
        self.queue_unit_event(h, pe, ue, ctx);
    }

    /// Cancels the queue's item at `slot` (a click on its icon, Tip10), refunded as
    /// [`World::cancel_last`] refunds.
    pub fn cancel_queue_at(&mut self, h: u32, slot: usize) {
        let Some(u) = self.unit(h) else { return };
        if u.revive.is_some() || u.constructing.is_some() || u.extra.upgrading.is_some() {
            return self.cancel_last(h);
        }
        let owner = u.owner;
        let Some(&id) = u.queue.get(slot) else { return };
        let (pe, ue, ctx) = self.production_event(h, id, 1);
        let key = match pe {
            EVENT_PLAYER_UNIT_RESEARCH_CANCEL => "ResearchRefundRate",
            EVENT_PLAYER_UNIT_UPGRADE_CANCEL => "UpgradeRefundRate",
            _ => "TrainRefundRate",
        };
        let rate = self.misc_f32(key, 1.0);
        let u = self.unit_mut(h).unwrap();
        let token = u.extra.queue_tokens.get(slot).copied().unwrap_or(false);
        u.queue.remove(slot);
        if slot < u.extra.queue_tokens.len() {
            u.extra.queue_tokens.remove(slot);
        }
        if slot == 0 {
            u.queue_progress = 0.0;
            u.queue_started = false;
        }
        if pe == EVENT_PLAYER_UNIT_TRAIN_CANCEL {
            self.tech_reg(owner, id).producing -= 1;
            self.tech_changed(owner);
        }
        self.refund_queued(owner, id, rate, token);
        self.queue_unit_event(h, pe, ue, ctx);
    }

    /// A free spot near a structure for a new unit: the nearest walkable cell outside it.
    pub fn spawn_point_pub(&self, h: u32, radius: f32) -> (f32, f32) {
        self.spawn_point(h, radius)
    }

    fn spawn_point(&self, h: u32, radius: f32) -> (f32, f32) {
        let u = self.unit(h).unwrap();
        let (x, y) = (u.x, u.y);
        let Some(p) = &self.pathing else { return (x, y - self.reach_radius(h) - radius) };
        let c = p.cell_of(x, y);
        for r in 1..64 {
            // Search rings starting south of the structure.
            for k in 0..(8 * r) {
                let side = k / (2 * r);
                let t = (k % (2 * r)) as i32 - r;
                let (dx, dy) = match side {
                    0 => (t, -r),
                    1 => (r, t),
                    2 => (-t, r),
                    _ => (-r, -t),
                };
                let cc = (c.0 + dx, c.1 + dy);
                if p.fits(cc, radius * 0.5) {
                    return p.center(cc);
                }
            }
        }
        (x, y)
    }

    fn update_structure(&mut self, vm: &mut Vm, h: u32, dt: f32) {
        // A frozen structure (Freezing Breath) does nothing: no training, research,
        // building, or attacks.
        if self.buff_mods(h).4 {
            return;
        }
        // warpten: building, training, research, and revival go 16 times as fast.
        let dt = if self.cheat(crate::cheats::flag::FAST_BUILD) { dt * crate::cheats::FAST } else { dt };
        self.update_revive(vm, h, dt);
        // Construction.
        if let Some(left) = self.unit(h).and_then(|u| u.constructing) {
            // A peasant's structure stands still without a builder at it, and goes up
            // faster with more (powerbuild), each extra builder costing its share.
            let needs = self.unit(h).is_some_and(|u| u.extra.needs_builder);
            let speed = if needs { self.build_speed(h) } else { 1.0 };
            if speed != 1.0 {
                // (Its pace changed, it is done as its time runs out.)
                if let Some(u) = self.unit_mut(h) {
                    u.extra.build_due = None;
                }
            }
            if speed <= 0.0 {
                return;
            }
            if speed > 1.0 {
                self.charge_powerbuild(h, dt);
            }
            let dt = dt * speed;
            let left = left - dt;
            let u = self.unit_mut(h).unwrap();
            let total = u.build_time.max(0.01);
            // Hit points rise with construction from 10% of maximum.
            u.life = (u.life + u.max_life * 0.9 * dt / total).min(u.max_life);
            if left <= 0.0 && u.extra.build_due.is_none() {
                self.complete_construction(vm, h);
            } else {
                // (One due on the clock is done in the turn's steps, `constructions_to`.)
                u.constructing = Some(left.max(f32::MIN_POSITIVE));
            }
            return;
        }
        // Production.
        let Some(&id) = self.unit(h).and_then(|u| u.queue.first()) else { return };
        if self.objects.units.get(id).is_some() {
            return;
        }
        self.start_production(vm, h);
        if let Some(up) = self.objects.upgrades.get(id) {
            let owner = self.unit(h).unwrap().owner;
            let level = self.players[owner as usize].tech_researched.get(&id).copied().unwrap_or(0);
            let t = (up.real("timebase") + up.real("timemod") * level as f32).max(0.01);
            let u = self.unit_mut(h).unwrap();
            u.queue_progress += dt;
            if u.queue_progress >= t {
                u.queue_progress = 0.0;
                u.queue.remove(0);
                if !u.extra.queue_tokens.is_empty() {
                    u.extra.queue_tokens.remove(0);
                }
                u.queue_started = false;
                *self.players[owner as usize].tech_researched.entry(id).or_insert(0) += 1;
                self.research_changed(owner);
                let sound = format!("ResearchComplete{}", self.race_label(owner));
                self.requests.push(Request::Completed { unit: h, id, level: level as u32 + 1, sound });
                let ctx = EventCtx { research: crate::objects::id_to_int(id), ..Default::default() };
                self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_RESEARCH_FINISH, EVENT_UNIT_RESEARCH_FINISH, ctx);
            }
            return;
        }
        // A unit trained (or an upgrade into one) goes on in its own pass, before the
        // tick's first movement step (`World::train_before_steps`).
    }

    /// A structure KillUnit killed, as its body goes: the rest of its kill's tasks, its
    /// abilities' objects (the last first), its own objects, and its detection's record,
    /// in the order of a removal's (traced: Mafarazzo TD's Freeze Tower exploded by its
    /// Sell trigger, its kill's third task, its abilities, its reals, path, regions,
    /// position, itself, its kill's second task, its `Cdet`).
    fn body_objects_go(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        let k = std::mem::take(&mut u.extra.kill_tasks);
        if k.len() != 2 {
            return;
        }
        let objects = u.extra.object_slots.clone();
        let detection = u.extra.detected_by.first().map(|d| d.1);
        let live = |w: &World, (s, b): (u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == b) && !w.tags.doomed.contains(&s);
        let mut gone = vec![k[1]];
        gone.extend(objects[2].iter().rev().copied().filter(|&o| live(self, o)).map(|o| o.0));
        gone.extend([8, 7, 6, 5, 4, 9, 2, 3, 1, 0].iter().filter_map(|&j| objects[0].get(j).filter(|&&o| live(self, o)).map(|o| o.0)));
        gone.push(k[0]);
        if let Some(d) = detection.filter(|&d| self.tags.live.get(&d).is_some_and(|e| &e.0 == codes::class::DETECTION)) {
            gone.push(d);
        }
        for s in gone {
            self.tags.doom(s);
        }
    }

    /// A structure's construction done: its objects go at once (the rise's listener, its
    /// modifier, the life's modifier, the real, and Build In Progress; traced: Twisted
    /// Meadows' human computer's first farm), then its owner's tech and its events.
    fn complete_construction(&mut self, vm: &mut Vm, h: u32) {
        let last = self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i));
        // (Done in the turn's last main tick, the first at or past its due: its player
        // builder's order ends in the next, the next turn's first, `player_builder_done`.)
        let n = self.tick_clocks.1.len();
        let in_last_tick = self.unit(h).and_then(|u| u.extra.build_due).is_some_and(|d| n >= 3 && self.tick_abs(n - 3).is_some_and(|c| d as f64 > c + 1e-6));
        self.look_when_built(h);
        let Some(u) = self.unit_mut(h) else { return };
        u.constructing = None;
        // Its Blight Growth begins its steps now (`World::update_blight`).
        u.extra.blight_start = u.extra.build_due.map(|d| d as f64).or(last);
        u.extra.build_due = None;
        restart_anim(u, "stand");
        let objects = std::mem::take(&mut u.extra.build_objects);
        // (With a builder it consumes, after that one's removal has made its tasks:
        // traced, Mafarazzo TD's wisp's Tower of Vengence, the wisp's block and four
        // tasks made, then these five freed, then the wisp's.)
        let consumed: Vec<u32> = self.unit_order.iter().copied().filter(|&b| self.unit(b).is_some_and(|u| !u.dead && u.extra.consumed && matches!(u.action, Action::Construct { target } if target == h))).collect();
        if objects.len() == 5 && consumed.is_empty() {
            for k in [3, 2, 4, 1, 0] {
                self.tags.release(objects[k]);
            }
        }
        let u = self.unit_mut(h).unwrap();
        let (owner, id) = (u.owner, u.type_id);
        self.tech_built(owner, id);
        let sound = format!("JobDoneSound{}", self.race_label(owner));
        self.requests.push(Request::Completed { unit: h, id, level: 1, sound });
        self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_CONSTRUCT_FINISH, EVENT_UNIT_CONSTRUCT_FINISH, EventCtx::default());
        // Then its AI's builder takes its next order (provisional: by the structure's
        // event 0xd01da after its finish events, `FUN_6f67b630` and `FUN_6f679d50`).
        self.ai_builder_done(h);
        self.player_builder_done(h, in_last_tick);
        // Then the builder's build ability ends its work (Build In Progress's vf213,
        // `FUN_6f422c10`, calling its vf209 after `FUN_6f427e40`'s events): one the
        // structure consumed goes (`FUN_6f543eb0`, `FUN_6f694690(1, 0)`).
        for &b in &consumed {
            crate::natives::units::remove_with_tasks(self, vm, b, false);
        }
        if objects.len() == 5 && !consumed.is_empty() {
            for k in [3, 2, 4, 1, 0] {
                self.tags.release(objects[k]);
            }
        }
    }

    /// An armed structure looks for a target as it is done: its look's timer (event
    /// 0xd01c1) started `LOOK_AFTER_DROP` on (`FUN_6f49d000`, its wait DAT_6fcd53a4)
    /// from the agent queue's clock as the construction's end falls due on the main
    /// clock, the clock of the first main tick at or past it, then every half second
    /// from that due as it finds none, its wait at its making forgotten.  Measured
    /// (Mafarazzo TD's flying peasant's Arrow Tower, `abtest/mafb.w3x`, its build done
    /// 19.147955; breakpoints on `FUN_6f493ff0` from `CAbilityAttack::vf3_6f49a5f0`):
    /// its looks in the main ticks 19.157785, 19.662701, 20.162617, and so on each
    /// half second, its first look's due so 19.152787 and 0.01; the port's had kept its
    /// making's phase, looking at 19.4, 19.9, and on.  (Which event starts it there is
    /// not read.)
    fn look_when_built(&mut self, h: u32) {
        use crate::efloat as ef;
        let Some(u) = self.unit(h) else { return };
        let (Some(due), Some(_)) = (u.extra.build_due, u.extra.look_timer) else { return };
        let n = self.tick_clocks.1.len();
        let Some(c) = (0..n).filter_map(|i| self.tick_abs(i)).find(|&c| c >= due as f64 - 1e-7).or_else(|| n.checked_sub(1).and_then(|i| self.tick_abs(i))) else { return };
        let first = f32::from_bits(ef::add((c as f32).to_bits(), crate::tasks::LOOK_AFTER_DROP.to_bits()));
        // (Taken in the last main tick short of it, that tick's game time its look's.)
        let i = (0..n).rev().find(|&i| self.tick_abs(i).is_some_and(|t| t < first as f64)).unwrap_or(0);
        let at = (self.tick_clocks.0 + i as i64) as f64 * 0.005;
        let wait = (at - self.time_exact) as f32 + self.tick_dt;
        let k = self.tags.start_timer("49d000");
        if let Some(u) = self.unit_mut(h) {
            u.acquire_timer = wait;
            u.extra.look_timer = Some((k, first, 0.5));
        }
    }

    /// A player's builder at its structure's end: its order to build, none queued
    /// behind it (`World::player_build_objects`), ends, the rest of its objects freed
    /// after the structure's, the last first (traced: Mafarazzo TD's flying peasant's
    /// Arrow Tower, done a second after it was begun, its `Atrg`, `tskO`, two tasks,
    /// `tskA`, and `ordt` then, over LAN with the original).  For a structure done in
    /// its turn's last main tick, they go as the next turn starts: its builder's order
    /// ends in the next main tick (traced: maftr's Town Hall, due 8.589906, done in the
    /// main tick at 8.594559, the turn's last, its order's six freed in the next turn;
    /// its Cutter due 20.977648, done at 20.982479, two ticks before the turn's end,
    /// its builder's six in that turn).
    fn player_builder_done(&mut self, structure: u32, next_tick_next_turn: bool) {
        let builders: Vec<u32> = self.unit_order.iter().copied().filter(|&b| self.unit(b).is_some_and(|u| !u.dead && u.extra.ai_next_order.is_none() && matches!(u.action, Action::Construct { target } if target == structure) && matches!(u.extra.ai_order.len(), 6 | 7))).collect();
        for b in builders {
            let Some(u) = self.unit_mut(b) else { continue };
            let old = std::mem::take(&mut u.extra.ai_order);
            for s in old.into_iter().rev() {
                if next_tick_next_turn {
                    self.tags.next_turn.push(s);
                } else {
                    self.tags.release(s);
                }
            }
        }
    }

    /// The constructions due on the main clock by main tick `tick` (by the turn's last
    /// with none), done as their step begins.
    pub(crate) fn constructions_to(&mut self, vm: &mut Vm, tick: Option<i64>) {
        let at = match tick {
            Some(t) => self.tick_main(t),
            None => self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i)),
        };
        let Some(at) = at else { return };
        let due: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.constructing.is_some() && u.extra.build_due.is_some_and(|d| d as f64 <= at + 1e-6))).collect();
        for h in due {
            self.complete_construction(vm, h);
        }
    }

    /// The AI's trainings held for the steps (`World::update_training`) due by main tick
    /// `tick` (all with none), made as their step begins.
    pub(crate) fn trainings_to(&mut self, vm: &mut Vm, tick: Option<i64>) {
        let at = tick.and_then(|t| self.tick_main(t));
        let ready: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| {
                self.unit(h).is_some_and(|u| {
                    let build_time = u.queue.first().and_then(|&id| self.objects.units.get(id)).map_or(1.0, |o| o.real("bldtm")).max(0.01);
                    u.extra.train_due.is_some_and(|d| at.is_none_or(|a| d <= a + 1e-6)) && u.queue_progress >= build_time - 1e-3
                })
            })
            .collect();
        for h in ready {
            if let Some(u) = self.unit_mut(h) {
                u.extra.train_due = None;
            }
            let (doomed, issued) = (self.tags.doomed.len(), self.tags.issued.len());
            self.update_training(vm, h, 0.0);
            // A computer's unit made in the steps, its AI's harvest given it as it was
            // made, lets go in the main tick after (the agents' destroy list,
            // `FUN_6f04c1a0`'s `FUN_6f04f7e0`): what its making and its AI's orders let go,
            // in turn, then its tasks and ptcf, before the next step (read, each free's
            // clock and caller at the tag allocator: Twisted Meadows' human computer's
            // peasant trained at 63.096783, its AI's orders' objects, the last order's
            // four tasks, the making's two, and the ptcf freed at 63.096882 through
            // `FUN_6f054190`, a waiting peasant's cluster's at the step at 63.113529).
            let given = tick.is_some() && self.ai.contains_key(&self.unit(h).map_or(u8::MAX, |u| u.owner)) && self.tags.issued.len() > issued;
            if given {
                let now: Vec<u32> = self.tags.doomed.split_off(doomed.min(self.tags.doomed.len())).into_iter().chain(self.tags.issued.split_off(issued)).collect();
                for s in now {
                    self.tags.release(s);
                }
            }
        }
    }

    /// Structures' training, before the tick's first movement step (read: a peon trained
    /// made as the turn begins, before the turn's steps, a peon's arrival at the mine
    /// and a wanderer's walk end in its last step freeing theirs after).
    pub(crate) fn train_before_steps(&mut self, vm: &mut Vm, dt: f32) {
        let structures: Vec<u32> = self.unit_order.iter().copied().filter(|&h| {
            self.unit(h).is_some_and(|u| u.is_building && !u.dead && !u.paused && u.constructing.is_none() && u.queue.first().is_some_and(|&id| self.objects.units.get(id).is_some()))
        }).collect();
        for h in structures {
            if self.buff_mods(h).4 {
                continue;
            }
            let dt = if self.cheat(crate::cheats::flag::FAST_BUILD) { dt * crate::cheats::FAST } else { dt };
            self.start_production(vm, h);
            // A training falling due in a main tick after the turn's first, the
            // computers' threads due before that tick run first (read, each
            // `GetPlayerState` and `SetProduce` and the training's objects: Twisted
            // Meadows' night elf computer's thread at 30.095949, its wisp asked for while
            // the Tree of Life still trained one, due 30.098299, so no Ancient of War
            // asked for after it; the port had made the wisp first).
            let due = self.unit(h).and_then(|u| {
                let build_time = u.queue.first().and_then(|&id| self.objects.units.get(id)).map_or(1.0, |o| o.real("bldtm")).max(0.01);
                u.extra.train_due.filter(|_| u.queue_progress + dt >= build_time - 1e-3)
            });
            if let Some(d) = due {
                let n = self.tick_clocks.1.len();
                if let Some(i) = (1..n).find(|&i| self.tick_abs(i).is_some_and(|c| c + 1e-6 >= d)) {
                    let t = self.tick_clocks.0 + i as i64;
                    self.ai_threads_to(((t - 1) as f64 * 0.005) as f32, Some(t));
                }
            }
            let doomed = self.tags.doomed.len();
            self.update_training(vm, h, dt);
            // (What a unit made as the turn begins lets go goes then, before the turn's
            // steps: read, Twisted Meadows' human computer's peasant trained as turn 152
            // began, its AI's first orders freed before a wood peasant's arrival at its
            // tree in that turn's steps.)
            for s in self.tags.doomed.split_off(doomed.min(self.tags.doomed.len())) {
                self.tags.release(s);
            }
        }
    }

    fn update_training(&mut self, vm: &mut Vm, h: u32, dt: f32) {
        let Some(&id) = self.unit(h).and_then(|u| u.queue.first()) else { return };
        let build_time = self.objects.units.get(id).map_or(1.0, |o| o.real("bldtm")).max(0.01);
        let u = self.unit_mut(h).unwrap();
        if std::mem::take(&mut u.extra.training_waits) {
            return;
        }
        u.queue_progress += dt;
        // Due on the clock at its time, not a hair after the turns' sum (read:
        // Forestwalk's undead computer's acolyte ordered in turn 2, made in turn 152;
        // 150 turns of 0.1 summed in single precision fall 2e-6 short of 15).
        if u.queue_progress < build_time - 1e-3 {
            return;
        }
        // One due after the turn's first step is made in the steps (`World::trainings_to`).
        let due = u.extra.train_due;
        let first = self.step_ticks.first().and_then(|&t| self.tick_main(t));
        if let (Some(due), Some(first)) = (due, first) {
            if due > first + 1e-6 {
                return;
            }
        }
        let u = self.unit_mut(h).unwrap();
        u.extra.train_due = None;
        // Food must be available when the unit finishes.
        let owner = u.owner;
        u.queue_progress = 0.0;
        u.queue.remove(0);
        if !u.extra.queue_tokens.is_empty() {
            u.extra.queue_tokens.remove(0);
        }
        u.queue_started = false;
        let radius = self.objects.units.get(id).map_or(16.0, |o| o.real("collision"));
        // About the structure's footprint, facing east (`FUN_6f677870` given 0; read:
        // the acolyte's heading 0 in the save).
        let (bx, by, bf, btex) = self.unit(h).map(|u| (u.x, u.y, u.facing, self.objects.units.get(u.type_id).map(|o| o.str("pathTex")).unwrap_or_default())).unwrap_or_default();
        let cells = if btex.is_empty() { Vec::new() } else { self.build_cells(&btex, bx, by, bf) };
        let rect = (!cells.is_empty()).then(|| (cells.iter().map(|c| c.0).min().unwrap(), cells.iter().map(|c| c.1).min().unwrap(), cells.iter().map(|c| c.0).max().unwrap() + 1, cells.iter().map(|c| c.1).max().unwrap() + 1));
        let (x, y) = rect.and_then(|r| crate::natives::units::trained_place(self, id, bx, by, r)).unwrap_or_else(|| self.spawn_point(h, radius));
        let new = crate::natives::units::create_unit(self, owner, id, x, y, 0.0);
        // Made finished from training: no longer in production (FUN_6f677870 with 4).
        self.tech_reg(owner, id).producing -= 1;
        self.tech_changed(owner);
        // Then a ptcf, and freed with the turn's releases, in order: the training's `rtl
        // and `rcm, the two tasks of the unit's making (the second first), and the ptcf
        // after what its finish's triggers let go (traced; with a trigger, Mafarazzo TD's
        // Sell, the maftw games: its execution and its actions' objects made first, the
        // ptcf freed after the execution).
        let ptcf = self.tags.take(codes::class::TRAINING_DONE);
        // Its queue emptied, its abilities are enabled again (`FUN_6f406c40`'s
        // `FUN_6f41de50`, `FUN_6f48c8c0`, `FUN_6f40bb00`), a computer's Entangle making
        // its listener anew (its vf 0xc8; read, the tag allocator's take and its
        // backtrace: Twisted Meadows' night elf computer's Tree of Life, its first wisp
        // trained at 14.1, the `` `pil `` after the ptcf).
        if self.ai.contains_key(&owner) && self.unit(h).is_some_and(|u| u.queue.is_empty()) && self.ability_with_code(h, codes::ability::ENTANGLE).is_some() {
            let pil = self.tags.take(codes::class::IN_RANGE_LISTENER);
            if let Some(u) = self.unit_mut(h) {
                u.extra.spell_pils.push((pil, *codes::ability::id::ENTANGLE));
            }
        }
        // (The making's two tasks, doomed as it was made, after these.)
        let tasks: Vec<u32> = if new != 0 { (0..2).filter_map(|_| self.tags.doomed.pop()).collect() } else { Vec::new() };
        // An AI's unit's tasks and ptcf go last in the turn's frees, after the orders
        // its AI gives it (read: Forestwalk's undead computer's acolyte made in turn
        // 152, its `rtl and `rcm freed at once, the two tasks and the ptcf after its
        // orders' objects at the turn's end).
        let late = self.ai.contains_key(&owner);
        if let Some((rcm, rtl)) = self.unit_mut(h).and_then(|u| u.extra.training_tags.take()) {
            if late {
                self.tags.release(rtl);
                self.tags.release(rcm);
            } else {
                self.tags.doom(rtl);
                self.tags.doom(rcm);
            }
        }
        // (One its AI gave its harvest as it was made goes on at once, its tasks and
        // ptcf with the tick's orders before the steps: read, Twisted Meadows' human
        // computer's peasant trained as turn 152 began.)
        let given = self.unit(new).is_some_and(|u| u.extra.ai_harvesting);
        if new != 0 {
            for t in tasks.into_iter().rev() {
                if late && given {
                    self.tags.issued.push(t);
                } else if late {
                    self.tags.later_now.push(t);
                } else {
                    self.tags.doom(t);
                }
            }
        }
        if late && given {
            self.tags.issued.push(ptcf);
        } else if late {
            self.tags.later_now.push(ptcf);
        }
        self.requests.push(Request::Ack { unit: new, kind: "Ready" });
        self.requests.push(Request::UnitReady { unit: new });
        // The rally point (FUN_6f429790): a unit or widget there is followed or worked
        // (Tip58); a point on the ground is attack-moved to (Tip59); one on the
        // structure itself sends nowhere.
        let (rally, on) = self.unit(h).map_or((None, 0), |u| (u.rally, u.extra.rally_target));
        if let (Some((rx, ry)), true) = (rally, new != 0) {
            if on != 0 && self.widget_pos(on).is_some() {
                if on != h {
                    let smart = crate::natives::basic::order_id("smart");
                    self.order_unit(new, smart, rx, ry, on);
                }
            } else {
                let attack = crate::natives::basic::order_id("attack");
                self.order_unit(new, attack, rx, ry, 0);
            }
        }
        let ctx = EventCtx { target_unit: new, train_unit_type: crate::objects::id_to_int(id), ..Default::default() };
        self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_TRAIN_FINISH, EVENT_UNIT_TRAIN_FINISH, ctx);
        if !late {
            self.tags.doom(ptcf);
        }
        // One its finish's triggers removed takes an order still, freed last with the
        // turn's releases (traced: the Sell trigger's n001, an `ordt` after its removal's
        // tasks, in both maftw games; provisional: why).
        if new != 0 && self.pending_removals.contains(&new) {
            let o = self.tags.take(codes::class::ORDER_TASKS);
            self.tags.removed.push(o);
        }
    }

    /// Whether a structure of type `id` can be placed centered at (x, y).
    pub fn can_place(&mut self, id: Id, x: f32, y: f32) -> bool {
        if self.builds_on(id) {
            return self.build_on_site(x, y).is_some();
        }
        if self.objects.units.get(id).is_some_and(|o| o.str("requirePlace").to_ascii_lowercase().contains("blighted")) && !self.footprint_blighted(id, x, y) {
            return false;
        }
        // Its whole footprint (the pathing texture's blue) on buildable ground, clear
        // of blockers and of other structures' whole footprints (measured: a computer's
        // first crypt and ziggurat stand clear of its necropolis's and haunted mine's
        // margins).
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.build_cells(&tex, x, y, 0.0);
        let Some(p) = &self.pathing else { return true };
        cells.iter().all(|&c| p.buildable(c))
    }

    /// The footprint's cells at a placement, each with whether it can be built on
    /// (CPlacementBox's per-cell colors), and their size.
    pub fn placement_cells(&mut self, id: Id, x: f32, y: f32) -> Vec<((f32, f32), bool)> {
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.build_cells(&tex, x, y, 0.0);
        let whole = self.builds_on(id) && self.build_on_site(x, y).is_some();
        let Some(p) = &self.pathing else { return Vec::new() };
        cells.iter().map(|&c| (p.center(c), whole || p.buildable(c))).collect()
    }

    /// Snaps a structure's placement as the original does (FUN_6f22f410): each
    /// coordinate is cut toward zero to a multiple of 64, then moved 32 away from zero
    /// when half its footprint's texels is odd, and 16 more when their number is odd
    /// (measured: a farm, 4 by 4, ordered at -4000, -2700 stands at -3968, -2688).
    pub fn snap_placement(&mut self, id: Id, x: f32, y: f32) -> (f32, f32) {
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        self.snap_to_texture(&tex, x, y)
    }

    /// The same snap by a pathing texture's size.
    pub fn snap_to_texture(&mut self, tex: &str, x: f32, y: f32) -> (f32, f32) {
        let (w, h) = self.pathing_texture_size(tex).unwrap_or((0, 0));
        let snap = |v: f32, n: i32| -> f32 {
            let neg = v < 0.0;
            let vi = v as i32;
            let base = vi - vi % 64;
            let off = ((n >> 1) & 1) * 32;
            let mut r = (base + if neg { -off } else { off }) as f32;
            if n & 1 == 1 {
                r += if neg { -16.0 } else { 16.0 };
            }
            r
        };
        (snap(x, w), snap(y, h))
    }

    /// An AI's builder walking by the movement steps reaching its site in step `k`:
    /// within StructureFollowRange it begins the structure there, the step's clocks the
    /// moment its making counts from (read: Forestwalk's undead computer's acolyte
    /// beginning its crypt as its walk arrives in turn 20's last step, 1.98, its order
    /// ending a second on, at 2.979974, and the crypt's alarm watching from then, both
    /// in turn 30).
    pub(crate) fn build_step_arrive(&mut self, vm: &mut Vm, h: u32, k: usize) -> bool {
        let Some(u) = self.unit(h).filter(|u| u.ai_build_walk()) else { return false };
        let Action::Build { id, x, y } = u.action else { return false };
        // In range by its place at the step's clock (read under winedbg, the builders'
        // velocity changes: the crypt's acolyte 103.56 from the site's middle at 1.95,
        // walking on, and stopped at 1.98, 96.95 from it; the altar's 102.75 at
        // 38.337, walking on, and stopped at 38.367, 96.5).
        let follow = self.misc_f32("StructureFollowRange", 100.0);
        let origin = self.pathing.as_ref().map_or([0.0, 0.0], |p| p.origin);
        let at = match (u.extra.mover, self.step_clocks.get(k)) {
            (Some(m), Some(&c)) => {
                let (mx, my) = m.at(c);
                (f32::from_bits(mx) * crate::pathing::CELL + origin[0], f32::from_bits(my) * crate::pathing::CELL + origin[1])
            }
            _ => (u.x, u.y),
        };
        if (at.0 - x).hypot(at.1 - y) > follow {
            return false;
        }
        self.making_clock = self.step_abs(k);
        // It begins from its place at that step (read: the crypt's acolyte placed off
        // the footprint from its cell at 1.98, (338, 501), `natives::units::
        // off_footprint`).
        if let Some(m) = self.unit(h).and_then(|u| u.extra.mover) {
            let at = self.step_clocks.iter().copied().find(|&c| f32::from_bits(c) > f32::from_bits(m.placed_at)).unwrap_or(crate::efloat::add(m.placed_at, crate::motion::STEP));
            self.stop_walker_at(h, at);
        }
        if let Some(u) = self.unit_mut(h) {
            u.path.clear();
            u.extra.mover = None;
            u.extra.build_legacy = true;
        }
        // What the begin lets go goes as its step ends (read: Forestwalk's undead
        // computer's ziggurat, begun in turn 77's second step, its build's `tsk.` and
        // its AI's objects freed before its walk's path and cluster in the third).
        let doomed = self.tags.doomed.len();
        self.build(vm, h, id, x, y, 0.0);
        for s in self.tags.doomed.split_off(doomed.min(self.tags.doomed.len())) {
            self.tags.release_in_step(s);
        }
        self.making_clock = None;
        true
    }

    fn build(&mut self, vm: &mut Vm, h: u32, id: Id, x: f32, y: f32, dt: f32) {
        // An AI's builder walks by the steps (`World::build_step_arrive`).
        if self.unit(h).is_some_and(|u| u.ai_build_walk()) {
            return;
        }
        let u = self.unit(h).unwrap();
        let (d, need_path, owner) = ((u.x - x).hypot(u.y - y), u.path.is_empty() || u.repath_timer <= 0.0, u.owner);
        // Structures whose requirements are not met, or over their limit, are not
        // started.
        if !self.missing_requirements(owner, id, 1).is_empty() || !self.tech_allowed(owner, id, 1) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        // The worker starts once within StructureFollowRange of the site, whatever the
        // structure's size (measured: farms, a blacksmith, a lumber mill, and barracks
        // all begun with their peasant 94 to 99 from the center).
        // (One arrived by the steps is in range: `World::build_step_arrive`.)
        if d > self.misc_f32("StructureFollowRange", 100.0) && self.making_clock.is_none() {
            if need_path {
                self.path_to(h, (x, y));
            }
            // Stop walking at the end of the path even when the site itself is blocked.
            if !self.walk(h, dt) {
                return;
            }
        }
        if !self.can_place(id, x, y) {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            set_anim(u, "stand");
            return;
        }
        // Paid for when ordered (order_build).
        self.paid_builds.retain(|&(w, _, _)| w != h);
        // A structure built on another unit stands on it and takes it over.
        let site = if self.builds_on(id) { self.build_on_site(x, y) } else { None };
        let (x, y) = site.and_then(|s| self.unit(s).map(|m| (m.x, m.y))).unwrap_or((x, y));
        // An AI's build order's second `tsk.` goes as the structure is begun, before
        // what the AI makes of the structure, and its walk's path and cluster as an
        // ended walk's do, in the next movement step (`FUN_6f15aa80`'s, `Tags::
        // walk_ended`; read with the main clock: Forestwalk's undead computer's crypt,
        // begun at 1.98, its turn's last step, the path and cluster freed at 2.009998,
        // the next turn's first, after its melee timer's objects; its ziggurat, begun at
        // 7.65, its turn's second step, the path and cluster freed last in that turn; its
        // altar, begun at 38.367, the path and cluster freed in the next turn before a
        // wanderer's arrival's tree in its last step; provisional: the rest of the
        // order).
        if let Some(order) = self.unit(h).filter(|u| u.extra.ai_order.len() == crate::ai::AI_BUILD.len()).map(|u| u.extra.ai_order.clone()) {
            self.tags.doom(order[7]);
            let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
            self.tags.walk_ended([order[16], order[15]], serial);
        }
        let b = crate::natives::units::create_unit(self, owner, id, x, y, self.misc_f32("BuildingAngle", 270.0));
        if let Some(s) = site {
            self.cover_mine(b, s);
        }
        // Its construction is done on the main clock at the build time from the step its
        // rise begins in: this one, or for a builder still walking to it the step it
        // arrives in (`construct_step_arrive`; measured, `CAbilityBuildInProgress`'s vf205
        // at 0x6f427e40: Twisted Meadows' human computer's altar, barracks, and second
        // farm done at 62.130093, 81.837433, and 85.755417, their begins' steps and their
        // build times on to 0.0001; its first farm, its builder arriving at 17.668, at
        // 52.668034, after turn 527's first order, where taking a turn's 0.1 a time had it
        // done a turn early).
        let stamp = self.step_ticks.get(self.step_range.0).and_then(|&t| self.tick_main(t));
        if let Some(bu) = self.unit_mut(b) {
            let t = bu.build_time;
            bu.constructing = Some(t);
            bu.extra.build_due = stamp.map(|s| f32::from_bits(crate::efloat::add((s as f32).to_bits(), t.to_bits())));
            bu.life = bu.max_life * 0.1;
            restart_anim(bu, "birth");
        }
        // Begun, it is in production in its owner's tech tree, not yet made or present
        // (`FUN_6f249cf0`; read: the undead computer's crypt begun at 2.0, its tech tree
        // hashing as the original's only so).
        self.tech_begun(owner, id);
        // The structure is begun with the worker where it stood (measured: a blacksmith's
        // peasant is inside the footprint when EVENT_PLAYER_UNIT_CONSTRUCT_START fires).
        // The event's data (`pucs`) is made when anything listens for it, before the
        // triggers run, and goes with their executions (`FUN_6f2609b0`, event 0x8021a;
        // traced: freed at the turn's end after the melee trigger's `+tex`).
        let listened = self.events.iter().any(|&e| matches!(self.handles.get(e), Some(Obj::Event(r)) if matches!(r.kind, EventKind::PlayerUnit { event: EVENT_PLAYER_UNIT_CONSTRUCT_START, .. })));
        let data = listened.then(|| self.tags.take(b"pucs"));
        // The data takes a handle as the event goes to its triggers, then the structure
        // (`FUN_6f262cb0`, each by `FUN_6f282e30`), and gives its own back as their
        // executions end, the next handle made taking it (read: Forestwalk's test map's
        // crypt begun at 1.98, the data's handle 0x100f61 and the crypt's 0x100f62, the
        // melee timer's first force at 2.0 then given 0x100f61).
        let data_handle = data.map(|_| {
            let d = self.handles.insert(Obj::Opaque("player unit event data".into()));
            self.handles.handle(b);
            d
        });
        let ctx = EventCtx { unit: b, player: owner, has_player: true, event_id: EVENT_PLAYER_UNIT_CONSTRUCT_START, ..Default::default() };
        self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_CONSTRUCT_START));
        if let Some(d) = data_handle {
            self.handles.release(d);
        }
        if let Some(d) = data {
            if self.exec_at_tick_end {
                self.tags.issued.push(d);
            } else {
                self.tags.doom(d);
            }
        }
        // Then its Build In Progress ability with its real (`FUN_6f533490`), and as the
        // build ability starts it (`FUN_6f41b380`), the rise's modifier and threshold
        // listener (`FUN_6f436b00`, event 0xd01c4) and its regeneration's modifier
        // (`FUN_6f431e50`) (traced: Forestwalk's undead computer's crypt; provisional:
        // when they go).
        if self.unit(b).is_some() {
            let objects: Vec<u32> = [codes::ability::id::BUILD_IN_PROGRESS, codes::class::REAL, codes::class::CONTINUOUS_MODIFIER, codes::class::THRESHOLD_LISTENER, codes::class::CONTINUOUS_MODIFIER].iter().map(|c| self.tags.take(c)).collect();
            let builds = [codes::ability::id::BUILD_HUMAN, codes::ability::id::BUILD_ORC, codes::ability::id::BUILD_UNDEAD, codes::ability::id::BUILD_NIGHT_ELF, codes::ability::id::BUILD_NAGA, codes::ability::id::BUILD_NEUTRAL];
            let by = self.unit(h).and_then(|w| w.extra.object_slots.iter().flatten().copied().find(|&(s, n)| self.tags.live.get(&s).is_some_and(|e| e.1 == n && builds.contains(&&e.0))));
            if let Some(bu) = self.unit_mut(b) {
                bu.extra.build_objects = objects;
                bu.extra.build_by = by;
            }
            // Begun, it lets go as the turn ends its own regeneration's modifier (the
            // build's own taking its place), its spells' timers, and its abilities' AI
            // listeners (read: Twisted Meadows' night elf computer's first Moon Well,
            // begun at 17.788, its `` `rcm `` and Replenish's `` `rtl `` and `` `pil ``
            // freed in turn 178; provisional: why, the abilities disabled as it builds).
            let live = |w: &World, (s, n): (u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == n && &e.0 == codes::class::CONTINUOUS_MODIFIER);
            let rcm = self.unit(b).and_then(|bu| bu.extra.object_slots[1].iter().copied().find(|&o| live(self, o))).map(|o| o.0);
            let (timers, pils) = self.unit_mut(b).map(|bu| (std::mem::take(&mut bu.extra.spell_timers), std::mem::take(&mut bu.extra.spell_pils))).unwrap_or_default();
            if let Some(s) = rcm {
                self.tags.doom(s);
                if let Some(bu) = self.unit_mut(b) {
                    for g in bu.extra.object_slots.iter_mut() {
                        g.retain(|o| o.0 != s);
                    }
                }
            }
            for s in timers.into_iter().chain(pils.into_iter().map(|p| p.0)) {
                self.tags.doom(s);
            }
        }
        if self.unit(h).is_none() {
            return;
        }
        // A worker where one made there could not stand moves off as such a one is placed;
        // one with the room its size class asks for stays (read: Twisted Meadows' human
        // computer's farm's peasant, at cells (170.15, 178.63), clear of the footprint
        // but its room not, standing then at (171.5, 179.5)).
        let (sx, sy) = crate::natives::units::off_footprint(self, h);
        // As the original builds (observed): a peasant (AHbu) works on the structure,
        // which advances only while one does; a peon or wisp (AObu, AEbu) goes inside
        // until it is done; an acolyte (AUbu) summons it and is free at once.
        // The engine gives builders their build ability by race.
        let race = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map(|o| o.str("race").to_ascii_lowercase()).unwrap_or_default();
        let enters = race == "orc" || race == "nightelf";
        let summons = race == "undead";
        if let Some(bu) = self.unit_mut(b) {
            bu.extra.needs_builder = !enters && !summons;
        }
        // A night elf builder an Ancient takes in is consumed (CAbilityNightElfBuild's
        // vf208, `FUN_6f5441e0`, for a structure whose class has `ancient`: its +0x248
        // bit 0x80, as the classes' table at 0x6fce5fa0 has it): it leaves its owner's
        // tech tree's present (`FUN_6f266f70`, by its +0x64 bit 0x10) and its food, and
        // goes as the structure is done (vf209, `FUN_6f543eb0`, `FUN_6f694690(1, 0)`;
        // `World::complete_construction`); another goes inside and comes out (vf209's
        // `FUN_6f566dd0`).  (Not yet in the port: its current order's tasks cleared then,
        // `FUN_6f673e80`, a night elf's build order having no objects here.)
        let ancient = self.unit(b).and_then(|s| self.objects.units.get(s.type_id)).is_some_and(|o| o.str("type").split(',').any(|c| c.trim().eq_ignore_ascii_case("ancient")));
        if race == "nightelf" && ancient {
            let (owner, type_id) = self.unit(h).map(|u| (u.owner, u.type_id)).unwrap();
            if self.unit(h).is_some_and(|u| u.constructing.is_none() && u.extra.upgrading.is_none()) {
                self.tech_reg(owner, type_id).present -= 1;
                self.tech_changed(owner);
            }
            if let Some(u) = self.unit_mut(h) {
                u.extra.consumed = true;
            }
            self.update_food();
        }
        let u = self.unit_mut(h).unwrap();
        u.x = sx;
        u.y = sy;
        u.path.clear();
        if summons {
            u.action = Action::Idle;
            set_anim(u, "stand");
            // Its order ends a second on, on the agent queue from the step it arrived in
            // (`FUN_6f54d0f0` setting the unit's animation timer, event 0xd019d, for 1.0;
            // read: Forestwalk's undead computer's acolyte, the crypt begun as its walk
            // arrived in turn 20's last step, its order ending in turn 30).
            let k = self.step_agent.len().saturating_sub(1);
            let at = self.making_clock.or_else(|| self.step_abs(k)).map_or(self.time as f64, |(_, a)| a);
            if let Some(u) = self.unit_mut(h) {
                u.extra.build_end = Some((at + 1.0) as f32);
            }
        } else {
            u.building = b;
            u.action = Action::Construct { target: b };
            u.hidden = enters;
            // A peasant builds under a repair order (measured: GetUnitCurrentOrder reads
            // repair once the structure is begun).
            if !enters {
                u.order = Order { id: crate::natives::basic::order_id("repair"), target_x: x, target_y: y, target: b };
            }
            // An AI's peasant then takes its order to build (a block, two tasks, a
            // `tskO`, its target, a `tskO`, three tasks, a `tskO`, a block) and the
            // structure's rise anew; with the step's end go the rise begun with the
            // structure (the last first), its build order's six left (6 to 2, its order),
            // and the new order's last six, the last first (traced: Twisted Meadows'
            // human computer's farm begun at 2.1, over LAN with the original).  One not
            // touching the structure has a target task before its target and searches
            // (its cluster's goal freed at once), the rise made in its next step when in
            // range (`construct_step_arrive`; traced: its farm begun at 17.578).  A flyer
            // needs only its reach as that step tests it (`FUN_6f41d1f0`; traced over LAN:
            // Mafarazzo TD's flying peasant, its player's, 2.97 cells from its Cutter
            // Tower's middle, clear of the ring of cells about it, the rise made anew at
            // once; provisional: why, the air layer not holding the structure).
            let order = self.unit(h).filter(|u| !enters && u.extra.ai_order.len() == crate::ai::AI_BUILD.len()).map(|u| u.extra.ai_order.clone());
            if let Some(order) = order {
                let touching = if self.move_kind(h) == crate::pathing::Move::Fly { self.construct_in_reach(h, b) } else { self.touches_footprint(h, b) };
                let mut codes: Vec<&crate::tags::Code> = vec![codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK_O, codes::ability::id::TARGET_CLASS, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_O, codes::class::TASK_A];
                if !touching {
                    codes.insert(5, codes::class::TASK_T);
                }
                let construct: Vec<u32> = codes.iter().map(|c| self.tags.take(c)).collect();
                let rise: Vec<u32> = if touching {
                    [codes::class::CONTINUOUS_MODIFIER, codes::class::THRESHOLD_LISTENER, codes::class::CONTINUOUS_MODIFIER].iter().map(|c| self.tags.take(c)).collect()
                } else {
                    let walk: Vec<u32> = [codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH].iter().map(|c| self.tags.take(c)).collect();
                    self.tags.release(walk[0]);
                    // (Its walk the newest: its path and cluster go first as it ends, read
                    // before a gold peasant's queuing at the mine in that step.)
                    let serial = self.tags.born;
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.construct_walk = Some(([construct[5], walk[1], walk[2]], 2));
                        u.extra.walk_serial = serial;
                    }
                    Vec::new()
                };
                let old = self.unit_mut(b).filter(|bu| bu.extra.build_objects.len() == 5).map(|bu| {
                    let old = bu.extra.build_objects[2..5].to_vec();
                    if !rise.is_empty() {
                        bu.extra.build_objects.splice(2..5, rise.iter().copied());
                    }
                    old
                });
                for s in old.unwrap_or_default().into_iter().rev() {
                    self.tags.doom(s);
                }
                for k in [6, 5, 4, 3, 2, 0] {
                    self.tags.doom(order[k]);
                }
                let n = construct.len();
                for k in (n - 6..n).rev() {
                    self.tags.doom(construct[k]);
                }
                // (The order its AI queued behind the build, the build order's last, is
                // kept for the structure's end: `World::ai_builder_done`; a player's
                // build order has none, `World::player_build_objects`.)
                let next = Some(order[14]).filter(|s| self.tags.live.get(s).is_some_and(|e| &e.0 == codes::class::ORDER));
                if let Some(u) = self.unit_mut(h) {
                    u.extra.ai_order = construct[..n - 6].to_vec();
                    u.extra.ai_next_order = next;
                }
            }
        }
    }

    /// The unit's ability with the given base code, with its level.
    pub fn ability_with_code(&self, h: u32, code: &str) -> Option<(Id, u32)> {
        let u = self.unit(h)?;
        u.abilities.iter().find_map(|&(id, lvl)| {
            let a = self.objects.abilities.get(id)?;
            (a.str("code") == code && self.ability_enabled(u.owner, id)).then_some((id, lvl.max(1) as u32))
        })
    }

    pub(crate) fn ability_real(&self, id: Id, field: &str, level: u32) -> f32 {
        self.objects.abilities.get(id).map_or(0.0, |a| a.level_real(field, level))
    }

    pub fn is_tree_pub(&self, h: u32) -> bool {
        self.is_tree(h)
    }

    fn is_tree(&self, h: u32) -> bool {
        match self.handles.get(h) {
            Some(Obj::Destructable(d)) => self.objects.destructables.get(d.type_id).is_some_and(|o| o.str("targType").to_ascii_lowercase().contains("tree")) && d.life > 0.0,
            _ => false,
        }
    }

    fn is_mine(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| !u.dead && !u.hidden) && self.ability_with_code(h, codes::ability::GOLD_MINE).is_some()
    }

    pub fn can_harvest(&self, h: u32, target: u32) -> bool {
        self.can_mine_covered(h, target)
            || self.ability_with_code(h, codes::ability::WISP_HARVEST).is_some() && self.is_tree(target)
            || self.ability_with_code(h, codes::ability::HARVEST).is_some() && (self.is_mine(target) || self.is_tree(target))
            || self.ability_with_code(h, codes::ability::HARVEST_LUMBER).is_some() && self.is_tree(target)
    }

    /// A worker leaving a mine with its load comes out on the mine's side toward the
    /// structure it returns to, just past its edge (measured against the original,
    /// whose workers then walk the straight way between the two).
    fn exit_toward_drop_off(&mut self, h: u32, mine: u32) {
        let (Some(hall), Some((mx, my)), Some(c)) = (self.drop_off(h), self.widget_pos(mine), self.unit(h).map(|u| u.collision)) else { return };
        let Some((hx, hy)) = self.widget_pos(hall) else { return };
        let len = (hx - mx).hypot(hy - my);
        if len < 1.0 {
            return;
        }
        let (dx, dy) = ((hx - mx) / len, (hy - my) / len);
        let mut r = 0.0;
        while r < len {
            let (x, y) = (mx + dx * r, my + dy * r);
            if self.edge_distance(mine, x, y).is_some_and(|e| e - c >= crate::pathing::CELL) {
                let kind = self.move_kind(h);
                if self.pathing.as_ref().is_none_or(|p| p.passable(p.cell_of(x, y), kind)) {
                    if let Some(u) = self.unit_mut(h) {
                        u.x = x;
                        u.y = y;
                        u.path.clear();
                        u.facing = dy.atan2(dx).to_degrees();
                    }
                }
                return;
            }
            r += 8.0;
        }
    }

    /// A gold mine another player's workers hold: one is inside, or one left it less
    /// than GoldMineOwnDuration ago (measured: while a peasant of Player 1 works a mine,
    /// Player 2's peasants ordered to harvest it or to autoharvestgold stay idle; two
    /// seconds after it last came out they go).
    pub(crate) fn mine_held_by_other(&self, h: u32, mine: u32) -> bool {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return false };
        self.ability_with_code(mine, codes::ability::GOLD_MINE).is_some() && self.unit(mine).and_then(|m| m.extra.mine_holder).is_some_and(|(p, until)| p != owner && until > self.time)
    }

    pub(crate) fn leave_mine(&mut self, h: u32) {
        let mine = self.unit(h).map_or(0, |u| u.inside);
        if mine == 0 {
            return;
        }
        if let Some(m) = self.unit_mut(mine) {
            m.inside = m.inside.saturating_sub(1);
        }
        if let Some(u) = self.unit_mut(h) {
            u.inside = 0;
            u.hidden = false;
        }
        let until = self.time + self.misc_f32("GoldMineOwnDuration", 2.0);
        if let Some(m) = self.unit_mut(mine).filter(|m| m.inside == 0) {
            if let Some(holder) = m.extra.mine_holder.as_mut() {
                holder.1 = until;
            }
        }
        self.mine_work_anim(mine);
    }

    fn carry_tag(u: &Unit) -> &'static str {
        if u.carry_gold > 0 {
            " gold"
        } else if u.carry_lumber > 0 {
            " lumber"
        } else {
            ""
        }
    }

    /// Radius used for reaching a unit: structures extend to their pathing footprint.
    pub fn reach_radius(&self, h: u32) -> f32 {
        let Some(u) = self.unit(h) else { return 16.0 };
        if u.pathing_cells.is_empty() {
            return u.collision;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
        for &(x, y) in &u.pathing_cells {
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
        let span = ((x1 - x0 + 1).max(y1 - y0 + 1)) as f32 * crate::pathing::CELL;
        (span / 2.0).max(u.collision)
    }

    /// Whether a unit touches a structure's footprint: a cell of the ring about the
    /// square its size class takes at its cell is the structure's (`FUN_6f166310`,
    /// `FUN_6f148790`: class 0 the ring of 3 from one below and left of its cell, class
    /// 1 of 4 from two, class 2 of 5 from two, class 3 of 6 from three; read: Twisted
    /// Meadows' human computer's altar's peasant, placed at cells (158.5, 196.5) below
    /// its footprint, ordered at once to build on; its farm's at (171.5, 179.5), a cell
    /// clear above its footprint, ordered to reach it first; provisional: its cell its
    /// place rounded, read only so).
    /// Whether a builder is within its repair's reach of a structure it builds
    /// (`FUN_6f41d1f0`'s distance: the range over 32 and the two sizes, in cells, from
    /// the structure's middle).
    pub(crate) fn construct_in_reach(&self, h: u32, structure: u32) -> bool {
        let (Some(u), Some(s)) = (self.unit(h), self.unit(structure)) else { return false };
        let range = self.repair_ability(h).map_or(0.0, |(a, l)| self.ability_real(a, "Rng", l));
        let r = ((range + u.collision + s.collision) / crate::pathing::CELL).max(0.49);
        (u.x - s.x).hypot(u.y - s.y) / crate::pathing::CELL <= r
    }

    pub(crate) fn touches_footprint(&self, h: u32, structure: u32) -> bool {
        let (Some(u), Some(s), Some(p)) = (self.unit(h), self.unit(structure), self.pathing.as_ref()) else { return false };
        let (off, size) = [(1, 3), (2, 4), (2, 5), (3, 6)][crate::lrpath::class_of(u.collision / crate::pathing::CELL) as usize];
        let cx = ((u.x - p.origin[0]) / crate::pathing::CELL + 0.5).floor() as i32;
        let cy = ((u.y - p.origin[1]) / crate::pathing::CELL + 0.5).floor() as i32;
        let (x0, y0) = (cx - off, cy - off);
        let mut ring = (x0..x0 + size).flat_map(|x| [(x, y0), (x, y0 + size - 1)]).chain((y0 + 1..y0 + size - 1).flat_map(|y| [(x0, y), (x0 + size - 1, y)]));
        ring.any(|c| s.pathing_cells.contains(&c))
    }

    /// An AI's peasant ordered to build on a structure it did not touch, in the second
    /// step after: in range (its repair's range over 32 and the two sizes,
    /// `FUN_6f41d1f0`), the structure's rise is made and its target task freed, and its
    /// walk ends, its path and cluster going in the next step (`Tags::walk_ended`;
    /// traced: Twisted Meadows' human computer's farm begun at 17.608, its peasant
    /// 3.54 cells from its middle tested in range then and at 17.668, the rise made
    /// at 17.668 and the task freed after its build walk's path and cluster went, its
    /// own path and cluster in the next turn; provisional: one out of range, its walk
    /// not yet read, doing as well).
    pub(crate) fn construct_step_arrive(&mut self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let Some(([task, cluster, path], steps)) = u.extra.construct_walk else { return false };
        if steps > 1 {
            if let Some(u) = self.unit_mut(h) {
                u.extra.construct_walk = Some(([task, cluster, path], steps - 1));
            }
            return true;
        }
        let target = match u.action {
            Action::Construct { target } => target,
            _ => 0,
        };
        let serial = u.extra.walk_serial;
        if self.unit(target).is_some() {
            if !self.construct_in_reach(h, target) && !self.touches_footprint(h, target) {
                log::debug!("construct walk of {h}: out of reach of {target}");
            }
            let rise: Vec<u32> = [codes::class::CONTINUOUS_MODIFIER, codes::class::THRESHOLD_LISTENER, codes::class::CONTINUOUS_MODIFIER].iter().map(|c| self.tags.take(c)).collect();
            let stamp = self.step_ticks.get(self.step_range.0).and_then(|&t| self.tick_main(t));
            if let Some(bu) = self.unit_mut(target).filter(|bu| bu.extra.build_objects.len() == 5) {
                bu.extra.build_objects.splice(2..5, rise);
                // Its rise begun anew, it is done its time left on from this step.
                if let (Some(s), Some(left)) = (stamp, bu.constructing) {
                    bu.extra.build_due = Some(f32::from_bits(crate::efloat::add((s as f32).to_bits(), left.to_bits())));
                }
            }
        }
        self.tags.release(task);
        self.tags.walk_ended([path, cluster], serial);
        if let Some(u) = self.unit_mut(h) {
            u.extra.construct_walk = None;
            u.extra.ai_order.retain(|&t| t != task);
        }
        true
    }

    /// Distance from a point to the edge of a widget: to its pathing footprint's rectangle
    /// for structures, else to its collision circle.
    pub fn edge_distance(&self, target: u32, x: f32, y: f32) -> Option<f32> {
        let tp = self.widget_pos(target)?;
        let cells = self.unit(target).map(|u| u.pathing_cells.as_slice()).unwrap_or(&[]);
        if cells.is_empty() || self.pathing.is_none() {
            let tr = if self.unit(target).is_some() { self.reach_radius(target) } else { 16.0 };
            return Some((tp.0 - x).hypot(tp.1 - y) - tr);
        }
        let p = self.pathing.as_ref().unwrap();
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for &c in cells {
            let (cx, cy) = p.center(c);
            let half = crate::pathing::CELL / 2.0;
            x0 = x0.min(cx - half);
            x1 = x1.max(cx + half);
            y0 = y0.min(cy - half);
            y1 = y1.max(cy + half);
        }
        let dx = (x0 - x).max(x - x1).max(0.0);
        let dy = (y0 - y).max(y - y1).max(0.0);
        Some(dx.hypot(dy))
    }

    /// Walks toward a widget until within `range` of its edge; true once there.
    pub(crate) fn approach(&mut self, h: u32, target: u32, range: f32, dt: f32) -> bool {
        let Some(tp) = self.widget_pos(target) else { return false };
        let u = self.unit(h).unwrap();
        let Some(edge) = self.edge_distance(target, u.x, u.y) else { return false };
        let d = edge - u.collision;
        // A unit comes no nearer a structure's footprint than one pathing cell past its
        // own size (measured: the original's workers stop 48 from a hall's or a mine's
        // footprint), so it is there once that close.
        let structure = self.unit(target).is_some_and(|t| !t.pathing_cells.is_empty());
        let range = if structure { range.max(crate::pathing::CELL) } else { range };
        if d <= range {
            let u = self.unit_mut(h).unwrap();
            u.path.clear();
            return true;
        }
        if u.path.is_empty() || u.repath_timer <= 0.0 {
            self.path_to(h, tp);
            // Already in the nearest open cell yet short of reach (a cell's half off the
            // footprint): the last step is straight at it (ours stood there for good,
            // three of five peasants mining past a standing hero idle 54 from a mine's
            // footprint, where the original's never stall).
            if structure && self.unit(h).is_some_and(|u| u.path.is_empty()) {
                if let Some(u) = self.unit_mut(h) {
                    u.path.push(tp);
                }
            }
        }
        let tag = Self::carry_tag(self.unit(h).unwrap());
        self.walk(h, dt);
        if let Some(u) = self.unit_mut(h) {
            if u.anim == "walk" && !tag.is_empty() {
                set_anim(u, &format!("walk{tag}"));
            }
        }
        false
    }

    fn harvest(&mut self, vm: &mut Vm, h: u32, target: u32, dt: f32) {
        if self.ability_with_code(h, codes::ability::ACOLYTE_HARVEST).is_some() {
            return self.acolyte_harvest(vm, h, target, dt);
        }
        if self.ability_with_code(h, codes::ability::WISP_HARVEST).is_some() {
            return self.wisp_harvest(vm, h, target, dt);
        }
        let Some((ahar, lvl)) = self.ability_with_code(h, codes::ability::HARVEST).or_else(|| self.ability_with_code(h, codes::ability::HARVEST_LUMBER)) else {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        };
        let is_mine = self.is_mine(target);
        let is_tree = self.is_tree(target);
        if !is_mine && !is_tree {
            // The source is gone: look for another tree nearby, else stop.
            let was_tree = matches!(self.handles.get(target), Some(Obj::Destructable(_)));
            let area = self.ability_real(ahar, "Area", lvl).max(0.0);
            let next = if was_tree { self.nearest_tree(h, area) } else { None };
            let u = self.unit_mut(h).unwrap();
            u.extra.chop_swinging = false;
            match next {
                Some(t) => u.action = Action::Harvest { target: t },
                None if u.carry_gold > 0 || u.carry_lumber > 0 => u.action = Action::ReturnResources,
                None => {
                    u.action = Action::Idle;
                    set_anim(u, "stand");
                }
            }
            return;
        }
        if let Some(u) = self.unit_mut(h) {
            u.harvest_source = target;
        }
        // Full: go home.
        let u = self.unit(h).unwrap();
        // DataB is the lumber capacity, DataC the gold (the editor's Har2 and Har3;
        // measured: HumanX05's Mur'gul Slaves, ANha's 20 and 10, bring 20 lumber a trip).
        let gold_cap = self.ability_real(ahar, "DataC", lvl) as i32;
        let lumber_cap = self.ability_real(ahar, "DataB", lvl) as i32;
        // Improved Lumber Harvesting (rlum) adds to a trip.
        let lumber_cap = lumber_cap + self.unit(h).map_or(0, |u| self.upgrade_bonus(u.owner, u.type_id).lumber);
        // A full worker first waits out its last swing's backswing (FUN_6f40bce0).
        let full = u.carry_lumber >= lumber_cap && u.extra.chop_recover <= 0.0;
        if (is_mine && u.carry_lumber > 0) || (is_tree && u.carry_gold > 0) || (is_tree && full) {
            self.carry_home(h);
            return;
        }
        if is_mine {
            if u.inside == target {
                let u = self.unit_mut(h).unwrap();
                u.work_timer -= dt;
                if u.work_timer <= 0.0 {
                    // Out with the gold taken on entering.
                    self.leave_mine(h);
                    let u = self.unit_mut(h).unwrap();
                    u.carry_lumber = 0;
                    self.carry_home(h);
                    self.exit_toward_drop_off(h, target);
                }
                return;
            }
            if !self.approach(h, target, 0.0 + 8.0, dt) {
                return;
            }
            // Enter if the mine has room (Agld DataC: workers inside at once).
            let (agld, mlvl) = self.ability_with_code(target, codes::ability::GOLD_MINE).unwrap();
            let capacity = self.ability_real(agld, "DataC", mlvl).max(1.0) as u32;
            let duration = self.ability_real(agld, "DataB", mlvl);
            if self.unit(target).is_some_and(|m| m.inside < capacity) {
                let owner = self.unit(h).map_or(0, |u| u.owner);
                if let Some(m) = self.unit_mut(target) {
                    m.inside += 1;
                    m.extra.mine_holder = Some((owner, f32::INFINITY));
                }
                let u = self.unit_mut(h).unwrap();
                u.inside = target;
                u.hidden = true;
                u.work_timer = duration;
                // The gold is taken on entering; a mine emptied so collapses then, its
                // worker coming out with it (measured: a mine left with 15 gold
                // collapses at 6.15 seconds under one peasant, as the second trip goes in,
                // and the peasant brings out the last 5).
                let before = self.unit(target).map_or(0, |m| m.resource);
                let take = if self.slow_chopping(owner) { 1 } else { gold_cap }.min(before);
                if let Some(m) = self.unit_mut(target) {
                    m.resource -= take;
                }
                self.gold_warning(h, before, before - take);
                self.unit_mut(h).unwrap().carry_gold = take;
                if self.unit(target).is_some_and(|m| m.resource <= 0) {
                    self.leave_mine(h);
                    let u = self.unit_mut(h).unwrap();
                    u.carry_lumber = 0;
                    self.carry_home(h);
                    self.exit_toward_drop_off(h, target);
                    kill_unit(self, vm, target, 0);
                }
            } else if let Some(u) = self.unit_mut(h) {
                set_anim(u, "stand");
            }
            return;
        }
        // Trees: Rng is the chopping range, Dur the time between swings, DataA lumber per
        // swing.  A swing lands its chop at the unit's attack damage point, and the next
        // starts Dur after the last or once that one's backswing is over, if later
        // (FUN_6f436a90 starts a swing, FUN_6f405110 chops, FUN_6f40bce0 and FUN_6f40c4a0
        // start the next; CAbilityHarvestLumber::vf32 takes both times from the attack).
        let range = self.ability_real(ahar, "Rng", lvl);
        // One that comes to the tree swings at once (FUN_6f436400; measured: the
        // original's peasant stands 10.92 seconds at the tree every trip, where ours
        // first waited out what was left of its last trip's swing).
        if !self.approach(h, target, range, dt) {
            if let Some(u) = self.unit_mut(h) {
                u.extra.chop_swinging = false;
                u.extra.chop_recover = 0.0;
                u.work_timer = 0.0;
            }
            return;
        }
        let tp = self.widget_pos(target).unwrap();
        let (point, backswing) = self.unit(h).and_then(|u| weapon(self, u.type_id)).map_or((0.0, 0.0), |w| (w.damage_point, w.backswing));
        let u = self.unit_mut(h).unwrap();
        u.extra.chop_recover -= dt;
        if u.carry_lumber >= lumber_cap {
            return;
        }
        let left = turn(u, angle_to((u.x, u.y), tp), dt);
        if left.abs() > 30.0 && !u.extra.chop_swinging {
            return;
        }
        u.work_timer -= dt;
        if u.work_timer > 0.0 {
            return;
        }
        if !u.extra.chop_swinging {
            u.extra.chop_swinging = true;
            u.work_timer = u.work_timer.max(-dt) + point;
            restart_anim(u, "attack lumber");
            if u.work_timer > 0.0 {
                return;
            }
        }
        {
            let per = self.ability_real(ahar, "DataA", lvl) as i32;
            let interval = self.ability_real(ahar, "Dur", lvl).max(0.1);
            let u = self.unit_mut(h).unwrap();
            u.extra.chop_swinging = false;
            u.extra.chop_recover = backswing;
            u.work_timer = u.work_timer.max(-dt) + (interval - point).max(backswing);
            u.carry_gold = 0;
            u.carry_lumber = (u.carry_lumber + per).min(lumber_cap.max(1));
            let owner = u.owner;
            // A chop takes as much of the tree's life as the lumber it gives (FUN_6f405110;
            // measured: a ghoul, Ahrl's 2, fells a 50-life tree in 25 chops, a peasant in 50).
            let damage = if self.slow_chopping(owner) { 0.1 } else { per as f32 };
            let mut died = false;
            if let Some(Obj::Destructable(d)) = self.handles.get_mut(target) {
                d.life -= damage;
                died = d.life <= 0.0;
            }
            if died {
                d_kill(self, vm, target);
            }
        }
    }

    pub(crate) fn nearest_tree_pub(&self, h: u32, radius: f32) -> Option<u32> {
        self.nearest_tree(h, radius)
    }

    fn nearest_tree(&self, h: u32, radius: f32) -> Option<u32> {
        let u = self.unit(h)?;
        self.destructable_order
            .iter()
            .copied()
            .filter(|&d| self.is_tree(d))
            .filter_map(|d| self.widget_pos(d).map(|p| ((p.0 - u.x).hypot(p.1 - u.y), d)))
            .filter(|(dist, _)| *dist <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }

    /// The closest friendly structure that accepts what the unit carries (Artn DataA gold,
    /// DataB lumber).
    pub(crate) fn drop_off(&self, h: u32) -> Option<u32> {
        let u = self.unit(h)?;
        let (gold, lumber) = (u.carry_gold > 0, u.carry_lumber > 0);
        let mut best: Option<(f32, u32)> = None;
        for &o in &self.unit_order {
            let Some(b) = self.unit(o) else { continue };
            if b.dead || b.owner != u.owner {
                continue;
            }
            if b.extra.uprooted {
                continue;
            }
            let Some((a, l)) = self.ability_with_code(o, codes::ability::RETURN) else { continue };
            let accepts = (gold && self.ability_real(a, "DataA", l) != 0.0) || (lumber && self.ability_real(a, "DataB", l) != 0.0);
            if !accepts {
                continue;
            }
            let d = (b.x - u.x).hypot(b.y - u.y);
            if best.is_none_or(|x| d < x.0) {
                best = Some((d, o));
            }
        }
        best.map(|b| b.1)
    }

    /// A worker turning home with what it gathered, its order "resumeharvesting" on the
    /// way (measured: a peasant's order reads resumeharvesting from the mine to the hall
    /// and harvest from the hall to the mine and inside).
    fn carry_home(&mut self, h: u32) {
        if let Some(u) = self.unit_mut(h) {
            u.action = Action::ReturnResources;
            u.order = Order { id: crate::natives::basic::order_id("resumeharvesting"), target_x: 0.0, target_y: 0.0, target: 0 };
        }
    }

    fn return_resources(&mut self, h: u32, dt: f32) {
        let Some(u) = self.unit(h) else { return };
        if u.carry_gold == 0 && u.carry_lumber == 0 {
            let src = u.harvest_source;
            let u = self.unit_mut(h).unwrap();
            u.action = if src != 0 { Action::Harvest { target: src } } else { Action::Idle };
            return;
        }
        let Some(hall) = self.drop_off(h) else {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            set_anim(u, "stand");
            return;
        };
        if !self.approach(h, hall, 8.0, dt) {
            return;
        }
        // Back to the source, unless it is gone (a mine that collapsed): then the worker
        // stands, its order done (measured: GetUnitCurrentOrder reads no order).
        let src_gone = self.unit(h).map(|u| u.harvest_source).is_some_and(|s| s != 0 && self.widget_pos(s).is_none() || self.unit(s).is_some_and(|m| m.dead));
        let u = self.unit_mut(h).unwrap();
        let (mut gold, lumber, owner, src) = (u.carry_gold, u.carry_lumber, u.owner, u.harvest_source);
        u.carry_gold = 0;
        u.carry_lumber = 0;
        u.action = if src != 0 && !src_gone { Action::Harvest { target: src } } else { Action::Idle };
        if src_gone {
            u.harvest_source = 0;
            u.order = Order::default();
        } else if src != 0 {
            // Back out to the source, its order "harvest" again (measured).
            u.order = Order { id: crate::natives::basic::order_id("harvest"), target_x: 0.0, target_y: 0.0, target: src };
        }
        // Upkeep takes its share of what is returned; the amount shows over the worker
        // (0x6f40b150).
        if gold > 0 && self.slow_chopping(owner) {
            gold = 1;
        }
        self.gather(owner, h, gold, lumber, 0.0);
    }

    /// The upkeep level from food used: how many of [Misc] UpkeepUsage's thresholds it is
    /// above (game.dll FUN_6f047f90; 40 and 70 for Reign of Chaos, 50 and 80 for The Frozen
    /// Throne when the data has none).
    pub fn upkeep_level(&self, player: u8) -> usize {
        let used = self.players.get(player as usize).map_or(0, |p| p.get_state(player_state::RESOURCE_FOOD_USED));
        self.upkeep_thresholds().iter().take_while(|&&t| t < used).count()
    }

    /// [Misc] UpkeepUsage, or the built-in levels.
    fn upkeep_thresholds(&self) -> Vec<i32> {
        let data: Vec<i32> = self.misc.get_list("Misc", "UpkeepUsage").iter().filter_map(|v| v.trim().parse().ok()).collect();
        if data.is_empty() { if self.version >= 1 { vec![50, 80] } else { vec![40, 70] } } else { data }
    }

    /// The upkeep levels the resource bar's tooltip lists (FUN_6f3c99d0): for each, the
    /// food it covers, from and to (at most FoodCeiling), and the percent of gold and of
    /// lumber income kept, until a level covers nothing new.
    pub fn upkeep_levels(&self) -> Vec<(i32, i32, usize, i32, i32)> {
        let ceiling = self.misc.get_str("Misc", "FoodCeiling").and_then(|v| v.trim().parse::<i32>().ok()).map_or(if self.version >= 1 { 100 } else { 90 }, |v| v.clamp(1, 300));
        let thresholds = self.upkeep_thresholds();
        let mut out = Vec::new();
        let mut last = -1;
        for level in 0..10 {
            // The built-in table's further levels are 10000.
            let to = thresholds.get(level).copied().unwrap_or(10000).min(ceiling);
            if to == 0 || to == last {
                break;
            }
            let (gold, lumber) = self.upkeep_tax_at(level);
            out.push((last + 1, to, level, 100 - gold, 100 - lumber));
            last = to;
        }
        out
    }

    /// Percent of gold and lumber income upkeep takes ([Misc] UpkeepGoldTax and
    /// UpkeepLumberTax by level; 0, 30, 60 and none without data: FUN_6f0016f0,
    /// FUN_6f001880).
    pub fn upkeep_tax(&self, player: u8) -> (i32, i32) {
        self.upkeep_tax_at(self.upkeep_level(player))
    }

    /// Upkeep's percent of gold and lumber income at a level.
    pub fn upkeep_tax_at(&self, level: usize) -> (i32, i32) {
        let level = level.min(9);
        let tax = |key: &str, default: &[f32]| -> i32 {
            let data: Vec<f32> = self.misc.get_list("Misc", key).iter().filter_map(|v| v.trim().parse().ok()).collect();
            let v = if data.is_empty() { default.get(level).or(default.last()).copied().unwrap_or(0.0) } else { data.get(level).or(data.last()).copied().unwrap_or(0.0) };
            (v * 100.0).round() as i32
        };
        (tax("UpkeepGoldTax", &[0.0, 0.3, 0.6]), tax("UpkeepLumberTax", &[0.0]))
    }

    pub fn primary_attribute_pub(&self, h: u32) -> f32 {
        self.primary_attribute(h)
    }

    /// The primary attribute's share of the attack from the hero's own attributes,
    /// without what items and abilities add.
    pub fn primary_attribute_base(&self, h: u32) -> f32 {
        let Some(u) = self.unit(h) else { return 0.0 };
        if !u.is_hero {
            return 0.0;
        }
        match self.objects.units.get(u.type_id).map(|o| o.str("Primary").to_ascii_uppercase()).as_deref() {
            Some("STR") => u.str_.floor() * self.misc_f32("StrAttackBonus", 1.0),
            Some("AGI") => u.agi.floor(),
            Some("INT") => u.int.floor(),
            _ => 0.0,
        }
    }

    /// A hero's primary attribute, which adds to its attack damage.
    fn primary_attribute(&self, h: u32) -> f32 {
        let Some(u) = self.unit(h) else { return 0.0 };
        if !u.is_hero {
            return 0.0;
        }
        let b = u.item_bonus;
        match self.objects.units.get(u.type_id).map(|o| o.str("Primary").to_ascii_uppercase()).as_deref() {
            Some("STR") => (u.str_ + b.str_).floor() * self.misc_f32("StrAttackBonus", 1.0),
            Some("AGI") => (u.agi + b.agi).floor(),
            Some("INT") => (u.int + b.int).floor(),
            _ => 0.0,
        }
    }

    /// The structures' damage points falling due before `until` (an agent due, seconds
    /// since the game began; with none, all this tick's), taken among the movement steps
    /// in the order they fall due: the damage point is a timer on the agent queue
    /// (CAbilityAttack +0x1ec, event 0xd01b1 → `FUN_6f49a5b0` → `FUN_6f4946b0`, started
    /// with the attack by `FUN_6f49d130`), as are the steps, and each main tick runs that
    /// queue's events in turn (`FUN_6f04c1a0`, `FUN_6f054050`).  Read: Mafarazzo TD's
    /// Plague Tower's missile, its damage point at 13.804, made after a turn's first step
    /// and before the third (13.859), in which its runner's walk ended.  The rest are
    /// the units' logic's (`World::update_units`).  Provisional: only a swing begun
    /// before the tick; and the other units' and a walking ancient's are not yet so.
    pub(crate) fn points_before(&mut self, vm: &mut Vm, until: Option<f64>) {
        let dt = self.tick_dt;
        let mut due: Vec<(f64, u32)> = Vec::new();
        for &h in &self.unit_order {
            if !self.attacks_in_place(h) || self.buff_mods(h).4 {
                continue;
            }
            let Some(u) = self.unit(h).filter(|u| !u.dead && !u.paused && u.constructing.is_none()) else { continue };
            let Some(s) = u.swing.as_ref().filter(|s| !s.done && !s.landed) else { continue };
            let Some(wp) = weapon_numbered(self, u, s.weapon) else { continue };
            // (As the units' logic will find it at the tick's end.)
            let speed = self.attack_speed(h);
            let (_, point) = swing_point(&wp, true, speed);
            let elapsed = s.elapsed + dt * speed;
            if elapsed < point - STEP_SLACK {
                continue;
            }
            let at = self.time_exact - ((elapsed - point).max(0.0) / speed) as f64;
            if until.is_none_or(|t| at < t) {
                due.push((at, h));
            }
        }
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        let landed = !due.is_empty();
        for (at, h) in due {
            let Some(u) = self.unit(h) else { continue };
            let Some((s, wp)) = u.swing.clone().and_then(|s| weapon_numbered(self, u, s.weapon).map(|wp| (s, wp))) else { continue };
            if let Some(s) = self.unit_mut(h).and_then(|u| u.swing.as_mut()) {
                s.landed = true;
            }
            self.swing_lands(vm, h, &s, &wp, at);
        }
        // (What falls due after them waits for those left, `World::first_missile_point`.)
        if landed {
            self.steps_free_until = self.first_missile_point(dt);
        }
    }

    /// A swing's damage point: an attack on the ground sends its missile at the point;
    /// one on a unit strikes it or sends its missile.  False when the attacker is gone or
    /// died of it.
    fn swing_lands(&mut self, vm: &mut Vm, h: u32, s: &Swing, wp: &Weapon, at: f64) -> bool {
        // The missiles landing earlier in the tick land first (read: the Plague Tower's
        // second, landing 0.016 before its third is sent, lets its `Atrg` go, and the
        // third makes another).
        self.fly_bullets(vm, at);
        // (What fell due before then is gone by then: read, that `Atrg` freed before the
        // third missile's objects are made.)
        // (Not past a laid buff's end, which goes first, `World::update_buffs`.)
        let now = self.bullet_clock(at);
        let hold = self.laid_buff_end(self.bullet_clock(self.time_exact - self.tick_dt as f64), now);
        self.tags.free_timed(hold.map_or(now, |e| now.min(e)));
        // (A structure's damage point is an event on the agent queue, `at` its due: what
        // its main tick's start has let go is gone, as an earlier main tick ended,
        // `World::free_as_tick_ends`; traced, the upgrade game's take-up's block at turn
        // 507, its slot the h00X's shell's in a main tick after.)
        if hold.is_none() && self.attacks_in_place(h) {
            let kept = ((at - self.clock_wraps as f64 * 300.0) as f32).to_bits();
            if let Some(c) = self.agent_tick(kept).and_then(|t| self.tick_main(t)) {
                self.tags.free_timed(c);
            }
        }
        if self.unit(h).is_none_or(|u| u.dead) {
            return false;
        }
        if s.ground.is_some() {
            let (owner, tid) = self.unit(h).map_or((0, [0; 4]), |u| (u.owner, u.type_id));
            let ub = self.upgrade_bonus(owner, tid);
            let bonus = self.unit(h).map_or(0.0, |u| u.item_bonus.damage) + ub.damage;
            let dice = wp.dice + ub.dice;
            let raw = wp.base + self.attack_dice(dice, wp.sides) + self.primary_attribute(h) + bonus;
            if let Some(src) = self.unit(h).cloned() {
                let launch_z = self.objects.units.get(src.type_id).map_or(60.0, |o| o.real("launchZ"));
                let (class, _) = self.bullet_class(h, 0, wp);
                let bullet = Some(self.bullet_sent(h, 0, class, None, at));
                self.missiles.push(Missile {
                    id: 0,
                    source: h,
                    target: 0,
                    model: wp.missile.clone(),
                    x: src.x,
                    y: src.y,
                    z: self.ground_z(src.x, src.y) + launch_z,
                    speed: wp.missile_speed.max(1.0),
                    arc: wp.missile_arc,
                    damage: raw,
                    attack_type: wp.attack_type.clone(),
                    start: (src.x, src.y),
                    spell: None,
                    impact: s.ground,
                    weapon: Some(wp.clone()),
                    bounces: 0,
                    hit: Vec::new(),
                    stampede: None,
                    rock: None,
                    reflected: false,
                    bullet,
                });
            }
            return true;
        }
        let (owner, tid) = self.unit(h).map_or((0, [0; 4]), |u| (u.owner, u.type_id));
        let ub = self.upgrade_bonus(owner, tid);
        let bonus = self.unit(h).map_or(0.0, |u| u.item_bonus.damage) + ub.damage;
        let dice = wp.dice + ub.dice;
        let raw = wp.base + self.attack_dice(dice, wp.sides) + self.primary_attribute(h) + bonus;
        let raw = self.attack_modifiers(vm, h, s.target, raw);
        self.attack_passives(vm, h, s.target);
        self.club_swing(h);
        if self.unit(h).is_none_or(|u| u.dead) {
            return false;
        }
        self.pillage_ready(h, s.target);
        // (A line artillery's too: its shell, `B-Al`, CArtilleryLine, flies to where its
        // target stood; traced, the upgrade game's h00X's first at turn 507, its shell's
        // objects made at its damage point.)
        let artillery = wp.weapon_type == "artillery" || wp.weapon_type == "aline";
        if wp.weapon_type == "missile" || artillery || wp.weapon_type.starts_with("msplash") || wp.weapon_type == "mbounce" || wp.weapon_type == "mline" {
            if let (Some(src), Some(_)) = (self.unit(h).cloned(), self.unit(s.target)) {
                // Lightning Attack: a weapon firing the ability's missile also
                // strikes with its lightning, DataA after for DataB.
                if let Some((a, lvl)) = self.ability_with_code(h, codes::ability::LIGHTNING_ATTACK) {
                    let lvl = lvl.max(1) as u32;
                    if self.profile_art(a, "Missileart").eq_ignore_ascii_case(&wp.missile) {
                        let (delay, life) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl));
                        let codes = self.objects.abilities.get(a).map(|o| o.list("LightningEffect")).unwrap_or_default();
                        if let Some(code) = codes.first().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()) {
                            self.timed_bolt(&code, h, s.target, delay, life.max(0.01));
                        }
                    }
                }
                let launch_z = self.objects.units.get(src.type_id).map_or(60.0, |o| o.real("launchZ"));
                let main = self.missiles.len();
                self.missiles.push(Missile {
                    id: 0,
                    source: h,
                    target: s.target,
                    model: wp.missile.clone(),
                    x: src.x,
                    y: src.y,
                    z: self.ground_z(src.x, src.y) + launch_z,
                    speed: wp.missile_speed.max(1.0),
                    arc: wp.missile_arc,
                    damage: raw,
                    attack_type: wp.attack_type.clone(),
                    start: (src.x, src.y),
                    spell: None,
                    // Artillery lands where the target stood when it fired.
                    impact: (wp.weapon_type == "artillery" || wp.weapon_type == "aline").then(|| self.widget_pos(s.target)).flatten(),
                    weapon: Some(wp.clone()),
                    // targCount targets, and more with Moon Glaive (ratc).
                    bounces: if wp.weapon_type == "mbounce" { wp.target_count - 1 + self.upgrade_bonus(src.owner, src.type_id).targets } else { 0 },
                    hit: Vec::new(),
                    stampede: None,
                    rock: None,
                    reflected: false,
                    bullet: None,
                });
                // Barrage (Aroc): an attack on an air unit sends rockets at the
                // other air enemies within Area too, DataC in all, each with the
                // attack's damage (measured: a Siege Engine's every 2 seconds
                // strike three Gryphon Riders alike, 28 each, then 26 each).
                let barrage = self.ability_with_code(h, codes::ability::ROCKET_ATTACK).filter(|&(a, _)| self.ability_enabled(src.owner, a));
                if let Some((a, lvl)) = barrage.filter(|_| self.unit(s.target).is_some_and(|t| self.objects.units.get(t.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly")))) {
                    let lvl = lvl.max(1) as u32;
                    let (area, max) = (self.data(a, "Area", lvl), self.data(a, "DataC", lvl).max(1.0) as usize);
                    let mut others: Vec<u32> = self.enemies_in(src.owner, src.x, src.y, area).into_iter().filter(|&t| t != s.target && self.unit(t).is_some_and(|tu| !tu.is_building && self.objects.units.get(tu.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly")))).collect();
                    others.sort_by(|&p, &q| {
                        let d = |t: u32| self.unit(t).map_or(f32::MAX, |u| (u.x - src.x).hypot(u.y - src.y));
                        d(p).total_cmp(&d(q))
                    });
                    // (Each a CMissile of its own, sent by the ability's hook before the
                    // attack's own is made: CAbilityBarrage vf84 `FUN_6f58f700`,
                    // `FUN_6f6d6920`, `FUN_6f6d3ec0`.)
                    for t in others.into_iter().take(max.saturating_sub(1)) {
                        let mut m = self.missiles.last().cloned().unwrap();
                        m.target = t;
                        m.impact = None;
                        m.bullet = Some(self.bullet_sent(h, t, codes::class::MISSILE, None, at));
                        self.missiles.push(m);
                    }
                }
                let (class, ability) = self.bullet_class(h, s.target, wp);
                let mut bullet = self.bullet_sent(h, s.target, class, ability, at);
                bullet.missed = self.high_ground_miss(h, s.target);
                self.missiles[main].bullet = Some(bullet);
                // A structure's is aimed where it meets its target (`World::bullet_aimed`;
                // provisional: the other units' still fly the port's way).
                if wp.weapon_type != "artillery" && self.attacks_in_place(h) {
                    let mut m = self.missiles[main].clone();
                    self.bullet_aimed(&mut m, at);
                    self.missiles[main] = m;
                }
            }
        } else {
            // Melee impacts sound by weapon (weapType1) and target armor
            // (UnitCombatSounds: MetalMediumSlice + Flesh).
            let weap = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map(|o| o.str("weapType1")).unwrap_or_default();
            let armor = self.unit(s.target).and_then(|t| self.objects.units.get(t.type_id)).map(|o| o.str("armor")).unwrap_or_default();
            if !weap.is_empty() && !weap.eq_ignore_ascii_case("_") {
                if let Some(t) = self.unit(s.target) {
                    let (x, y) = (t.x, t.y);
                    let z = self.ground_z(x, y);
                    self.requests.push(Request::WorldSound { label: format!("{weap}{armor}"), x, y, z });
                }
            }
            let at = self.widget_pos(s.target).unwrap_or_default();
            // An instant ranged attack Defend deflects strikes its source.
            let deflected = if wp.weapon_type == "instant" { self.defend_deflects(s.target, h, &wp.attack_type) } else { None };
            if let Some(taken) = deflected {
                if taken > 0.0 {
                    self.deal_damage(vm, h, s.target, raw * taken, &wp.attack_type);
                }
                self.deal_damage(vm, s.target, h, raw, &wp.attack_type);
            } else {
                self.attack_impact(vm, h, s.target, raw, &wp, at);
            }
        }
        true
    }

    /// A quick weapon's next attack, started as its cooldown runs out within the step
    /// (the cooldown's end tries the attack again, `FUN_6f49a4f0` and `FUN_6f495180`),
    /// what ran past the end carried into the new cooldown and swing.  None when the unit
    /// would not strike its target again at once, left to the step's usual course: it
    /// has to turn or close in, its target is gone, or it takes a better one.
    fn next_quick_swing(&mut self, h: u32, s: &Swing, speed: f32, dt: f32) -> Option<Swing> {
        let u = self.unit(h)?;
        if u.cooldown > STEP_SLACK || u.extra.reaction > 0.0 {
            return None;
        }
        // On a target it took up itself it looks again each second, as between swings
        // (the look's timer runs on beside the attack's, `FUN_6f493ff0`).  A structure's
        // comes by its timer whatever its swing (`World::structure_look`).
        let own = matches!(u.action, Action::Attack { target } if target == s.target) && s.ground.is_none() && u.extra.acquired == s.target && !(u.is_building && !u.extra.uprooted);
        if own && u.acquire_timer <= 0.0 && self.unit(s.target).is_some_and(|t| !t.dead) {
            let (range, creeps) = (u.acquire_range, u.owner == PLAYER_NEUTRAL_AGGRESSIVE);
            let better = self.find_target_ex(h, range, creeps).filter(|&t| t != s.target);
            let u = self.unit_mut(h)?;
            u.acquire_timer = u.acquire_timer.max(-dt) + 1.0;
            if let Some(t) = better {
                u.action = Action::Attack { target: t };
                self.acquired(h, t);
                return None;
            }
        }
        let u = self.unit(h)?;
        let half = 0.5f32.to_degrees();
        let facing = |to: (f32, f32)| u.is_building || angle_diff(u.facing, angle_to((u.x, u.y), to)).abs() <= half;
        let (wn, wp) = match (&u.action, s.ground) {
            (Action::AttackGround { x, y }, Some(at)) if (*x, *y) == at => {
                let (wn, wp) = unit_weapons(self, u).into_iter().find(|(_, w)| w.weapon_type == "artillery" || w.weapon_type == "aline")?;
                if (at.0 - u.x).hypot(at.1 - u.y) - u.collision > wp.range || !facing(at) {
                    return None;
                }
                (wn, wp)
            }
            (Action::Attack { target }, None) if *target == s.target => {
                let t = self.unit(s.target).filter(|t| !t.dead)?;
                if u.extra.acquired == s.target && self.cloaked(h) {
                    return None;
                }
                let (wn, wp) = weapon_for(self, u, t)?;
                let reach = wp.range + self.upgrade_bonus(u.owner, u.type_id).range;
                // (A structure's try judges a walker where it is then, and gives up one
                // out of reach, `crate::tasks`.)
                let at = self.time_exact + u.cooldown.min(0.0) as f64;
                let engine = if u.is_building && !u.extra.uprooted { self.reaches_walker(h, s.target, reach, at) } else { None };
                let out = engine.map_or_else(|| (t.x - u.x).hypot(t.y - u.y) - u.collision - self.reach_radius(s.target) > reach, |r| !r);
                if out && u.is_building && !u.extra.uprooted && !u.extra.attack_tasks.is_empty() {
                    self.attack_given_up(h, at);
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                    }
                    return None;
                }
                if out || !facing((t.x, t.y)) {
                    return None;
                }
                (wn, wp)
            }
            _ => return None,
        };
        if wn != s.weapon {
            return None;
        }
        let u = self.unit_mut(h).unwrap();
        let carry = u.cooldown.min(0.0);
        u.cooldown = wp.cooldown / speed + carry;
        u.extra.cooldown_speed = Some(speed);
        restart_anim(u, "attack");
        if s.ground.is_none() {
            self.queue_unit_event(s.target, EVENT_PLAYER_UNIT_ATTACKED, EVENT_UNIT_ATTACKED, EventCtx { other_unit: h, ..Default::default() });
        }
        Some(Swing { target: s.target, ground: s.ground, elapsed: -carry * speed, done: false, weapon: wn, landed: false })
    }

    /// Moves into range of `target` and starts a swing when ready.
    /// An attack on the ground: into range of the point, turned to it, then a swing
    /// whenever the cooldown allows; the order stands until replaced.
    fn attack_ground_step(&mut self, h: u32, at: (f32, f32), dt: f32) {
        let Some(u) = self.unit(h) else { return };
        let Some((wn, wp)) = unit_weapons(self, u).into_iter().find(|(_, w)| w.weapon_type == "artillery" || w.weapon_type == "aline") else {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        };
        let d = (at.0 - u.x).hypot(at.1 - u.y) - u.collision;
        if d > wp.range {
            if u.path.is_empty() || u.repath_timer <= 0.0 {
                self.path_to(h, at);
            }
            self.walk(h, dt);
            return;
        }
        let speed = self.attack_speed(h);
        let u = self.unit_mut(h).unwrap();
        u.path.clear();
        let left = if u.is_building { 0.0 } else { turn(u, angle_to((u.x, u.y), at), dt) };
        if left.abs() > 0.5f32.to_degrees() {
            return;
        }
        if u.cooldown <= STEP_SLACK {
            u.cooldown = wp.cooldown / speed + u.cooldown.min(0.0);
            u.extra.cooldown_speed = Some(speed);
            u.swing = Some(Swing { target: 0, ground: Some(at), elapsed: 0.0, done: false, weapon: wn, landed: false });
            restart_anim(u, "attack");
        } else if u.anim == "walk" {
            set_anim(u, "stand");
        }
    }

    /// The factor a unit's attacks are quickened by: agility for a hero
    /// (AgiAttackSpeedBonus per point), items, upgrades, and buffs.
    pub(crate) fn attack_speed(&self, h: u32) -> f32 {
        let Some(u) = self.unit(h) else { return 1.0 };
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        let agi = self.misc_f32("AgiAttackSpeedBonus", 0.02);
        let base = if u.is_hero { 1.0 + (u.agi + u.item_bonus.agi).floor() * agi } else { 1.0 };
        // A burrow attacks the faster for each worker inside, its cooldown over one more
        // than their count (measured: a Burrow, cooldown 4, with one, two, and four
        // peons fires every 2.0, 1.33, and 0.8 seconds).
        let crew = if self.ability_with_code(h, codes::ability::CARGO_HOLD_BURROW).is_some() { (u.extra.cargo.len() + 1) as f32 } else { 1.0 };
        ((base + u.item_bonus.attack_speed + ub.attack_speed) * self.buff_mods(h).1 * crew).max(0.01)
    }

    fn try_attack(&mut self, h: u32, target: u32, dt: f32, stationary: bool) {
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return };
        let Some((wn, wp)) = weapon_for(self, u, t) else { return };
        let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - self.reach_radius(target);
        let (tpos, upos) = ((t.x, t.y), (u.x, u.y));
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        // A structure's first attack on a target it took up waits its first try
        // (`crate::tasks`).
        let first = u.extra.first_swing_at;
        if first.is_some_and(|s| s > self.time_exact + 1e-6) {
            return;
        }
        // (Its try judges a walker where it is then.)
        let engine = if u.is_building && !u.extra.uprooted && u.cooldown <= STEP_SLACK && !stationary {
            let at = first.unwrap_or(self.time_exact + u.cooldown.min(0.0) as f64);
            self.reaches_walker(h, target, wp.range + ub.range, at)
        } else {
            None
        };
        if engine.map_or(d > wp.range + ub.range, |r| !r) {
            // A structure does not walk, whatever its speed, unless an uprooted ancient
            // (Mafarazzo TD's Towers of Vengence, made from the Ancient Protector, speed
            // 40, without its Root): it gives up a target it took up as its try finds it
            // out of reach (`FUN_6f495180`; its look takes up none past it).
            if stationary || (u.is_building && !u.extra.uprooted) {
                if !stationary && !u.extra.attack_tasks.is_empty() && u.cooldown <= STEP_SLACK {
                    let at = first.unwrap_or(self.time_exact + u.cooldown.min(0.0) as f64);
                    self.attack_given_up(h, at);
                    if let Some(u) = self.unit_mut(h) {
                        u.action = Action::Idle;
                        set_anim(u, "stand");
                    }
                }
                return;
            }
            if u.path.is_empty() || u.repath_timer <= 0.0 {
                self.path_to(h, tpos);
            }
            self.walk(h, dt);
            return;
        }
        let speed = self.attack_speed(h);
        let now = self.time_exact;
        let u = self.unit_mut(h).unwrap();
        u.path.clear();
        // Units attack once facing within AttackHalfAngle radians of the target;
        // structures turn not at all.
        let left = if u.is_building { 0.0 } else { turn(u, angle_to(upos, tpos), dt) };
        let half = 0.5f32.to_degrees();
        if left.abs() > half {
            return;
        }
        if u.cooldown <= STEP_SLACK {
            // The swing begins as the cooldown runs out (`FUN_6f49a4f0`), or for one ready
            // since an earlier tick three main ticks into this one (provisional), or for
            // a structure's first on a target it took up its first try's wait after its
            // look (`crate::tasks`; read under winedbg: Mafarazzo TD's Plague Tower's
            // attack start, `FUN_6f49d130`, at the main clock's 0.215 as it took up its
            // target, then at 0.315, 0.635, 0.955, its missiles' dues from 0.614999).
            let early = if u.cooldown >= 0.0 { dt - SWING_START_IN_TICK } else { -u.cooldown };
            // (A structure's first, at its first try, or as its cooldown runs out after.)
            let early = match u.extra.first_swing_at.take() {
                Some(s) => ((now - s) as f32).min(if u.cooldown < 0.0 { -u.cooldown } else { f32::MAX }).max(0.0),
                None => early,
            };
            u.cooldown = wp.cooldown / speed - early;
            u.extra.cooldown_speed = Some(speed);
            u.swing = Some(Swing { target, ground: None, elapsed: early * speed, done: false, weapon: wn, landed: false });
            restart_anim(u, "attack");
            // The target is attacked (GetAttacker is the attacker).
            self.queue_unit_event(target, EVENT_PLAYER_UNIT_ATTACKED, EVENT_UNIT_ATTACKED, EventCtx { other_unit: h, ..Default::default() });
        } else if u.anim == "walk" {
            set_anim(u, "stand");
        }
    }

    /// An idle armed unit hurt by an enemy, by an attack or a spell, attacks it back: hurt
    /// by a spell, its order an attack (measured: a footman hurt by a spell from an
    /// Archmage 600 away walks at it ordered to attack, even an invulnerable one); by an
    /// attack, its order left as it was, as for a target taken up on its own (measured:
    /// a footman shot by an archer walks at it, its order reading none).
    /// A unit attacking what it took up on its own, a target not fighting, turns on its
    /// attacker the same way (measured: a Rifleman shooting a paused footman beside it
    /// turns on an archer that shoots it, at its next shot; one shooting an archer that
    /// shoots back keeps to it when a second archer joins in).
    /// An idle unit without a weapon runs from an enemy that hurts it, of any player
    /// (CAbilityMove's damage hook, `FUN_6f5fa400`: a unit without an order,
    /// `FUN_6f419fb0`, and without an attack or with it disabled, `FUN_6f49cf60`; traced:
    /// Mafarazzo TD's fort runner, made idle without a weapon by a script, struck by a
    /// Plague tower, ordered at once to walk off and out of its reach).
    pub(crate) fn retaliate(&mut self, target: u32, source: u32, ordered: bool) {
        let Some(t) = self.unit(target) else { return };
        let idle = matches!(t.action, Action::Idle);
        let unordered = t.order.id == 0 && matches!(t.action, Action::Attack { target: c } if c != source && !self.attacking(c));
        if idle && unit_weapons(self, t).is_empty() && !t.is_building && !t.paused {
            if self.unit(source).is_some_and(|s| !s.dead && !is_ally(self, t.owner, s.owner)) {
                self.flee_from(target, source);
            }
            return;
        }
        if !(idle || unordered) || unit_weapons(self, t).is_empty() || t.is_building || t.paused {
            return;
        }
        let Some(s) = self.unit(source) else { return };
        if is_ally(self, t.owner, s.owner) || s.dead {
            return;
        }
        // Nor at an attacker it cannot hit (measured: an ogre struck by an invulnerable
        // footman takes no attack order; ours stood forever ordered to attack it).
        let flying = self.move_kind(source) == crate::pathing::Move::Fly;
        let hits = unit_weapons(self, t).iter().any(|(_, wp)| {
            let wants = |k: &str| wp.targets.split(',').any(|x| x.trim() == k);
            if flying { wants("air") } else if s.is_building { wants("structure") || wants("ground") } else { wants("ground") }
        });
        if !hits || self.unit_invulnerable(s) {
            self.flee_from(target, source);
            return;
        }
        let (sx, sy) = (s.x, s.y);
        let u = self.unit_mut(target).unwrap();
        u.action = Action::Attack { target: source };
        if ordered {
            u.order = Order { id: crate::natives::basic::order_id("attack"), target_x: sx, target_y: sy, target: source };
        }
    }

    /// A unit runs from an enemy that hurt it instead of answering (`FUN_6f5fc350`), if
    /// it may flee (UnitData canFlee), straight away from it (along its facing were they
    /// at one point).  One its owner's AI has taken (a computer's or a neutral player's,
    /// its flag 4 at +0x5c set as the AI takes it, `FUN_6f9b9230`) moves off the
    /// attacker's reach and 100 more (300 from an attacker without a weapon, or for a Hero
    /// below 200 or a third of its most), and then its AI looks at it again
    /// (`FUN_6f693450` queues the order 0xd0006 after its move, `World::ai_unit_idle`); any
    /// other goes 300, a fighter attack-moving and a worker moving.  Measured: a footman
    /// struck by a gryphon rider it cannot hit, from 200 or 400 away, goes 291.6 off on
    /// an attack order each time, and a peasant 284.5 on a move order, each stopping
    /// within 15.6875 of a point 300 off; an ogre struck by an invulnerable footman,
    /// reach 90, goes 177 a time, and by a rifleman, 400, 485.
    pub(crate) fn flee_from(&mut self, target: u32, source: u32) {
        let (Some(t), Some(s)) = (self.unit(target), self.unit(source)) else { return };
        if !self.objects.units.get(t.type_id).is_some_and(|o| o.int("canFlee") != 0) {
            return;
        }
        let (sx, sy, tx, ty) = (s.x, s.y, t.x, t.y);
        let taken = self.ai_takes(t.owner);
        let theirs = unit_weapons(self, s);
        let low_hero = t.is_hero && t.extra.illusion.is_none() && (t.life < 200.0 || t.life < t.max_life / 3.0);
        let away = if taken && !low_hero && !theirs.is_empty() { theirs.iter().map(|(_, w)| w.range).fold(0.0, f32::max) + 100.0 } else { 300.0 };
        let d = (tx - sx).hypot(ty - sy);
        let (nx, ny) = if d > 0.001 { ((tx - sx) / d, (ty - sy) / d) } else { (t.facing.to_radians().cos(), t.facing.to_radians().sin()) };
        let (fx, fy) = self.clamp_to_map(tx + nx * away, ty + ny * away);
        let walks = taken || self.unit_class(t, "peon");
        let name = if walks { "move" } else { "attack" };
        let u = self.unit_mut(target).unwrap();
        u.action = if walks { Action::Move { x: fx, y: fy } } else { Action::AttackMove { x: fx, y: fy } };
        u.order = Order { id: crate::natives::basic::order_id(name), target_x: fx, target_y: fy, target: 0 };
        u.path.clear();
        u.extra.order_queue.clear();
        if taken {
            u.extra.order_queue.push_back((crate::aiposts::AI_IDLE_ORDER, fx, fy, 0));
            if walks && self.pathing.is_some() && self.move_kind(target) == crate::pathing::Move::Foot {
                self.flee_walk(target);
            }
        }
    }

    /// Whether a player's AI takes its units (`FUN_6f9b9230`: an AI player, +0x2d4): a
    /// computer's, or a neutral player's.
    pub(crate) fn ai_takes(&self, p: u8) -> bool {
        p >= PLAYER_NEUTRAL_AGGRESSIVE || self.players.get(p as usize).is_some_and(|pl| pl.controller == map_control::COMPUTER)
    }

    /// Pushes overlapping units apart, flyers among flyers and the rest among
    /// themselves; units unseen, aboard, with their pathing off, or ghosts take no part,
    /// nor a worker ghosting on its run (its region's 0x02 clear, `Unit::region_flags`;
    /// read: Twisted Meadows' human computer's farm peasant, standing by its hall at
    /// (171.0388, 187.4737) from 57.4466 to 60.1 as the mine's peasant came home past
    /// it at 58.406, where the port's was pushed to (171.21, 187.63)).
    fn separate_units(&mut self, _dt: f32) {
        let hs: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && !u.is_building && u.collision > 0.0 && !u.extra.no_pathing && !self.is_ghost(u) && u.region_flags() & 0x02 != 0))
            .collect();
        let pos: Vec<(f32, f32, f32)> = hs.iter().map(|&h| self.unit(h).map(|u| (u.x, u.y, u.collision)).unwrap()).collect();
        let air: Vec<bool> = hs.iter().map(|&h| self.move_kind(h) == crate::pathing::Move::Fly).collect();
        let mut push = vec![(0.0f32, 0.0f32); hs.len()];
        for i in 0..hs.len() {
            for j in i + 1..hs.len() {
                if air[i] != air[j] {
                    continue;
                }
                let (a, b) = (pos[i], pos[j]);
                let (mut dx, mut dy) = (b.0 - a.0, b.1 - a.1);
                // Two on the very same spot part along a way fixed by their order.
                if dx.abs() < 0.001 && dy.abs() < 0.001 {
                    let ang = (i as f32 * 2.399963 + j as f32) % std::f32::consts::TAU;
                    dx = ang.cos() * 0.01;
                    dy = ang.sin() * 0.01;
                }
                let d = (dx * dx + dy * dy).sqrt();
                // Each keeps about its size from the other (measured: footmen, size 31,
                // made on one spot stand 59 to 71 apart, and Human01's bandits stay 61
                // apart).
                let min = (a.2 + b.2) * 0.9;
                if d < min && d > 0.0 {
                    let o = (min - d) * 0.25;
                    push[i].0 -= dx / d * o;
                    push[i].1 -= dy / d * o;
                    push[j].0 += dx / d * o;
                    push[j].1 += dy / d * o;
                }
            }
        }
        for (i, &h) in hs.iter().enumerate() {
            let (px, py) = push[i];
            if px == 0.0 && py == 0.0 {
                continue;
            }
            let kind = if air[i] { crate::pathing::Move::Fly } else { self.move_kind(h) };
            let fits = |w: &World, x: f32, y: f32| w.pathing.as_ref().is_none_or(|p| p.passable(p.cell_of(x, y), kind));
            let (x, y) = self.unit(h).map(|u| (u.x + px, u.y + py)).unwrap();
            if fits(self, x, y) {
                let u = self.unit_mut(h).unwrap();
                u.x = x;
                u.y = y;
            }
        }
    }

    fn update_missiles(&mut self, vm: &mut Vm, dt: f32) {
        for m in self.missiles.iter_mut().filter(|m| m.id == 0) {
            self.missile_serial += 1;
            m.id = self.missile_serial;
        }
        self.update_stampede(vm, dt);
        let end = self.time_exact;
        let clock = self.bullet_clock(end);
        self.reveals_ended(clock);
        self.fly_bullets(vm, end);
        let mut hits = Vec::new();
        let mut i = 0;
        while i < self.missiles.len() {
            let m = &mut self.missiles[i];
            if m.stampede.is_some() || m.rock.is_some() || m.bullet.is_some() {
                i += 1;
                continue;
            }
            // A spell's missile flies to its target's middle (measured: a Storm Bolt at
            // 1000 lands on footmen 300 and 600 away 0.31 and 0.61 seconds after leaving).
            let spell = m.spell.is_some();
            let target = match self.handles.get(m.target) {
                Some(Obj::Unit(t)) => Some((t.x, t.y, if spell { 0.0 } else { t.collision })),
                _ if m.target == 0 => m.impact.map(|(x, y)| (x, y, 0.0)),
                _ => None,
            };
            let Some((tx, ty, edge)) = target else {
                self.missiles.swap_remove(i);
                continue;
            };
            let (dx, dy) = (tx - m.x, ty - m.y);
            let d = (dx * dx + dy * dy).sqrt();
            let step = m.speed * dt;
            if d <= step + edge {
                let m = self.missiles.swap_remove(i);
                hits.push(m);
                continue;
            }
            m.x += dx / d * step;
            m.y += dy / d * step;
            i += 1;
        }
        self.missiles_land(vm, hits);
    }

    /// The attacks' missiles flown on to game time `t` of this tick, those reaching their
    /// targets by then landing in the order they land.  An attack's missile at a unit
    /// lands on reaching its edge (measured: an archer's arrow, at 900, strikes a footman
    /// 500 away 0.51 seconds after leaving, its 469 to the footman's collision of 31);
    /// it flies from the moment it was sent, its target taken to move evenly through
    /// the tick (`World::bullet_flies`).  Among the steps (a damage point's,
    /// `World::points_before`) only those landing by `t` are flown, each target where
    /// its velocity takes it by then (`World::place_now`), the others left to be flown
    /// after the steps.
    pub(crate) fn fly_bullets(&mut self, vm: &mut Vm, t: f64) {
        let end = self.time_exact;
        let among = !self.after_steps;
        let at = if among { self.main_tick_clock(t) } else { None };
        let clock = std::mem::replace(&mut self.script_clock, at);
        let mut places: Vec<Option<(f32, f32)>> = self.missiles.iter().map(|m| if among { self.place_now(m.target) } else { None }).collect();
        self.script_clock = clock;
        // (The tick's main clocks, and the main clock's lead on the game's time.)
        let ticks: Vec<f64> = (0..self.tick_clocks.1.len()).filter_map(|i| self.tick_abs(i)).collect();
        let off = ticks.last().map_or(0.0, |c| c - end);
        let mut hits = Vec::new();
        let mut i = 0;
        while i < self.missiles.len() {
            let place = places[i];
            let m = &mut self.missiles[i];
            let Some(b) = m.bullet.as_ref().filter(|_| m.stampede.is_none() && m.rock.is_none()) else {
                i += 1;
                continue;
            };
            // (A line artillery's shell flies to where its target stood, as it was sent:
            // traced, the upgrade game's h00X's shell sent at 50.607, landing in turn
            // 512, its 1132 at 2200, where the port's had flown at the creep closing on
            // it and landed in 511.  Provisional: an artillery's own, which the port
            // flies at its target, not yet read.)
            let line = m.weapon.as_ref().is_some_and(|w| w.weapon_type == "aline");
            let target = match self.handles.get(m.target) {
                Some(Obj::Unit(_)) if line && m.impact.is_some() => m.impact.map(|(x, y)| (x, y, 0.0)),
                Some(Obj::Unit(u)) => Some(place.map_or((u.x, u.y, u.collision), |p| (p.0, p.1, u.collision))),
                _ if m.target == 0 => m.impact.map(|(x, y)| (x, y, 0.0)),
                _ => None,
            };
            // (One aimed where it meets its target lands as its flight falls due, in the
            // first main tick at or past it, `World::bullet_aimed`.)
            if let Some(due) = b.due.filter(|_| target.is_some()) {
                let landing = ticks.iter().copied().find(|&c| c >= due - 1e-7);
                match landing.map(|c| c - off) {
                    Some(at) if at <= t + 1e-9 => {
                        hits.push(self.missiles.swap_remove(i));
                        places.swap_remove(i);
                    }
                    _ => {
                        // (Shown on its way there.)
                        let (sent, aim) = (b.at, b.seen.unwrap_or((m.x, m.y)));
                        let f = if due - off > sent + 1e-9 { ((t - sent) / (due - off - sent)).clamp(0.0, 1.0) as f32 } else { 1.0 };
                        m.x = m.start.0 + (aim.0 - m.start.0) * f;
                        m.y = m.start.1 + (aim.1 - m.start.1) * f;
                        i += 1;
                    }
                }
                continue;
            }
            if among {
                if let Some((tx, ty, edge)) = target {
                    let mut c = m.clone();
                    if World::bullet_flies(&mut c, (tx, ty), edge, t).is_some() {
                        self.missiles.swap_remove(i);
                        places.swap_remove(i);
                        hits.push(c);
                        continue;
                    }
                }
                i += 1;
                continue;
            }
            let Some((tx, ty, edge)) = target else {
                // (Provisional: an attack's missile whose target is gone ends where it
                // is, its objects as at a landing.)
                let m = self.missiles.swap_remove(i);
                places.swap_remove(i);
                if let Some(mut b) = m.bullet {
                    b.at = t;
                    self.bullet_landed(b, m.target);
                }
                continue;
            };
            // Its target at `t`, on its way from where it was last seen to where it is now.
            let t0 = b.at.clamp(end - 1.0, end);
            let seen = b.seen.unwrap_or((tx, ty));
            let f = if end - t0 > 1e-9 { ((t - t0) / (end - t0)).clamp(0.0, 1.0) as f32 } else { 1.0 };
            let to = (seen.0 + (tx - seen.0) * f, seen.1 + (ty - seen.1) * f);
            if World::bullet_flies(m, to, edge, t).is_some() {
                hits.push(self.missiles.swap_remove(i));
                places.swap_remove(i);
            } else {
                i += 1;
            }
        }
        // (One aimed so is as of its landing's main tick, `World::bullet_aimed`.)
        for m in hits.iter_mut() {
            if let Some(b) = m.bullet.as_mut().filter(|b| b.due.is_some()) {
                let due = b.due.unwrap_or(0.0);
                b.at = ticks.iter().copied().find(|&c| c >= due - 1e-7).unwrap_or(due) - off + 2e-7;
            }
        }
        hits.sort_by(|p, q| p.bullet.as_ref().map_or(t, |b| b.at).total_cmp(&q.bullet.as_ref().map_or(t, |b| b.at)));
        self.missiles_land(vm, hits);
    }

    /// Missiles reaching their targets: a spell's takes hold, an attack's strikes.
    fn missiles_land(&mut self, vm: &mut Vm, hits: Vec<Missile>) {
        for mut m in hits {
            let mut bullet = m.bullet.take();
            match m.spell {
                Some((a, lvl, _, _)) if crate::ability::spell_flies(&self.ability_code(a)) => {
                    let at = self.widget_pos(m.target).or(m.impact).unwrap_or((m.x, m.y));
                    self.spell_landed(vm, m.source, a, lvl, m.target, at);
                }
                Some((a, lvl, stun, buff)) => {
                    // The touch of a spell aimed at an enemy, as it lands.
                    let enemy = self.unit(m.target).is_some_and(|t| self.unit(m.source).is_some_and(|c| !is_ally(self, t.owner, c.owner)));
                    if enemy {
                        self.spell_damage(vm, m.source, m.target, 0.0);
                    }
                    if self.ability_code(a) == codes::ability::DEATH_KNIGHT_DEATH_COIL {
                        if self.is_undead(m.target) {
                            self.magic_heal(m.target, m.damage);
                        } else {
                            self.spell_damage(vm, m.source, m.target, m.damage / 2.0);
                        }
                        continue;
                    }
                    self.spell_damage(vm, m.source, m.target, m.damage);
                    if self.ability_code(a) == codes::ability::PHOENIX_FIRE && self.unit(m.target).is_some_and(|t| !t.dead) {
                        self.phoenix_burn(m.source, m.target, a, lvl, buff);
                    }
                    if stun > 0.0 {
                        self.apply_buff(m.target, Buff { id: buff, ability: a, level: lvl, caster: m.source, remaining: stun, stun: true, ..Default::default() });
                    }
                }
                None => match &m.weapon {
                    Some(wp) => {
                        let at = m.impact.or_else(|| self.widget_pos(m.target)).unwrap_or((m.x, m.y));
                        // Defend may send the attack back at its source.
                        if m.impact.is_none() && !m.reflected {
                            if let Some(taken) = self.defend_deflects(m.target, m.source, &wp.attack_type) {
                                if taken > 0.0 {
                                    self.deal_damage(vm, m.source, m.target, m.damage * taken, &wp.attack_type);
                                }
                                let z = self.unit(m.target).map_or(m.z, |u| self.ground_z(u.x, u.y) + 60.0);
                                // (Provisional: the same missile sent back, its count on its
                                // target let go.)
                                if let Some(b) = bullet.as_mut() {
                                    self.bullet_uncounted(b, m.target);
                                }
                                let bullet = bullet.take().map(|b| crate::bullets::Bullet { seen: self.unit(m.source).map(|t| (t.x, t.y)), ..b });
                                self.missiles.push(Missile { id: 0, source: m.target, target: m.source, x: at.0, y: at.1, z, start: at, bounces: 0, hit: Vec::new(), impact: None, reflected: true, bullet, ..m.clone() });
                                continue;
                            }
                        }
                        // An ability's missile lets its count on its target go as it
                        // lands, after its effect and before its damage (CBuffMissile vf47
                        // `FUN_6f6cf940`).
                        if let Some(b) = bullet.as_mut() {
                            self.bullet_uncounted(b, m.target);
                        }
                        // Shot from below at a target on higher ground, it may miss
                        // (ChanceToMiss): the target takes nothing and any splash only
                        // MissDamageReduction of the damage (FUN_6f6d48b0, FUN_6f6d2530).
                        let missed = match bullet.as_ref() {
                            Some(b) => b.missed,
                            None => self.high_ground_miss(m.source, m.target),
                        };
                        // (One aimed where it meets its target lands in the first main
                        // tick its flight's due reaches, `World::bullet_aimed`.)
                        self.landing_clock = bullet.as_ref().map(|b| match b.due {
                            Some(d) => (0..self.tick_clocks.1.len()).filter_map(|i| self.tick_abs(i)).find(|&c| c >= d - 1e-7).map_or(self.bullet_clock(b.at), |c| c + 2e-7),
                            None => self.bullet_clock(b.at),
                        });
                        if let Some(b) = bullet.as_mut() {
                            b.due = None;
                        }
                        log::trace!("bullet of {} lands on {} at {:?} (main clock {:?})", m.source, m.target, bullet.as_ref().map(|b| b.at), self.landing_clock);
                        if missed {
                            self.show_miss(m.source);
                            let reduced = m.damage * self.misc_f32("MissDamageReduction", 0.5);
                            self.attack_impact_missed(vm, m.source, m.target, reduced, wp, at);
                        } else {
                            self.attack_impact(vm, m.source, m.target, m.damage, wp, at);
                        }
                        self.landing_clock = None;
                        // A splash's landing shows its attacker (`World::splash_revealed`).
                        let splash = matches!(wp.weapon_type.as_str(), "artillery" | "aline") || wp.weapon_type.starts_with("msplash");
                        if let (true, Some(b)) = (splash, bullet.as_ref()) {
                            self.splash_revealed(m.source, m.target, b.at);
                        }
                        // A bounce goes on to the nearest unit within Farea it may hit that it
                        // has not (AllowMultiBounce 0), losing damageLoss (CMissileBounce).
                        if m.bounces > 0 {
                            let mut hit = m.hit.clone();
                            hit.push(m.target);
                            let owner = self.unit(m.source).map_or(0, |u| u.owner);
                            let next = self
                                .unit_order
                                .iter()
                                .copied()
                                .filter(|u| !hit.contains(u))
                                .filter(|&u| self.unit(u).is_some_and(|t| (t.x - at.0).hypot(t.y - at.1) <= wp.full_area && self.splash_allows(m.source, &wp.splash_targets, t) && self.unit_visible_to(u, owner)))
                                .min_by(|&p, &q| {
                                    let d = |u: u32| self.unit(u).map_or(f32::MAX, |t| (t.x - at.0).hypot(t.y - at.1));
                                    d(p).total_cmp(&d(q))
                                });
                            // (The same missile flies on: CMissileBounce vf47
                            // `FUN_6f6cfbc0`, its objects kept.)
                            if let Some(n) = next {
                                let z = self.ground_z(at.0, at.1) + 60.0;
                                if bullet.is_some() {
                                    self.bounce_marked(n);
                                }
                                let bullet = bullet.take().map(|b| crate::bullets::Bullet { seen: self.unit(n).map(|t| (t.x, t.y)), ..b });
                                self.missiles.push(Missile { id: 0, target: n, x: at.0, y: at.1, z, start: at, damage: m.damage * (1.0 - wp.damage_loss), bounces: m.bounces - 1, hit, impact: None, bullet, ..m.clone() });
                            }
                        }
                    }
                    None => self.deal_damage(vm, m.source, m.target, m.damage, &m.attack_type),
                },
            }
            if let Some(b) = bullet {
                self.bullet_landed(b, m.target);
            }
        }
    }

    /// Whether a missile attack misses for its target standing on higher ground: a roll
    /// under ChanceToMiss, with the attacker on a lower cliff level than its target.
    /// It is decided as the missile is sent, on the attack's own generator (stream 2:
    /// `FUN_6f6d48b0` from the launch, `FUN_6f6d3470`, an artillery's `FUN_6f6d2530`).
    /// (Provisional: the target's and the attacker's own misses first, Evasion's and
    /// Curse's kin, `FUN_6f6d6cc0` and `FUN_6f6cd570`, are not read; the port's are
    /// elsewhere.)
    fn high_ground_miss(&mut self, source: u32, target: u32) -> bool {
        let (Some(s), Some(t)) = (self.unit(source), self.unit(target)) else { return false };
        let ((sx, sy), (tx, ty)) = ((s.x, s.y), (t.x, t.y));
        let chance = self.misc_f32("ChanceToMiss", 0.25);
        if self.rand_data.streams[crate::rng::stream::ATTACK].real(0.0, 1.0) >= chance {
            return false;
        }
        let Some(th) = self.terrain_heights.as_ref() else { return false };
        th.level_at(sx, sy) < th.level_at(tx, ty)
    }

    /// An attack's dice (`FUN_6f498960`, `FUN_6f695c70`, `FUN_6f1ec180`), as its damage
    /// point comes: on the attack's own generator (stream 2), each die a draw `r`, `r ×
    /// sides >> 32`, and one.  (Provisional: more than 16 dice drawn so too, where the
    /// original draws once from a normal spread, `FUN_6f1d12e0`.)
    fn attack_dice(&mut self, dice: i32, sides: i32) -> f32 {
        let r = &mut self.rand_data.streams[crate::rng::stream::ATTACK];
        (0..dice.max(0)).map(|_| ((r.next_u32() as u64 * sides.max(0) as u64) >> 32) as f32 + 1.0).sum()
    }

    /// "miss" over an attacker whose attack missed, for whoever sees it (FUN_6f6d6350):
    /// MISS in [Misc] Miss text colors.
    pub fn show_miss(&mut self, attacker: u32) {
        let Some((x, y, hidden)) = self.unit(attacker).map(|u| (u.x, u.y, u.hidden)) else { return };
        if hidden || self.fog_state(x, y, self.local_player) != crate::fog::FOG_VISIBLE {
            return;
        }
        let text = self.localized("MISS");
        let tag = self.engine_text_tag("Miss", text, x, y, self.ground_z(x, y));
        self.local_text_tags.push(tag);
    }

    /// A missed missile attack lands: its splash only, without the target.
    fn attack_impact_missed(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, wp: &Weapon, at: (f32, f32)) {
        let area = if wp.weapon_type == "mbounce" { 0.0 } else { wp.full_area.max(wp.half_area).max(wp.quarter_area) };
        if area <= 0.0 {
            return;
        }
        for u in self.units_in_pub(at.0, at.1, area + 64.0) {
            if u == target {
                continue;
            }
            let Some(t) = self.unit(u) else { continue };
            if !self.splash_allows(source, &wp.splash_targets, t) {
                continue;
            }
            let d = ((t.x - at.0).hypot(t.y - at.1) - t.collision).max(0.0);
            let f = if d <= wp.full_area {
                1.0
            } else if d <= wp.half_area {
                wp.half_factor
            } else if d <= wp.quarter_area {
                wp.quarter_factor
            } else {
                0.0
            };
            if f > 0.0 {
                self.deal_damage(vm, source, u, raw * f, &wp.attack_type);
            }
        }
    }

    /// An attack lands: the target takes the damage (artillery hits only what is
    /// where it lands), then splash and line spill spread it.
    fn attack_impact(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, wp: &Weapon, at: (f32, f32)) {
        let artillery = wp.weapon_type == "artillery" || wp.weapon_type == "aline";
        // Bladestorming, the hero's attacks strike for nothing (measured: a Blademaster's
        // hits show damage events of 0 while it spins).
        let raw = if self.unit(source).is_some_and(|u| u.buffs.iter().any(|b| self.ability_code(b.ability) == codes::ability::BLADE_MASTER_BLADESTORM)) { 0.0 } else { raw };
        // (A slow it lays takes the walker's speed at once, `World::slowed_among_steps`.)
        let before = (self.current_move_speed(target), self.buff_mods(target).0 < 1.0 - 1e-6);
        self.poison_on_hit(vm, source, target);
        if let Some(at) = self.landing_clock.filter(|_| self.current_move_speed(target) < before.0) {
            self.slowed_among_steps(target, at, before.1);
        }
        if !artillery {
            self.deal_damage(vm, source, target, raw, &wp.attack_type);
        }
        // A bouncing weapon's Farea is how far it looks for its next target, not splash.
        let area = if wp.weapon_type == "mbounce" { 0.0 } else { wp.full_area.max(wp.half_area).max(wp.quarter_area) };
        // (What an artillery's splash kills bursts: traced, the upgrade game's five creeps
        // killed by the h00X's line shell at turn 512, each with a burst's course.)
        self.artillery_strike = artillery;
        if area > 0.0 {
            for u in self.units_in_pub(at.0, at.1, area + 64.0) {
                if !artillery && u == target {
                    continue;
                }
                let Some(t) = self.unit(u) else { continue };
                if !self.splash_allows(source, &wp.splash_targets, t) {
                    continue;
                }
                let d = ((t.x - at.0).hypot(t.y - at.1) - t.collision).max(0.0);
                let f = if d <= wp.full_area {
                    1.0
                } else if d <= wp.half_area {
                    wp.half_factor
                } else if d <= wp.quarter_area {
                    wp.quarter_factor
                } else {
                    0.0
                };
                if f > 0.0 {
                    self.deal_damage(vm, source, u, raw * f, &wp.attack_type);
                }
            }
            self.artillery_strike = false;
            // Trees and other destructables the splash takes in (a catapult's).
            let classes: Vec<&str> = wp.splash_targets.split(',').map(|s| s.trim()).collect();
            if classes.iter().any(|c| matches!(*c, "tree" | "debris" | "wall")) {
                let dests: Vec<(u32, f32)> = self
                    .handles
                    .iter()
                    .filter_map(|(h, o)| match o {
                        Obj::Destructable(d) if d.life > 0.0 && !d.invulnerable && d.visible => Some((h, (d.x - at.0).hypot(d.y - at.1), d.type_id)),
                        _ => None,
                    })
                    .filter(|&(_, d, id)| {
                        d <= area && {
                            let t = self.objects.destructables.get(id).map(|o| o.str("targType").to_ascii_lowercase()).unwrap_or_default();
                            t.split(',').any(|c| classes.contains(&c.trim()))
                        }
                    })
                    .map(|(h, d, _)| (*h, d))
                    .collect();
                for (h, d) in dests {
                    let f = if d <= wp.full_area {
                        1.0
                    } else if d <= wp.half_area {
                        wp.half_factor
                    } else if d <= wp.quarter_area {
                        wp.quarter_factor
                    } else {
                        0.0
                    };
                    if f <= 0.0 {
                        continue;
                    }
                    let dead = match self.handles.get_mut(h) {
                        Some(Obj::Destructable(x)) => {
                            x.life -= raw * f;
                            x.life <= 0.0
                        }
                        _ => false,
                    };
                    if dead {
                        crate::natives::objects::kill_destructable_pub(self, vm, h);
                    }
                }
            }
        }
        // Plague Toss: the shell leaves a Plague Ward (UnitID) for Dur.
        if artillery {
            if let Some((a, lvl)) = self.ability_with_code(source, codes::ability::PLAGUE_TOSS) {
                let owner = self.unit(source).map_or(0, |u| u.owner);
                if self.ability_enabled(owner, a) {
                    let lvl = lvl.max(1) as u32;
                    let ward = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                    let dur = self.data(a, "Dur", lvl);
                    self.summon(source, owner, ward, 1, at.0, at.1, dur);
                }
            }
        }
        // Burning Oil: the shell sets the ground alight for Dur.
        if artillery {
            if let Some((a, lvl)) = self.ability_with_code(source, codes::ability::BALLS_OF_FIRE) {
                let owner = self.unit(source).map_or(0, |u| u.owner);
                if self.ability_enabled(owner, a) {
                    let lvl = lvl.max(1) as u32;
                    let (duration, interval) = (self.data(a, "Dur", lvl), self.data(a, "DataB", lvl).max(0.05));
                    self.ground_effects.push(GroundEffect { caster: source, owner, ability: a, level: lvl, x: at.0, y: at.1, elapsed: 0.0, duration, interval, next: 0.0, effect: 0 });
                }
            }
        }
        // Flak Cannons and Fragmentation Shards: DataC damage within Area, DataD within
        // DataA, and DataE within DataB, to what their targets allow.
        for code in [codes::ability::FLAK_CANNON, codes::ability::FRAG_SHARDS] {
            let Some((a, lvl)) = self.ability_with_code(source, code) else { continue };
            let owner = self.unit(source).map_or(0, |u| u.owner);
            if !self.ability_enabled(owner, a) {
                continue;
            }
            let lvl = lvl.max(1) as u32;
            let d = |k: &str| self.data(a, k, lvl);
            let (full_r, mid_r, small_r, full, mid, small) = (d("Area"), d("DataA"), d("DataB"), d("DataC"), d("DataD"), d("DataE"));
            let targs = self.objects.abilities.get(a).map(|o| o.level_str("targs", lvl).to_ascii_lowercase()).unwrap_or_default();
            if !self.unit(target).is_some_and(|t| self.splash_allows(source, &targs, t)) && !artillery {
                continue;
            }
            for u in self.units_in_pub(at.0, at.1, full_r.max(mid_r).max(small_r) + 64.0) {
                let Some(t) = self.unit(u) else { continue };
                if !self.splash_allows(source, &targs, t) {
                    continue;
                }
                let dist = ((t.x - at.0).hypot(t.y - at.1) - t.collision).max(0.0);
                let amount = if dist <= full_r {
                    full
                } else if dist <= mid_r {
                    mid
                } else if dist <= small_r {
                    small
                } else {
                    0.0
                };
                if amount > 0.0 {
                    self.deal_damage(vm, source, u, amount, &wp.attack_type);
                }
            }
        }
        // Line spill: on past the target (Storm Hammers' and Vorpal Blades' rasd).
        let spill = wp.spill_dist + self.unit(source).map_or(0.0, |s| self.upgrade_bonus(s.owner, s.type_id).spill_dist);
        if (wp.weapon_type == "mline" || wp.weapon_type == "aline") && spill > 0.0 {
            let Some(from) = self.unit(source).map(|s| (s.x, s.y)) else { return };
            let (dx, dy) = (at.0 - from.0, at.1 - from.1);
            let len = dx.hypot(dy).max(1.0);
            let dir = (dx / len, dy / len);
            for u in self.units_in_pub(at.0 + dir.0 * spill / 2.0, at.1 + dir.1 * spill / 2.0, spill / 2.0 + wp.spill_radius + 64.0) {
                if u == target {
                    continue;
                }
                let Some(t) = self.unit(u) else { continue };
                if !self.splash_allows(source, &wp.splash_targets, t) {
                    continue;
                }
                let (rx, ry) = (t.x - at.0, t.y - at.1);
                let along = rx * dir.0 + ry * dir.1;
                let across = (rx * dir.1 - ry * dir.0).abs();
                if along >= 0.0 && along <= spill && across <= wp.spill_radius + t.collision {
                    self.deal_damage(vm, source, u, raw * (1.0 - wp.damage_loss), &wp.attack_type);
                }
            }
        }
    }

    /// Whether a weapon's splashTargs take in a unit: its classes (ground, air,
    /// structure) and, when any is named, its allegiance to the attacker.
    fn splash_allows(&self, source: u32, targets: &str, t: &Unit) -> bool {
        let flags: Vec<String> = targets.split(',').map(crate::targets::flag_name).filter(|s| !s.is_empty() && s != "_").collect();
        if flags.is_empty() || t.dead || t.hidden {
            return false;
        }
        let has = |f: &str| flags.iter().any(|x| x == f);
        let air = self.objects.units.get(t.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
        let class_ok = if t.is_building { has("structure") } else if air { has("air") } else { has("ground") };
        if !class_ok {
            return false;
        }
        let Some(owner) = self.unit(source).map(|s| s.owner) else { return true };
        // notself names every owner, the source itself aside (0x1e000: player, ally,
        // neutral, enemy; FUN_6f6a9350's table).
        if has("notself") {
            return t.handle != source;
        }
        let named = ["enemy", "friend", "ally", "player", "neutral", "self"].iter().any(|f| has(f));
        if !named {
            return true;
        }
        if t.owner == owner {
            return has("player") || has("friend");
        }
        if crate::natives::players::is_ally(self, owner, t.owner) {
            return has("ally") || has("friend");
        }
        if t.owner >= PLAYER_NEUTRAL_AGGRESSIVE {
            return has("neutral") || has("enemy");
        }
        has("enemy")
    }
}

/// The cells a walker of a size class moves into on its way from cell `from` to cell
/// `to`, by the way's direction (`FUN_6f167940`: 2 east, 8 west, 4 north, 1 south): its
/// footprint's leading edge at `to` (`FUN_6f149560`, `FUN_6f1497c0`; larger classes,
/// `FUN_6f149b00` and `FUN_6f149e60`, taken as class 1's).
fn edge_cells(class: u8, from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let mut dir = if to.0 < from.0 { 8 } else if from.0 < to.0 { 2 } else { 0 };
    if to.1 < from.1 {
        dir |= 1;
    } else if from.1 < to.1 {
        dir |= 4;
    }
    let (x, y) = to;
    let row = |x0: i32, y: i32, n: i32| (x0..x0 + n).map(move |x| (x, y));
    let col = |x: i32, y0: i32, n: i32| (y0..y0 + n).map(move |y| (x, y));
    if class == 0 {
        let mut v = vec![(x, y)];
        match dir {
            3 => v.extend([(x - 1, y), (x, y + 1)]),
            6 => v.extend([(x - 1, y), (x, y - 1)]),
            9 => v.extend([(x + 1, y), (x, y + 1)]),
            12 => v.extend([(x + 1, y), (x, y - 1)]),
            _ => {}
        }
        return v;
    }
    match dir {
        1 => row(x - 1, y - 1, 2).collect(),
        2 => col(x, y - 1, 2).collect(),
        3 => row(x - 2, y - 1, 3).chain(col(x, y, 2)).collect(),
        4 => row(x - 1, y, 2).collect(),
        6 => row(x - 2, y, 3).chain(col(x, y - 2, 2)).collect(),
        8 => col(x - 1, y - 1, 2).collect(),
        9 => row(x - 1, y - 1, 3).chain(col(x - 1, y, 2)).collect(),
        12 => row(x - 1, y, 3).chain(col(x - 1, y - 2, 2)).collect(),
        _ => (y - 1..y + 1).flat_map(|y| (x - 1..x + 1).map(move |x| (x, y))).collect(),
    }
}

/// A vector scaled to length 1 when longer (`FUN_6f168280`, engine floats).
fn unit_dir(dx: u32, dy: u32) -> (u32, u32) {
    use crate::efloat as ef;
    let len = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
    if f32::from_bits(len) > 1.0 {
        (ef::div(dx, len), ef::div(dy, len))
    } else {
        (dx, dy)
    }
}

/// Whether a walker of a size class could go straight from `from` to `to` (in cells,
/// `FUN_6f168d30`): at each whole cell's distance along the line short of its end, the
/// leading edge of its footprint in the cell reached is open (`FUN_6f149630` for class
/// 1), the edge's direction from the cell last tested (from cell 0, 0 at first: the
/// original drops the start cell it works out).
fn line_clear(from: (u32, u32), to: (u32, u32), class: u8, open: &impl Fn((i32, i32)) -> bool) -> bool {
    use crate::efloat as ef;
    let (dx, dy) = (ef::sub(to.0, from.0), ef::sub(to.1, from.1));
    let len = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
    let (ux, uy) = unit_dir(dx, dy);
    let one = 1.0f32.to_bits();
    let mut t = one;
    let mut prev = (0, 0);
    while f32::from_bits(t) < f32::from_bits(len) {
        let (px, py) = (ef::add(from.0, ef::mul(ux, t)), ef::add(from.1, ef::mul(uy, t)));
        let c = (f32::from_bits(px).floor() as i32, f32::from_bits(py).floor() as i32);
        if c != prev {
            if !edge_cells(class, prev, c).into_iter().all(open) {
                return false;
            }
            prev = c;
        }
        t = ef::add(t, one);
    }
    true
}
