//! Unit behavior: orders, movement, target acquisition, attacks, missiles, and death.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::natives::players::is_ally;
use crate::natives::units::kill_unit;

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
#[derive(Clone, Debug)]
pub struct Missile {
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
}

/// Weapon 1 of a unit type, from UnitWeapons.slk and the unit's profile.
#[derive(Clone, Debug, Default)]
pub struct Weapon {
    pub range: f32,
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
}

pub fn weapon(w: &World, type_id: Id) -> Option<Weapon> {
    let o = w.objects.units.get(type_id)?;
    // weapsOn: bit 0 enables weapon 1.
    if o.int("weapsOn") & 1 == 0 {
        return None;
    }
    let missile = o.list("Missileart").first().cloned().unwrap_or_default();
    Some(Weapon {
        range: o.real("rangeN1"),
        cooldown: o.real("cool1"),
        damage_point: o.real("dmgpt1"),
        backswing: o.real("backSw1"),
        dice: o.int("dice1"),
        sides: o.int("sides1"),
        base: o.real("dmgplus1"),
        attack_type: o.str("atkType1").to_ascii_lowercase(),
        weapon_type: o.str("weapTp1").to_ascii_lowercase(),
        targets: o.str("targs1").to_ascii_lowercase(),
        missile,
        missile_speed: o.list("Missilespeed").first().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0),
        missile_arc: o.list("Missilearc").first().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0),
    })
}

fn misc_list(w: &World, key: &str) -> Vec<f32> {
    w.misc.get_list("Misc", key).iter().filter_map(|v| v.trim().parse().ok()).collect()
}

/// Damage after attack type, armor type, and armor value (Units\MiscGame.txt
/// DamageBonus* tables and DefenseArmor).
pub fn final_damage(w: &World, raw: f32, attack_type: &str, target: &Unit) -> f32 {
    let o = w.objects.units.get(target.type_id);
    let def_type = o.map(|o| o.str("defType").to_ascii_lowercase()).unwrap_or_default();
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
    let armor = if target.is_hero {
        armor + target.agi.floor() * w.misc_f32("AgiDefenseBonus", 0.3) + w.misc_f32("AgiDefenseBase", -2.0)
    } else {
        armor
    };
    let k = w.misc_f32("DefenseArmor", 0.06);
    let armor_factor = if armor >= 0.0 { 1.0 - (armor * k) / (1.0 + armor * k) } else { 2.0 - (1.0 - k).powf(-armor) };
    raw * mult * armor_factor
}

