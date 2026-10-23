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
}

/// Weapon 1 of a unit type, from UnitWeapons.slk and the unit's profile.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
    let o = w.objects.units.get(type_id)?;
    let f = |k: &str| format!("{k}{n}");
    let pick = |k: &str| -> String {
        let l = o.list(k);
        l.get(n - 1).or_else(|| l.first()).cloned().unwrap_or_default()
    };
    Some(Weapon {
        range: o.real(&f("rangeN")),
        cooldown: o.real(&f("cool")),
        damage_point: o.real(&f("dmgpt")),
        backswing: o.real(&f("backSw")),
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
pub fn unit_weapons(w: &World, u: &Unit) -> Vec<(u8, Weapon)> {
    // A burrow shoots only with workers inside.
    if u.extra.cargo.is_empty() && w.ability_with_code(u.handle, "Abun").is_some() {
        return Vec::new();
    }
    let mut mask = u.extra.weapons.unwrap_or_else(|| w.objects.units.get(u.type_id).map_or(0, |o| o.int("weapsOn")));
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

/// The first of a unit's weapons that can hit a target.
pub fn weapon_for(w: &World, u: &Unit, t: &Unit) -> Option<(u8, Weapon)> {
    unit_weapons(w, u).into_iter().find(|(_, wp)| can_target(w, wp, u, t))
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
    let def_type = target.extra.defense.clone().unwrap_or_else(|| o.map(|o| o.str("defType").to_ascii_lowercase()).unwrap_or_default());
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
    let armor_factor = if armor >= 0.0 { 1.0 - (armor * k) / (1.0 + armor * k) } else { 2.0 - (1.0 - k).powf(-armor) };
    raw * mult * armor_factor
}

fn can_target(w: &World, wp: &Weapon, attacker: &Unit, t: &Unit) -> bool {
    if t.dead || t.hidden || w.unit_invulnerable(t) || w.is_locust(t) || t.handle == attacker.handle {
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
        self.order_unit(h, order, x, y, target);
    }

    pub fn order_unit(&mut self, h: u32, order: i32, x: f32, y: f32, target: u32) {
        let smart = crate::natives::basic::order_id("smart");
        let mv = crate::natives::basic::order_id("move");
        let attack = crate::natives::basic::order_id("attack");
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
        if order == crate::natives::basic::order_id("unloadall") || order == crate::natives::basic::order_id("unloadallinstant") || (order == crate::natives::basic::order_id("standdown") && self.ability_with_code(h, "Abun").is_some()) {
            self.unload_all(h);
            return;
        }
        if order == unload && self.cargo_hold(h).is_some() {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Unload { x, y };
                u.path.clear();
                u.work_timer = 0.0;
            }
            return;
        }
        if (order == crate::natives::basic::order_id("load") || order == smart) && target != 0 && self.ability_with_code(h, "Aloa").is_some() && self.can_board(h, target) {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Load { target };
                u.path.clear();
            }
            return;
        }
        // Battle Stations (Abtl): the owner's UnitID workers within Area head inside.
        if order == crate::natives::basic::order_id("battlestations") {
            if let Some((a, l)) = self.ability_with_code(h, "Abtl") {
                let (area, id) = self.objects.abilities.get(a).map_or((0.0, [0; 4]), |o| (o.level_real("Area", l), crate::objects::id_from_str(o.level_str("UnitID", l).trim())));
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
        // entangleinstant: Entangle Gold Mine at once, without its casting time.
        if order == crate::natives::basic::order_id("entangleinstant") && self.ability_with_code(h, "Aent").is_some() {
            self.entangle(h, target);
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
                self.unit_mut(h).unwrap().rally = Some(p);
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
            if self.ability_with_code(h, "Alam").is_some() && self.sacrifice(h, target) {
                return;
            }
            if self.ability_with_code(h, "Asac").is_some() && self.sacrifice(target, h) {
                return;
            }
        }
        // Autocast toggles, and morphs (Bear Form and the like) by their order or unorder.
        if let Some(name) = crate::natives::basic::order_name(order) {
            if self.autocast_order(h, name) || self.morph_order(h, name) {
                return;
            }
        }
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
        } else if (order == crate::natives::basic::order_id("repair") || order == smart) && target != 0 && self.unit(target).is_some_and(|t| t.extra.needs_builder && t.constructing.is_some()) && self.unit(h).is_some_and(|u| self.unit(target).is_some_and(|t| t.owner == u.owner)) && self.ability_with_code(h, "Arep").is_some() {
            // A peasant helps build a peasant's structure.
            Action::Construct { target }
        } else if (order == crate::natives::basic::order_id("repair") || order == smart) && target != 0 && self.can_repair(h, target) {
            Action::Repair { target }
        } else if order == smart && target != 0 && self.unit(target).is_some_and(|t| t.extra.waygate.is_some_and(|g| g.2)) {
            // Into an active waygate.
            let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
            Action::Move { x: tx, y: ty }
        } else if order == smart && target != 0 && self.cargo_hold(target).is_some() && self.can_board(target, h) {
            Action::Board { transport: target }
        } else if (order == smart || order == crate::natives::basic::order_id("harvest")) && target != 0 && self.can_harvest(h, target) {
            Action::Harvest { target }
        } else if order == crate::natives::basic::order_id("returnresources") {
            Action::ReturnResources
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
            Action::Cast { ability: a, target, x, y }
        } else {
            return;
        };
        self.leave_mine(h);
        if let Some(u) = self.unit_mut(h) {
            u.action = action;
            // A new order cuts off an attack or a cast's swing (and a channel, below),
            // unless it is the same attack.
            u.casting = None;
            if !matches!((&u.action, &u.swing), (Action::Attack { target: t }, Some(s)) if *t == s.target) {
                u.swing = None;
            }
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

    /// How a unit moves, from its type's movetp (an uprooted ancient walks).
    pub fn move_kind(&self, h: u32) -> crate::pathing::Move {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(crate::pathing::Move::Foot, |o| crate::pathing::Move::from_movetp(&o.str("movetp")))
    }

    pub(crate) fn path_to(&mut self, h: u32, goal: (f32, f32)) {
        let Some(u) = self.unit(h) else { return };
        let (from, radius) = ((u.x, u.y), u.collision);
        let kind = self.move_kind(h);
        let free = u.extra.no_pathing;
        let path = match &self.pathing {
            // Flyers go straight over everything but the map's no-fly cells, as does a
            // unit whose pathing is off (SetUnitPathing).
            Some(_) if kind == crate::pathing::Move::Fly || free => vec![goal],
            Some(p) => {
                let direct = p.find_path_kind(from, goal, radius * 0.5, 20000, kind);
                let reaches = |path: &Vec<(f32, f32)>, to: (f32, f32)| path.last().is_some_and(|e| (e.0 - to.0).hypot(e.1 - to.1) < 64.0);
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
        let (speed, owner, tid) = (u.move_speed + u.item_bonus.move_speed, u.owner, u.type_id);
        let ub = self.upgrade_bonus(owner, tid);
        let step = (speed + ub.move_speed) * (1.0 + ub.move_pct) * dt;
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

    fn find_target(&self, h: u32, range: f32) -> Option<u32> {
        let u = self.unit(h)?;
        let weapons = unit_weapons(self, u);
        if weapons.is_empty() {
            return None;
        }
        let mut best: Option<(f32, u32)> = None;
        for &t in &self.unit_order {
            let Some(tu) = self.unit(t) else { continue };
            if is_ally(self, u.owner, tu.owner) || !weapons.iter().any(|(_, wp)| can_target(self, wp, u, tu)) {
                continue;
            }
            // Neutral passive units are never acquired automatically, nor are
            // invisible or ethereal ones.
            if tu.owner == PLAYER_NEUTRAL_PASSIVE || (self.is_invisible(t) && !self.detected_by(t, u.owner)) || self.is_ethereal(t) {
                continue;
            }
            let d = (tu.x - u.x).hypot(tu.y - u.y);
            // Only what the owner can see (a cliff above, the fog) is acquired.
            if d <= range && self.fog_state(tu.x, tu.y, u.owner) != crate::fog::FOG_VISIBLE {
                continue;
            }
            if d <= range && best.is_none_or(|b| d < b.0) {
                best = Some((d, t));
            }
        }
        best.map(|b| b.1)
    }

    pub fn deal_damage_pub(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, attack_type: &str) {
        self.deal_damage(vm, source, target, raw, attack_type);
    }

    fn deal_damage(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, attack_type: &str) {
        let Some(t) = self.unit(target) else { return };
        if t.dead || self.unit_invulnerable(t) || self.buff_mods(target).5 {
            return;
        }
        // Ethereal units take no physical damage (EtherealDamageBonus: magic 1.66).
        if self.is_ethereal(target) && !attack_type.eq_ignore_ascii_case("magic") {
            return;
        }
        if self.evades(target) {
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
        // Berserk: the target takes more damage; Defend: less from piercing attacks.
        let mut taken = 1.0 + self.unit(target).map_or(0.0, |u| u.buffs.iter().map(|b| b.damage_taken).sum());
        if attack_type == "pierce" {
            if let Some(p) = self.unit(target).and_then(|u| u.buffs.iter().find(|b| b.pierce_taken > 0.0)).map(|b| b.pierce_taken) {
                taken *= p;
            }
        }
        let raw = raw * taken;
        let mut raw = self.on_attack_passives(source, target, raw * self.buff_mods(source).3) + self.buff_damage_bonus(source);
        // Wind Walk: the first strike adds its backstab damage and ends the invisibility.
        if let Some(u) = self.unit_mut(source) {
            if let Some(b) = u.buffs.iter().find(|b| b.backstab > 0.0) {
                raw += b.backstab;
            }
            u.buffs.retain(|b| !b.invisible);
        }
        let Some(t) = self.unit(target) else { return };
        let dmg = final_damage(self, raw, attack_type, t);
        let melee_hit = self.unit(source).and_then(|u| u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| weapon(self, u.type_id))).is_some_and(|w| w.range <= self.misc.get_str("InfoPanel", "MeleeRangeMax").and_then(|v| v.trim().parse().ok()).unwrap_or(128.0));
        let dmg = if attack_type == "spells" { dmg } else { self.hardened_skin(target, melee_hit, dmg) };
        let dmg = self.elunes_grace(target, &attack_type.to_ascii_lowercase(), dmg);
        let dmg = self.illusion_damage(source, target, dmg);
        let Some(t) = self.unit(target) else { return };
        let owner = t.owner;
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
            let Some(t) = self.unit(target) else { return };
            // Retaliate when idle.
            if matches!(t.action, Action::Idle) && !unit_weapons(self, t).is_empty() && !t.is_building {
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
                // UnitSuspendDecay holds the corpse's clock.
                if u.extra.suspend_decay {
                    self.unit_mut(h).unwrap().death_time += dt;
                    continue;
                }
                let decay = if u.extra.exploded { 0.0 } else if u.is_building { self.misc_f32("StructureDecayTime", 30.0) } else { self.misc_f32("BoneDecayTime", 88.0) };
                let t = self.time - u.death_time;
                // Dead heroes stay, out of sight, to be revived.
                if u.is_hero {
                    if t > decay && !u.hidden {
                        self.unit_mut(h).unwrap().hidden = true;
                    }
                } else if t > decay {
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
            // An idle unit takes its next queued order.
            if matches!(u.action, Action::Idle) && !u.extra.order_queue.is_empty() {
                let next = self.unit_mut(h).unwrap().extra.order_queue.pop_front();
                if let Some((order, x, y, target)) = next {
                    self.order_unit(h, order, x, y, target);
                }
            }
            let Some(u) = self.unit(h) else { continue };
            let action = u.action.clone();
            let wp = u.swing.as_ref().and_then(|s| weapon_numbered(self, u, s.weapon)).or_else(|| unit_weapons(self, u).into_iter().next().map(|x| x.1));
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
                    let (owner, tid) = self.unit(h).map_or((0, [0; 4]), |u| (u.owner, u.type_id));
                    let ub = self.upgrade_bonus(owner, tid);
                    let bonus = self.unit(h).map_or(0.0, |u| u.item_bonus.damage) + ub.damage;
                    let dice = wp.dice + ub.dice;
                    let raw = wp.base + (0..dice.max(0)).map(|_| self.rng.int(1, wp.sides.max(1)) as f32).sum::<f32>() + self.primary_attribute(h) + bonus;
                    let raw = self.attack_modifiers(vm, h, s.target, raw);
                    self.attack_passives(vm, h, s.target);
                    self.club_swing(h);
                    if self.unit(h).is_none_or(|u| u.dead) {
                        continue;
                    }
                    if wp.weapon_type == "missile" || wp.weapon_type == "artillery" || wp.weapon_type.starts_with("msplash") || wp.weapon_type == "mbounce" || wp.weapon_type == "mline" {
                        if let (Some(src), Some(_)) = (self.unit(h).cloned(), self.unit(s.target)) {
                            // Lightning Attack: a weapon firing the ability's missile also
                            // strikes with its lightning, DataA after for DataB.
                            if let Some((a, lvl)) = self.ability_with_code(h, "Alit") {
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
                            });
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
                        self.attack_impact(vm, h, s.target, raw, &wp, at);
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
                // Handing an item over: within DropItemRange of the target, into its
                // inventory (or onto the ground beside it when it has no room).
                Action::GiveItem { item, target } => {
                    let range = self.misc_f32("DropItemRange", 100.0);
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
                    if self.unit(h).is_some_and(|u| u.inventory.contains(&item)) {
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
                    if let Some(u) = self.unit_mut(h) {
                        if !u.path.is_empty() {
                            u.path.clear();
                        }
                        if u.anim == "walk" {
                            set_anim(u, "stand");
                        }
                    }
                    if wp.is_some() && self.unit(h).is_some_and(|u| u.acquire_timer <= 0.0 && !u.extra.sleeping) {
                        let range = self.unit(h).map_or(0.0, |u| u.acquire_range);
                        let range = if matches!(action, Action::Hold) { wp.as_ref().map_or(0.0, |w| w.range) } else { range };
                        if let Some(t) = self.find_target(h, range) {
                            if let Some(u) = self.unit_mut(h) {
                                u.action = if matches!(action, Action::Hold) { Action::Hold } else { Action::Attack { target: t } };
                            }
                            self.spell_ai(h, t, h);
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
                            self.spell_ai(h, t, h);
                            if self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                                continue;
                            }
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
        if !self.missing_requirements(owner, id, 1).is_empty() || !self.tech_allowed(owner, id, 1) {
            return Err("");
        }
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
        if !self.missing_requirements(owner, id, 1).is_empty() || !self.tech_allowed(owner, id, 1) {
            return Err("");
        }
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
        if level + queued >= max || u.queue.len() >= 7 || !self.missing_requirements(player, id, (level + queued + 1) as u32).is_empty() || !self.tech_allowed(player, id, (level + queued + 1) as u32) {
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
        if u.constructing.is_some() {
            let id = u.type_id;
            let rate = self.misc_f32("ConstructionRefundRate", 0.75);
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
        u.queue.pop();
        if u.queue.is_empty() {
            u.queue_progress = 0.0;
            u.queue_started = false;
        }
        self.refund(owner, id, rate);
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
        self.update_revive(vm, h, dt);
        // Construction.
        if let Some(left) = self.unit(h).and_then(|u| u.constructing) {
            // A peasant's structure stands still without a builder at it, and goes up
            // faster with more (powerbuild), each extra builder costing its share.
            let needs = self.unit(h).is_some_and(|u| u.extra.needs_builder);
            let speed = if needs { self.build_speed(h) } else { 1.0 };
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
            if left <= 0.0 {
                u.constructing = None;
                restart_anim(u, "stand");
                let owner = u.owner;
                let label = format!("JobDoneSound{}", self.race_label(owner));
                self.requests.push(Request::UiSound { players: 1 << owner, label });
                self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_CONSTRUCT_FINISH, EVENT_UNIT_CONSTRUCT_FINISH, EventCtx::default());
            } else {
                u.constructing = Some(left);
            }
            return;
        }
        // Production.
        let Some(&id) = self.unit(h).and_then(|u| u.queue.first()) else { return };
        // The item at the front starts: research, a structure upgrade, or training.
        if !self.unit(h).unwrap().queue_started {
            self.unit_mut(h).unwrap().queue_started = true;
            let (pe, ue, ctx) = self.production_event(h, id, 0);
            self.fire_unit_pair(vm, h, pe, ue, ctx);
        }
        if let Some(up) = self.objects.upgrades.get(id) {
            let owner = self.unit(h).unwrap().owner;
            let level = self.players[owner as usize].tech_researched.get(&id).copied().unwrap_or(0);
            let t = (up.real("timebase") + up.real("timemod") * level as f32).max(0.01);
            let u = self.unit_mut(h).unwrap();
            u.queue_progress += dt;
            if u.queue_progress >= t {
                u.queue_progress = 0.0;
                u.queue.remove(0);
                u.queue_started = false;
                *self.players[owner as usize].tech_researched.entry(id).or_insert(0) += 1;
                self.research_changed(owner);
                let label = format!("ResearchComplete{}", self.race_label(owner));
                self.requests.push(Request::UiSound { players: 1 << owner, label });
                let ctx = EventCtx { research: crate::objects::id_to_int(id), ..Default::default() };
                self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_RESEARCH_FINISH, EVENT_UNIT_RESEARCH_FINISH, ctx);
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
        u.queue_started = false;
        // A structure upgrade (the building's Upgrade list) transforms it in place.
        let from = u.type_id;
        let id_s = crate::objects::id_str(id);
        if self.objects.units.get(from).is_some_and(|o| o.list("Upgrade").iter().any(|x| x.trim() == id_s)) {
            self.transform_unit(h, id);
            let label = format!("UpgradeComplete{}", self.race_label(owner));
            self.requests.push(Request::UiSound { players: 1 << owner, label });
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_UPGRADE_FINISH, EVENT_UNIT_UPGRADE_FINISH, EventCtx::default());
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
        let ctx = EventCtx { target_unit: new, train_unit_type: crate::objects::id_to_int(id), ..Default::default() };
        self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_TRAIN_FINISH, EVENT_UNIT_TRAIN_FINISH, ctx);
    }

    /// Whether a structure of type `id` can be placed centered at (x, y).
    pub fn can_place(&mut self, id: Id, x: f32, y: f32) -> bool {
        if self.builds_on(id) {
            return self.build_on_site(x, y).is_some();
        }
        if self.objects.units.get(id).is_some_and(|o| o.str("requirePlace").to_ascii_lowercase().contains("blighted")) && !self.footprint_blighted(id, x, y) {
            return false;
        }
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.pathing_cells(&tex, x, y, 0.0);
        let Some(p) = &self.pathing else { return true };
        cells.iter().all(|&c| p.buildable(c))
    }

    /// The footprint's cells at a placement, each with whether it can be built on
    /// (CPlacementBox's per-cell colors), and their size.
    pub fn placement_cells(&mut self, id: Id, x: f32, y: f32) -> Vec<((f32, f32), bool)> {
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.pathing_cells(&tex, x, y, 0.0);
        let whole = self.builds_on(id) && self.build_on_site(x, y).is_some();
        let Some(p) = &self.pathing else { return Vec::new() };
        cells.iter().map(|&c| (p.center(c), whole || p.buildable(c))).collect()
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
        // Structures whose requirements are not met, or over their limit, are not
        // started.
        if !self.missing_requirements(owner, id, 1).is_empty() || !self.tech_allowed(owner, id, 1) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
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
        // A structure built on another unit stands on it and takes it over.
        let site = if self.builds_on(id) { self.build_on_site(x, y) } else { None };
        let (x, y) = site.and_then(|s| self.unit(s).map(|m| (m.x, m.y))).unwrap_or((x, y));
        let b = crate::natives::units::create_unit(self, owner, id, x, y, self.misc_f32("BuildingAngle", 270.0));
        if let Some(s) = site {
            self.cover_mine(b, s);
        }
        if let Some(bu) = self.unit_mut(b) {
            let t = bu.build_time;
            bu.constructing = Some(t);
            bu.life = bu.max_life * 0.1;
            restart_anim(bu, "birth");
        }
        // Move the worker out of the footprint.
        let r = self.unit(h).map_or(16.0, |u| u.collision);
        let (sx, sy) = self.spawn_point(b, r);
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
        let u = self.unit_mut(h).unwrap();
        u.x = sx;
        u.y = sy;
        u.path.clear();
        if summons {
            u.action = Action::Idle;
            set_anim(u, "stand");
        } else {
            u.building = b;
            u.action = Action::Construct { target: b };
            u.hidden = enters;
        }
        let ctx = EventCtx { unit: b, player: owner, has_player: true, event_id: EVENT_PLAYER_UNIT_CONSTRUCT_START, ..Default::default() };
        self.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == EVENT_PLAYER_UNIT_CONSTRUCT_START));
    }

    /// The unit's ability with the given base code, with its level.
    pub fn ability_with_code(&self, h: u32, code: &str) -> Option<(Id, u32)> {
        let u = self.unit(h)?;
        u.abilities.iter().find_map(|&(id, lvl)| {
            let a = self.objects.abilities.get(id)?;
            (a.str("code") == code && self.ability_enabled(u.owner, id)).then_some((id, lvl.max(1) as u32))
        })
    }

    fn ability_real(&self, id: Id, field: &str, level: u32) -> f32 {
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
        self.unit(h).is_some_and(|u| !u.dead && !u.hidden) && self.ability_with_code(h, "Agld").is_some()
    }

    pub fn can_harvest(&self, h: u32, target: u32) -> bool {
        self.can_mine_covered(h, target)
            || self.ability_with_code(h, "Awha").is_some() && self.is_tree(target)
            || self.ability_with_code(h, "Ahar").is_some() && (self.is_mine(target) || self.is_tree(target))
            || self.ability_with_code(h, "Ahrl").is_some() && self.is_tree(target)
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
        if self.ability_with_code(h, "Aaha").is_some() {
            return self.acolyte_harvest(vm, h, target, dt);
        }
        if self.ability_with_code(h, "Awha").is_some() {
            return self.wisp_harvest(vm, h, target, dt);
        }
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
        // Improved Lumber Harvesting (rlum) adds to a trip.
        let lumber_cap = lumber_cap + self.unit(h).map_or(0, |u| self.upgrade_bonus(u.owner, u.type_id).lumber);
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
    fn drop_off(&self, h: u32) -> Option<u32> {
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
        // Upkeep takes its share of what is returned; the amount shows over the worker
        // (0x6f40b150).
        self.gather(owner, h, gold, lumber, 0.0);
    }

    /// The upkeep level from food used: how many of [Misc] UpkeepUsage's thresholds it is
    /// above (game.dll FUN_6f047f90; 40 and 70 for Reign of Chaos, 50 and 80 for The Frozen
    /// Throne when the data has none).
    pub fn upkeep_level(&self, player: u8) -> usize {
        let used = self.players.get(player as usize).map_or(0, |p| p.get_state(player_state::RESOURCE_FOOD_USED));
        let data: Vec<i32> = self.misc.get_list("Misc", "UpkeepUsage").iter().filter_map(|v| v.trim().parse().ok()).collect();
        let thresholds = if data.is_empty() { if self.version >= 1 { vec![50, 80] } else { vec![40, 70] } } else { data };
        thresholds.iter().take_while(|&&t| t < used).count()
    }

    /// Percent of gold and lumber income upkeep takes ([Misc] UpkeepGoldTax and
    /// UpkeepLumberTax by level; 0, 30, 60 and none without data: FUN_6f0016f0,
    /// FUN_6f001880).
    pub fn upkeep_tax(&self, player: u8) -> (i32, i32) {
        let level = self.upkeep_level(player).min(9);
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

    /// Moves into range of `target` and starts a swing when ready.
    fn try_attack(&mut self, h: u32, target: u32, dt: f32, stationary: bool) {
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return };
        let Some((wn, wp)) = weapon_for(self, u, t) else { return };
        let d = (t.x - u.x).hypot(t.y - u.y) - u.collision - self.reach_radius(target);
        let (tpos, upos) = ((t.x, t.y), (u.x, u.y));
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        if d > wp.range + ub.range {
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
            let speed = if u.is_hero { 1.0 + (u.agi + u.item_bonus.agi).floor() * agi_bonus } else { 1.0 };
            let speed = (speed + u.item_bonus.attack_speed + ub.attack_speed) * buff_attack;
            u.cooldown = wp.cooldown / speed.max(0.01);
            u.swing = Some(Swing { target, elapsed: 0.0, done: false, weapon: wn });
            restart_anim(u, "attack");
            // The target is attacked (GetAttacker is the attacker).
            self.queue_unit_event(target, EVENT_PLAYER_UNIT_ATTACKED, EVENT_UNIT_ATTACKED, EventCtx { other_unit: h, ..Default::default() });
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
        for m in self.missiles.iter_mut().filter(|m| m.id == 0) {
            self.missile_serial += 1;
            m.id = self.missile_serial;
        }
        self.update_stampede(vm, dt);
        let mut hits = Vec::new();
        let mut i = 0;
        while i < self.missiles.len() {
            let m = &mut self.missiles[i];
            if m.stampede.is_some() || m.rock.is_some() {
                i += 1;
                continue;
            }
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
                None => match &m.weapon {
                    Some(wp) => {
                        let at = m.impact.or_else(|| self.widget_pos(m.target)).unwrap_or((m.x, m.y));
                        self.attack_impact(vm, m.source, m.target, m.damage, wp, at);
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
                            if let Some(n) = next {
                                let z = self.ground_z(at.0, at.1) + 60.0;
                                self.missiles.push(Missile { id: 0, target: n, x: at.0, y: at.1, z, start: at, damage: m.damage * (1.0 - wp.damage_loss), bounces: m.bounces - 1, hit, impact: None, ..m.clone() });
                            }
                        }
                    }
                    None => self.deal_damage(vm, m.source, m.target, m.damage, &m.attack_type),
                },
            }
        }
    }

    /// An attack lands: the target takes the damage (artillery hits only what is
    /// where it lands), then splash and line spill spread it.
    fn attack_impact(&mut self, vm: &mut Vm, source: u32, target: u32, raw: f32, wp: &Weapon, at: (f32, f32)) {
        let artillery = wp.weapon_type == "artillery" || wp.weapon_type == "aline";
        if !artillery {
            self.deal_damage(vm, source, target, raw, &wp.attack_type);
        }
        // A bouncing weapon's Farea is how far it looks for its next target, not splash.
        let area = if wp.weapon_type == "mbounce" { 0.0 } else { wp.full_area.max(wp.half_area).max(wp.quarter_area) };
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
            if let Some((a, lvl)) = self.ability_with_code(source, "Apts") {
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
            if let Some((a, lvl)) = self.ability_with_code(source, "Abof") {
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
        for code in ["Aflk", "Afsh"] {
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
        let flags: Vec<&str> = targets.split(',').map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "_").collect();
        if flags.is_empty() || t.dead || t.hidden {
            return false;
        }
        let has = |f: &str| flags.contains(&f);
        let air = self.objects.units.get(t.type_id).is_some_and(|o| o.str("movetp").eq_ignore_ascii_case("fly"));
        let class_ok = if t.is_building { has("structure") } else if air { has("air") } else { has("ground") };
        if !class_ok {
            return false;
        }
        let Some(owner) = self.unit(source).map(|s| s.owner) else { return true };
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
