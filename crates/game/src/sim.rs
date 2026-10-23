//! Unit behavior: orders, movement, target acquisition, attacks, missiles, and death.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::natives::players::is_ally;
use crate::natives::units::kill_unit;
use crate::objects::Id;
use crate::types::*;
use crate::world::World;

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
    let armor = o.map_or(0.0, |o| o.real("def"));
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

fn angle_to(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.1 - from.1).atan2(to.0 - from.0).to_degrees()
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (b - a + 540.0).rem_euclid(360.0) - 180.0
}

/// Turns toward `target_deg`; returns the remaining difference.
fn turn(u: &mut Unit, target_deg: f32, dt: f32) -> f32 {
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
        } else {
            return;
        };
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

    fn path_to(&mut self, h: u32, goal: (f32, f32)) {
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
    fn walk(&mut self, h: u32, dt: f32) -> bool {
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
        if t.dead || t.invulnerable {
            return;
        }
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
            if u.paused || u.is_building {
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
                    let raw = wp.base + (0..wp.dice.max(0)).map(|_| self.rng.int(1, wp.sides.max(1)) as f32).sum::<f32>();
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

    /// Moves into range of `target` and starts a swing when ready.
    fn try_attack(&mut self, h: u32, target: u32, dt: f32, stationary: bool) {
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return };
        let Some(wp) = weapon(self, u.type_id) else { return };
        let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - t.collision;
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
        let u = self.unit_mut(h).unwrap();
        u.path.clear();
        let left = turn(u, angle_to(upos, tpos), dt);
        // Units attack once facing within AttackHalfAngle radians of the target.
        let half = 0.5f32.to_degrees();
        if left.abs() > half {
            return;
        }
        if u.cooldown <= 0.0 {
            u.cooldown = wp.cooldown;
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
            self.deal_damage(vm, m.source, m.target, m.damage, &m.attack_type);
        }
    }
}