fn can_target(w: &World, wp: &Weapon, attacker: &Unit, t: &Unit) -> bool {
    if t.dead || t.hidden || t.invulnerable || t.handle == attacker.handle {
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

fn set_anim(u: &mut Unit, name: &str) {
    if u.anim != name {
        u.anim = name.to_string();
        u.anim_serial = u.anim_serial.wrapping_add(1);
    }
}

fn restart_anim(u: &mut Unit, name: &str) {
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
    // Turn rate: the data's turnRate scaled to a full revolution per second.  The exact
    // scale game.dll applies to this field (stored at unit-type offset 0x74) is pending.
    let rate = (u.turn_speed.max(0.05) * 360.0) * dt;
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
    pub fn order_unit(&mut self, h: u32, order: i32, x: f32, y: f32, target: u32) {
        let smart = crate::natives::basic::order_id("smart");
        let mv = crate::natives::basic::order_id("move");
        let attack = crate::natives::basic::order_id("attack");
        let stop = crate::natives::basic::order_id("stop");
        let hold = crate::natives::basic::order_id("holdposition");
        let patrol = crate::natives::basic::order_id("patrol");
        let owner_of = |w: &World, t: u32| w.unit(t).map(|u| u.owner);
        let me = match self.unit(h) {
            Some(u) if !u.dead && !u.is_building => u.owner,
            Some(u) if !u.dead && u.is_building && (order == smart) => {
                // Structures take a smart order as their rally point.
                let p = self.widget_pos(target).unwrap_or((x, y));
                let _ = u;
                self.unit_mut(h).unwrap().rally = Some(p);
                return;
            }
            _ => return,
        };
        let action = if order == stop {
            Action::Idle
        } else if order == hold {
            Action::Hold
        } else if order == attack && target != 0 {
            Action::Attack { target }
        } else if order == attack {
            Action::AttackMove { x, y }
        } else if order == patrol {
            let p = self.unit(h).map(|u| (u.x, u.y)).unwrap_or_default();
            Action::Patrol { a: p, b: (x, y) }
        } else if (order == smart || order == crate::natives::basic::order_id("harvest")) && target != 0 && self.can_harvest(h, target) {
            Action::Harvest { target }
        } else if order == crate::natives::basic::order_id("returnresources") {
            Action::ReturnResources
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
        } else if let Some(a) = self.ability_for_order(h, order) {
            if self.targeting(a) == crate::ability::Targeting::Passive {
                return;
            }
            Action::Cast { ability: a, target, x, y }
        } else {
            return;
        };
        self.leave_mine(h);
        if let Some(u) = self.unit_mut(h) {
            u.action = action;
            u.path.clear();
            u.repath_timer = 0.0;
            u.order = Order { id: order, target_x: x, target_y: y, target };
        }
    }

    pub fn widget_pos(&self, h: u32) -> Option<(f32, f32)> {
        match self.handles.get(h) {
            Some(Obj::Unit(u)) => Some((u.x, u.y)),
            Some(Obj::Destructable(d)) => Some((d.x, d.y)),
            Some(Obj::Item(i)) => Some((i.x, i.y)),
            _ => None,
        }
    }

    pub(crate) fn path_to(&mut self, h: u32, goal: (f32, f32)) {
        let Some(u) = self.unit(h) else { return };
        let (from, radius) = ((u.x, u.y), u.collision);
        let path = match &self.pathing {
            Some(p) => p.find_path(from, goal, radius * 0.5, 20000),
            None => vec![goal],
        };
        if let Some(u) = self.unit_mut(h) {
            u.path = path;
            u.repath_timer = 1.0;
        }
    }

    /// Steps along the unit's path; returns true when it has arrived.
    pub(crate) fn walk(&mut self, h: u32, dt: f32) -> bool {
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
        let step = u.move_speed * dt;
        let step = step * self.buff_mods(h).0;
        let Some(u) = self.unit_mut(h) else { return true };
        let (dx, dy) = (next.0 - u.x, next.1 - u.y);
        let d = (dx * dx + dy * dy).sqrt();
        set_anim(u, "walk");
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

    fn find_target(&self, h: u32, range: f32) -> Option<u32> {
        let u = self.unit(h)?;
        let wp = weapon(self, u.type_id)?;
        let mut best: Option<(f32, u32)> = None;
        for &t in &self.unit_order {
            let Some(tu) = self.unit(t) else { continue };
            if is_ally(self, u.owner, tu.owner) || !can_target(self, &wp, u, tu) {
                continue;
            }
            // Neutral passive units are never acquired automatically.
            if tu.owner == PLAYER_NEUTRAL_PASSIVE {
                continue;
            }
            let d = (tu.x - u.x).hypot(tu.y - u.y);
            if d <= range && best.is_none_or(|b| d < b.0) {
                best = Some((d, t));
            }
        }
        best.map(|b| b.1)
    }

    fn deal_damage(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, attack_type: &str) {
        let Some(t) = self.unit(target) else { return };
        if t.dead || t.invulnerable || self.buff_mods(target).5 {
            return;
        }
        if self.evades(target) {
            return;
        }
        let raw = self.on_attack_passives(source, target, raw * self.buff_mods(source).3);
        let Some(t) = self.unit(target) else { return };
        let dmg = final_damage(self, raw, attack_type, t);
        let owner = t.owner;
        let t = self.unit_mut(target).unwrap();
        t.life -= dmg;
        let dead = t.life <= 0.405;
        let ctx = EventCtx { unit: target, other_unit: source, damage: dmg, event_id: EVENT_UNIT_DAMAGED, player: owner, has_player: true, ..Default::default() };
        self.fire(vm, ctx, |_, k| matches!(k, EventKind::Unit { unit, event } if *unit == target && *event == EVENT_UNIT_DAMAGED));
        if dead {
            kill_unit(self, vm, target, source);
        } else if let Some(t) = self.unit(target) {
            // Retaliate when idle.
            if matches!(t.action, Action::Idle) && weapon(self, t.type_id).is_some() && !t.is_building {
                if let Some(s) = self.unit(source) {
                    if !is_ally(self, t.owner, s.owner) {
                        self.unit_mut(target).unwrap().action = Action::Attack { target: source };
                    }
                }
            }
        }
    }

    /// Advances every unit by `dt` seconds.
    pub fn update_units(&mut self, vm: &mut Vm, dt: f32) {
        let units = self.unit_order.clone();
        let reaction = self.misc_f32("ReactionDelay", 0.25);
        for h in units {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                let decay = if u.is_building { self.misc_f32("StructureDecayTime", 30.0) } else { self.misc_f32("BoneDecayTime", 88.0) };
                let t = self.time - u.death_time;
                if t > decay {
                    crate::natives::units::remove_unit(self, h);
                }
                continue;
            }
            if u.paused {
                continue;
            }
            if u.is_building {
                self.update_structure(vm, h, dt);
                continue;
            }
            // Stunned units do nothing; an attack in progress is lost.
            if self.buff_mods(h).4 {
                let u = self.unit_mut(h).unwrap();
                u.swing = None;
                u.casting = None;
                continue;
            }
            let action = u.action.clone();
            let wp = weapon(self, u.type_id);
            {
                let u = self.unit_mut(h).unwrap();
                u.cooldown = (u.cooldown - dt).max(0.0);
                u.repath_timer -= dt;
                u.acquire_timer -= dt;
            }
            // Attack swing in progress.
            if let Some(mut s) = self.unit(h).and_then(|u| u.swing.clone()) {
                let wp = wp.clone().unwrap_or_default();
                s.elapsed += dt;
                if !s.done && s.elapsed >= wp.damage_point {
                    s.done = true;
                    let raw = wp.base + (0..wp.dice.max(0)).map(|_| self.rng.int(1, wp.sides.max(1)) as f32).sum::<f32>() + self.primary_attribute(h);
                    if wp.weapon_type == "missile" || wp.weapon_type == "artillery" || wp.weapon_type.starts_with("msplash") || wp.weapon_type == "mbounce" || wp.weapon_type == "mline" {
                        if let (Some(src), Some(_)) = (self.unit(h).cloned(), self.unit(s.target)) {
                            let launch_z = self.objects.units.get(src.type_id).map_or(60.0, |o| o.real("launchZ"));
                            self.missiles.push(Missile {
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
                            });
                        }
                    } else {
                        self.deal_damage(vm, h, s.target, raw, &wp.attack_type);
                    }
                }
                let Some(u) = self.unit_mut(h) else { continue };
                if s.elapsed >= wp.damage_point + wp.backswing {
                    u.swing = None;
                    set_anim(u, "stand");
                } else {
                    u.swing = Some(s);
                }
                continue;
            }
            match action {
                Action::Cast { ability, target, x, y } => self.cast_step(vm, h, ability, target, x, y, dt),
                Action::Idle | Action::Hold => {
                    if let Some(u) = self.unit_mut(h) {
                        if !u.path.is_empty() {
                            u.path.clear();
                        }
                        if u.anim == "walk" {
                            set_anim(u, "stand");
                        }
                    }
                    if wp.is_some() && self.unit(h).is_some_and(|u| u.acquire_timer <= 0.0) {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        let range = if matches!(action, Action::Hold) { wp.as_ref().map_or(0.0, |w| w.range) } else { range };
                        if let Some(t) = self.find_target(h, range) {
                            if let Some(u) = self.unit_mut(h) {
                                u.action = if matches!(action, Action::Hold) { Action::Hold } else { Action::Attack { target: t } };
                            }
                            if matches!(action, Action::Hold) {
                                self.try_attack(h, t, dt, true);
                            }
                        }
                        if let Some(u) = self.unit_mut(h) {
                            u.acquire_timer = reaction;
                        }
                    }
                }
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
                    self.try_attack(h, target, dt, false);
                }
                Action::AttackMove { x, y } => {
                    if self.unit(h).is_some_and(|u| u.acquire_timer <= 0.0) {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        if let Some(t) = self.find_target(h, range) {
                            self.try_attack(h, t, dt, false);
                            continue;
                        }
                        self.unit_mut(h).unwrap().acquire_timer = reaction;
                    }
                    if self.unit(h).is_some_and(|u| u.path.is_empty()) {
                        self.path_to(h, (x, y));
                    }
                    if self.walk(h, dt) {
                        self.unit_mut(h).unwrap().action = Action::Idle;
                    }
                }
                Action::Harvest { target } => self.harvest(vm, h, target, dt),
                Action::Build { id, x, y } => self.build(vm, h, id, x, y, dt),
                Action::Construct { target } => {
                    let done = self.unit(target).is_none_or(|b| b.constructing.is_none() || b.dead);
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
    pub fn spend(&mut self, player: u8, id: Id, food: bool) -> Result<(), &'static str> {
        let (g, l, f) = self.cost(id);
        let p = &self.players[player as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        if food && f > 0 && p.get_state(player_state::RESOURCE_FOOD_USED) + f > p.get_state(player_state::RESOURCE_FOOD_CAP) {
            return Err("Nofood");
        }
        let p = &mut self.players[player as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        Ok(())
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

    /// Queues a unit at a structure (a train order).
    pub fn train(&mut self, h: u32, id: Id) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        if u.dead || u.constructing.is_some() || u.queue.len() >= 7 {
            return Err("");
        }
        let owner = u.owner;
        self.spend(owner, id, true)?;
        self.unit_mut(h).unwrap().queue.push(id);
        Ok(())
    }

    /// Changes a unit's type, keeping its life and mana fractions (structure upgrades,
    /// morphs).
    pub fn transform_unit(&mut self, h: u32, id: Id) {
        let Some(o) = self.objects.units.get(id) else { return };
        let (hp, mana) = (o.real("HP").max(1.0), o.real("manaN"));
        let abilities: Vec<(Id, i32)> = o.list("abilList").iter().map(|s| (crate::objects::id_from_str(s.trim()), 1)).filter(|(a, _)| a[0] != 0).collect();
        let Some(u) = self.unit_mut(h) else { return };
        let lf = if u.max_life > 0.0 { u.life / u.max_life } else { 1.0 };
        let mf = if u.max_mana > 0.0 { u.mana / u.max_mana } else { 0.0 };
        u.type_id = id;
        u.max_life = hp;
        u.life = hp * lf;
        u.max_mana = mana;
        u.mana = mana * mf;
        u.abilities = abilities;
        self.update_food();
    }

    /// Starts upgrading a structure into another type from its Upgrade list.
    pub fn upgrade_structure(&mut self, h: u32, id: Id) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        if u.dead || u.constructing.is_some() || !u.queue.is_empty() {
            return Err("");
        }
        let owner = u.owner;
        self.spend(owner, id, false)?;
        self.unit_mut(h).unwrap().queue.push(id);
        Ok(())
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
            self.unit(h).is_some_and(|u| u.owner == player && u.is_building && !u.dead && u.constructing.is_none() && u.queue.is_empty())
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
        if level + queued >= max || u.queue.len() >= 7 {
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

    /// Gold and lumber for researching level `level` + 1 of an upgrade (base plus
    /// increment per level).
    pub fn upgrade_cost(&self, id: Id, level: i32) -> (i32, i32) {
        self.objects.upgrades.get(id).map_or((0, 0), |o| (o.int("goldbase") + o.int("goldmod") * level, o.int("lumberbase") + o.int("lumbermod") * level))
    }

    /// Cancels the last queued item and refunds it (TrainRefundRate).
    pub fn cancel_last(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        let Some(id) = u.queue.pop() else { return };
        if u.queue.is_empty() {
            u.queue_progress = 0.0;
        }
        let owner = u.owner;
        let rate = self.misc_f32("TrainRefundRate", 1.0);
        self.refund(owner, id, rate);
    }

    /// A free spot near a structure for a new unit: the nearest walkable cell outside it.
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
        // Construction.
        if let Some(left) = self.unit(h).and_then(|u| u.constructing) {
            let left = left - dt;
            let u = self.unit_mut(h).unwrap();
            let total = u.build_time.max(0.01);
            // Hit points rise with construction from 10% of maximum.
            u.life = (u.life + u.max_life * 0.9 * dt / total).min(u.max_life);
            if left <= 0.0 {
                u.constructing = None;
                restart_anim(u, "stand");
                let owner = u.owner;
                let label = format!("JobDoneSound{}", self.race_label(owner));
                self.requests.push(Request::UiSound { players: 1 << owner, label });
                let ctx = EventCtx { unit: h, player: owner, has_player: true, event_id: EVENT_PLAYER_UNIT_CONSTRUCT_FINISH, ..Default::default() };
                self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_CONSTRUCT_FINISH));
            } else {
                u.constructing = Some(left);
            }
            return;
        }
        // Production.
        let Some(&id) = self.unit(h).and_then(|u| u.queue.first()) else { return };
        if let Some(up) = self.objects.upgrades.get(id) {
            let owner = self.unit(h).unwrap().owner;
            let level = self.players[owner as usize].tech_researched.get(&id).copied().unwrap_or(0);
            let t = (up.real("timebase") + up.real("timemod") * level as f32).max(0.01);
            let u = self.unit_mut(h).unwrap();
            u.queue_progress += dt;
            if u.queue_progress >= t {
                u.queue_progress = 0.0;
                u.queue.remove(0);
                *self.players[owner as usize].tech_researched.entry(id).or_insert(0) += 1;
                let label = format!("ResearchComplete{}", self.race_label(owner));
                self.requests.push(Request::UiSound { players: 1 << owner, label });
            }
            return;
        }
        let build_time = self.objects.units.get(id).map_or(1.0, |o| o.real("bldtm")).max(0.01);
        let u = self.unit_mut(h).unwrap();
        u.queue_progress += dt;
        if u.queue_progress < build_time {
            return;
        }
        // Food must be available when the unit finishes.
        let owner = u.owner;
        u.queue_progress = 0.0;
        u.queue.remove(0);
        // A structure upgrade (the building's Upgrade list) transforms it in place.
        let from = u.type_id;
        let id_s = crate::objects::id_str(id);
        if self.objects.units.get(from).is_some_and(|o| o.list("Upgrade").iter().any(|x| x.trim() == id_s)) {
            self.transform_unit(h, id);
            let label = format!("UpgradeComplete{}", self.race_label(owner));
            self.requests.push(Request::UiSound { players: 1 << owner, label });
            let ctx = EventCtx { unit: h, player: owner, has_player: true, event_id: EVENT_PLAYER_UNIT_UPGRADE_FINISH, ..Default::default() };
            self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_UPGRADE_FINISH));
            return;
        }
        let radius = self.objects.units.get(id).map_or(16.0, |o| o.real("collision"));
        let (x, y) = self.spawn_point(h, radius);
        let new = crate::natives::units::create_unit(self, owner, id, x, y, 270.0);
        self.requests.push(Request::Ack { unit: new, kind: "Ready" });
        let rally = self.unit(h).and_then(|u| u.rally);
        if let (Some((rx, ry)), true) = (rally, new != 0) {
            let smart = crate::natives::basic::order_id("smart");
            self.order_unit(new, smart, rx, ry, 0);
        }
        let ctx = EventCtx { unit: h, target_unit: new, player: owner, has_player: true, train_unit_type: crate::objects::id_to_int(id), event_id: EVENT_PLAYER_UNIT_TRAIN_FINISH, ..Default::default() };
        self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_TRAIN_FINISH));
    }

    /// Whether a structure of type `id` can be placed centered at (x, y).
    pub fn can_place(&mut self, id: Id, x: f32, y: f32) -> bool {
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.pathing_cells(&tex, x, y, 0.0);
        let Some(p) = &self.pathing else { return true };
        cells.iter().all(|&c| p.buildable(c))
    }

    /// Snaps a placement to the pathing grid the way structures align (to 64-unit marks
    /// for even-sized footprints).
    pub fn snap_placement(&self, x: f32, y: f32) -> (f32, f32) {
        let s = |v: f32| (v / 64.0).round() * 64.0;
        (s(x), s(y))
    }

    fn build(&mut self, vm: &mut Vm, h: u32, id: Id, x: f32, y: f32, dt: f32) {
        let u = self.unit(h).unwrap();
        let (d, need_path, owner) = ((u.x - x).hypot(u.y - y), u.path.is_empty() || u.repath_timer <= 0.0, u.owner);
        if d > 96.0 {
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
        if self.spend(owner, id, false).is_err() {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            set_anim(u, "stand");
            return;
        }
        let b = crate::natives::units::create_unit(self, owner, id, x, y, self.misc_f32("BuildingAngle", 270.0));
        if let Some(bu) = self.unit_mut(b) {
            let t = bu.build_time;
            bu.constructing = Some(t);
            bu.life = bu.max_life * 0.1;
            restart_anim(bu, "birth");
        }
        // Move the worker out of the footprint.
        let r = self.unit(h).map_or(16.0, |u| u.collision);
        let (sx, sy) = self.spawn_point(b, r);
        let u = self.unit_mut(h).unwrap();
        u.x = sx;
        u.y = sy;
        u.path.clear();
        u.building = b;
        u.action = Action::Construct { target: b };
        let ctx = EventCtx { unit: b, player: owner, has_player: true, event_id: EVENT_PLAYER_UNIT_CONSTRUCT_START, ..Default::default() };
        self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_CONSTRUCT_START));
    }

    /// The unit's ability with the given base code, with its level.
    pub fn ability_with_code(&self, h: u32, code: &str) -> Option<(Id, u32)> {
        let u = self.unit(h)?;
        u.abilities.iter().find_map(|&(id, lvl)| {
            let a = self.objects.abilities.get(id)?;
            (a.str("code") == code).then_some((id, lvl.max(1) as u32))
        })
    }

    fn ability_real(&self, id: Id, field: &str, level: u32) -> f32 {
        self.objects.abilities.get(id).map_or(0.0, |a| a.level_real(field, level))
    }

    fn is_tree(&self, h: u32) -> bool {
        match self.handles.get(h) {
            Some(Obj::Destructable(d)) => self.objects.destructables.get(d.type_id).is_some_and(|o| o.str("targType").to_ascii_lowercase().contains("tree")) && d.life > 0.0,
            _ => false,
        }
    }

    fn is_mine(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| !u.dead) && self.ability_with_code(h, "Agld").is_some()
    }

    pub fn can_harvest(&self, h: u32, target: u32) -> bool {
        self.ability_with_code(h, "Ahar").is_some() && (self.is_mine(target) || self.is_tree(target))
            || self.ability_with_code(h, "Ahrl").is_some() && self.is_tree(target)
    }

    fn leave_mine(&mut self, h: u32) {
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
    fn approach(&mut self, h: u32, target: u32, range: f32, dt: f32) -> bool {
        let Some(tp) = self.widget_pos(target) else { return false };
        let u = self.unit(h).unwrap();
        let Some(edge) = self.edge_distance(target, u.x, u.y) else { return false };
        let d = edge - u.collision;
        if d <= range {
            let u = self.unit_mut(h).unwrap();
            u.path.clear();
            return true;
        }
        if u.path.is_empty() || u.repath_timer <= 0.0 {
            self.path_to(h, tp);
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
        let Some((ahar, lvl)) = self.ability_with_code(h, "Ahar").or_else(|| self.ability_with_code(h, "Ahrl")) else {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        };
        let code = self.objects.abilities.get(ahar).map(|a| a.str("code")).unwrap_or_default();
        let is_mine = self.is_mine(target);
        let is_tree = self.is_tree(target);
        if !is_mine && !is_tree {
            // The source is gone: look for another tree nearby, else stop.
            let was_tree = matches!(self.handles.get(target), Some(Obj::Destructable(_)));
            let area = self.ability_real(ahar, "Area", lvl).max(0.0);
            let next = if was_tree { self.nearest_tree(h, area) } else { None };
            let u = self.unit_mut(h).unwrap();
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
        let gold_cap = self.ability_real(ahar, "DataB", lvl) as i32;
        let lumber_cap = if code == "Ahrl" { self.ability_real(ahar, "DataB", lvl) as i32 } else { self.ability_real(ahar, "DataC", lvl) as i32 };
        if (is_mine && u.carry_lumber > 0) || (is_tree && u.carry_gold > 0) || (is_tree && u.carry_lumber >= lumber_cap) {
            self.unit_mut(h).unwrap().action = Action::ReturnResources;
            return;
        }
        if is_mine {
            if u.inside == target {
                let u = self.unit_mut(h).unwrap();
                u.work_timer -= dt;
                if u.work_timer <= 0.0 {
                    self.leave_mine(h);
                    let take = gold_cap.min(self.unit(target).map_or(0, |m| m.resource));
                    if let Some(m) = self.unit_mut(target) {
                        m.resource -= take;
                    }
                    let u = self.unit_mut(h).unwrap();
                    u.carry_gold = take;
                    u.carry_lumber = 0;
                    u.action = Action::ReturnResources;
                    if self.unit(target).is_some_and(|m| m.resource <= 0) {
                        kill_unit(self, vm, target, 0);
                    }
                }
                return;
            }
            if !self.approach(h, target, 0.0 + 8.0, dt) {
                return;
            }
            // Enter if the mine has room (Agld DataC: workers inside at once).
            let (agld, mlvl) = self.ability_with_code(target, "Agld").unwrap();
            let capacity = self.ability_real(agld, "DataC", mlvl).max(1.0) as u32;
            let duration = self.ability_real(agld, "DataB", mlvl);
            if self.unit(target).is_some_and(|m| m.inside < capacity) {
                if let Some(m) = self.unit_mut(target) {
                    m.inside += 1;
                }
                let u = self.unit_mut(h).unwrap();
                u.inside = target;
                u.hidden = true;
                u.work_timer = duration;
            } else if let Some(u) = self.unit_mut(h) {
                set_anim(u, "stand");
            }
            return;
        }
        // Trees: Rng is the chopping range, Dur the time between swings, DataA lumber per
        // swing; each swing damages the tree by one.
        let range = self.ability_real(ahar, "Rng", lvl);
        if !self.approach(h, target, range, dt) {
            return;
        }
        let tp = self.widget_pos(target).unwrap();
        let u = self.unit_mut(h).unwrap();
        let left = turn(u, angle_to((u.x, u.y), tp), dt);
        if left.abs() > 30.0 {
            return;
        }
        u.work_timer -= dt;
        if u.work_timer <= 0.0 {
            let per = self.ability_real(ahar, "DataA", lvl) as i32;
            let interval = self.ability_real(ahar, "Dur", lvl).max(0.1);
            let u = self.unit_mut(h).unwrap();
            u.work_timer = interval;
            u.carry_gold = 0;
            u.carry_lumber = (u.carry_lumber + per).min(lumber_cap.max(1));
            restart_anim(u, "attack lumber");
            let mut died = false;
            if let Some(Obj::Destructable(d)) = self.handles.get_mut(target) {
                d.life -= 1.0;
                died = d.life <= 0.0;
            }
            if died {
                d_kill(self, vm, target);
            }
        }
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
    fn drop_off(&self, h: u32) -> Option<u32> {
        let u = self.unit(h)?;
        let (gold, lumber) = (u.carry_gold > 0, u.carry_lumber > 0);
        let mut best: Option<(f32, u32)> = None;
        for &o in &self.unit_order {
            let Some(b) = self.unit(o) else { continue };
            if b.dead || b.owner != u.owner {
                continue;
            }
            let Some((a, l)) = self.ability_with_code(o, "Artn") else { continue };
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
        let u = self.unit_mut(h).unwrap();
        let (gold, lumber, owner, src) = (u.carry_gold, u.carry_lumber, u.owner, u.harvest_source);
        u.carry_gold = 0;
        u.carry_lumber = 0;
        u.action = if src != 0 { Action::Harvest { target: src } } else { Action::Idle };
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += gold;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += lumber;
        *p.state.entry(player_state::GOLD_GATHERED).or_insert(0) += gold;
        *p.state.entry(player_state::LUMBER_GATHERED).or_insert(0) += lumber;
    }

    /// A hero's primary attribute, which adds to its attack damage.
    fn primary_attribute(&self, h: u32) -> f32 {
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

    /// Moves into range of `target` and starts a swing when ready.
    fn try_attack(&mut self, h: u32, target: u32, dt: f32, stationary: bool) {
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return };
        let Some(wp) = weapon(self, u.type_id) else { return };
        let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - self.reach_radius(target);
        let (tpos, upos) = ((t.x, t.y), (u.x, u.y));
        if d > wp.range {
            if stationary {
                return;
            }
            if u.path.is_empty() || u.repath_timer <= 0.0 {
                self.path_to(h, tpos);
            }
            self.walk(h, dt);
            return;
        }
        let buff_attack = self.buff_mods(h).1;
        let agi_bonus = self.misc_f32("AgiAttackSpeedBonus", 0.02);
        let u = self.unit_mut(h).unwrap();
        u.path.clear();
        let left = turn(u, angle_to(upos, tpos), dt);
        // Units attack once facing within AttackHalfAngle radians of the target.
        let half = 0.5f32.to_degrees();
        if left.abs() > half {
            return;
        }
        if u.cooldown <= 0.0 {
            // Agility shortens a hero's attack cooldown (AgiAttackSpeedBonus per point).
            let speed = if u.is_hero { 1.0 + u.agi.floor() * agi_bonus } else { 1.0 };
            let speed = speed * buff_attack;
            u.cooldown = wp.cooldown / speed.max(0.01);
            u.swing = Some(Swing { target, elapsed: 0.0, done: false });
            restart_anim(u, "attack");
        } else if u.anim == "walk" {
            set_anim(u, "stand");
        }
    }

    /// Pushes overlapping ground units apart.
    fn separate_units(&mut self, _dt: f32) {
        let hs: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.is_building && u.collision > 0.0))
            .collect();
        let pos: Vec<(f32, f32, f32)> = hs.iter().map(|&h| self.unit(h).map(|u| (u.x, u.y, u.collision)).unwrap()).collect();
        let mut push = vec![(0.0f32, 0.0f32); hs.len()];
        for i in 0..hs.len() {
            for j in i + 1..hs.len() {
                let (a, b) = (pos[i], pos[j]);
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let d = (dx * dx + dy * dy).sqrt();
                let min = (a.2 + b.2) * 0.5;
                if d < min && d > 0.001 {
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
            let fits = |w: &World, x: f32, y: f32| w.pathing.as_ref().is_none_or(|p| p.walkable(p.cell_of(x, y)));
            let (x, y) = self.unit(h).map(|u| (u.x + px, u.y + py)).unwrap();
            if fits(self, x, y) {
                let u = self.unit_mut(h).unwrap();
                u.x = x;
                u.y = y;
            }
        }
    }

    fn update_missiles(&mut self, vm: &mut Vm, dt: f32) {
        let mut hits = Vec::new();
        let mut i = 0;
        while i < self.missiles.len() {
            let m = &mut self.missiles[i];
            let target = match self.handles.get(m.target) {
                Some(Obj::Unit(t)) => Some((t.x, t.y, t.dead)),
                _ => None,
            };
            let Some((tx, ty, _)) = target else {
                self.missiles.swap_remove(i);
                continue;
            };
            let (dx, dy) = (tx - m.x, ty - m.y);
            let d = (dx * dx + dy * dy).sqrt();
            let step = m.speed * dt;
            if d <= step {
                let m = self.missiles.swap_remove(i);
                hits.push(m);
                continue;
            }
            m.x += dx / d * step;
            m.y += dy / d * step;
            i += 1;
        }
        for m in hits {
            match m.spell {
                Some((a, lvl, stun, buff)) => {
                    self.spell_damage(vm, m.source, m.target, m.damage);
                    if stun > 0.0 {
                        self.apply_buff(m.target, Buff { id: buff, ability: a, level: lvl, caster: m.source, remaining: stun, stun: true, ..Default::default() });
                    }
                }
                None => self.deal_damage(vm, m.source, m.target, m.damage, &m.attack_type),
            }
        }
    }
}
