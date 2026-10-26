//! Transports (CAbilityCargoHold, Acar and Abun): a transport carries units up to its
//! DataA in their UnitData cargoSize; Load (Aloa) takes a unit aboard within its Rng,
//! a unit told onto a friendly transport walks to it and boards, and Drop (Adro)
//! unloads the cargo at a point one unit every Dur of the cargo hold.  Units aboard are
//! hidden and keep the place they boarded at (measured: GetUnitX of a footman aboard a
//! flying zeppelin reads where it boarded); they are lost when it dies.

use w3_jass::Vm;

use crate::types::*;
use crate::world::World;

impl World {
    /// The cargo hold of a unit: (capacity, load range, unload interval).
    pub fn cargo_hold(&self, h: u32) -> Option<(i32, f32, f32)> {
        let (a, l) = self.ability_with_code(h, "Acar").or_else(|| self.ability_with_code(h, "Abun"))?;
        let o = self.objects.abilities.get(a)?;
        Some((o.level_real("DataA", l) as i32, o.level_real("Rng", l), o.level_real("Dur", l)))
    }

    pub fn cargo_size(&self, h: u32) -> i32 {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(1, |o| o.int("cargoSize").max(1))
    }

    fn cargo_used(&self, h: u32) -> i32 {
        self.unit(h).map_or(0, |u| u.extra.cargo.iter().map(|&c| self.cargo_size(c)).sum())
    }

    /// Whether a transport can take a unit aboard: its friend, on the ground, not a
    /// structure, and with room.
    pub fn can_board(&self, transport: u32, h: u32) -> bool {
        let (Some(t), Some(u)) = (self.unit(transport), self.unit(h)) else { return false };
        let Some((cap, _, _)) = self.cargo_hold(transport) else { return false };
        if t.dead || u.dead || u.is_building || u.hidden || transport == h || !crate::natives::players::is_ally(self, t.owner, u.owner) {
            return false;
        }
        if self.move_kind(h) == crate::pathing::Move::Fly {
            return false;
        }
        // A burrow takes only its owner's workers that can enter it.
        if self.ability_with_code(transport, "Abun").is_some() && (t.owner != u.owner || self.ability_with_code(h, "Ahar").is_none()) {
            return false;
        }
        self.cargo_used(transport) + self.cargo_size(h) <= cap
    }

    /// Puts a unit aboard a transport.
    pub fn board(&mut self, transport: u32, h: u32) -> bool {
        if !self.can_board(transport, h) {
            return false;
        }
        self.leave_mine(h);
        if let Some(u) = self.unit_mut(h) {
            u.hidden = true;
            u.action = Action::Idle;
            u.path.clear();
            u.swing = None;
            u.extra.transport = transport;
        }
        if let Some(t) = self.unit_mut(transport) {
            t.extra.cargo.push(h);
        }
        // EVENT_UNIT_LOADED, on the transport: the loaded unit and its transport.
        let ctx = EventCtx { other_unit: transport, ..Default::default() };
        self.queue_unit_event(h, crate::event_ids::EVENT_PLAYER_UNIT_LOADED, crate::event_ids::EVENT_UNIT_LOADED, ctx);
        true
    }

    /// Takes a unit off its transport beside it.
    pub fn disembark(&mut self, h: u32) {
        self.disembark_at(h, None);
    }

    /// Takes a unit off its transport, at a point if given (the nearest clear spot to
    /// it: measured, a zeppelin unloading at a point sets its first footman there and
    /// the others just beside), else beside the transport.
    pub fn disembark_at(&mut self, h: u32, at: Option<(f32, f32)>) {
        let Some(transport) = self.unit(h).map(|u| u.extra.transport).filter(|&t| t != 0) else { return };
        if let Some(t) = self.unit_mut(transport) {
            t.extra.cargo.retain(|&c| c != h);
        }
        let r = self.unit(h).map_or(16.0, |u| u.collision);
        let (x, y) = match at {
            Some((px, py)) => {
                let type_id = self.unit(h).map(|u| u.type_id).unwrap_or_default();
                crate::natives::units::placed_at(self, type_id, px, py, 0.0)
            }
            None if self.unit(transport).is_some() => self.spawn_point_pub(transport, r),
            None => self.unit(h).map(|u| (u.x, u.y)).unwrap_or_default(),
        };
        if let Some(u) = self.unit_mut(h) {
            u.hidden = false;
            u.x = x;
            u.y = y;
            u.extra.transport = 0;
            u.action = Action::Idle;
        }
    }

    /// A transport takes a unit aboard (order "load"): it goes to within its Load range.
    pub(crate) fn load_step(&mut self, h: u32, target: u32, dt: f32) {
        let range = self.ability_with_code(h, "Aloa").and_then(|(a, l)| self.objects.abilities.get(a).map(|o| o.level_real("Rng", l))).unwrap_or(80.0);
        if !self.can_board(h, target) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        if !self.approach(h, target, range, dt) {
            return;
        }
        self.board(h, target);
        self.unit_mut(h).unwrap().action = Action::Idle;
    }

    /// A unit walks to a friendly transport and boards it; a flying transport comes for
    /// it instead, once free, while it waits (measured: footmen told to board a
    /// zeppelin stay put, and the zeppelin flies to each).
    pub(crate) fn board_step(&mut self, h: u32, transport: u32, dt: f32) {
        let Some((_, range, _)) = self.cargo_hold(transport) else {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        };
        if !self.can_board(transport, h) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        if self.move_kind(transport) == crate::pathing::Move::Fly && self.ability_with_code(transport, "Aloa").is_some() {
            if let Some(t) = self.unit_mut(transport).filter(|t| matches!(t.action, Action::Idle)) {
                t.action = Action::Load { target: h };
                t.path.clear();
            }
            return;
        }
        if !self.approach(h, transport, range, dt) {
            return;
        }
        self.board(transport, h);
    }

    /// A transport unloads (order "unload" at a point): it goes there, then drops its
    /// cargo one unit every unload interval.
    pub(crate) fn unload_step(&mut self, h: u32, x: f32, y: f32, dt: f32) {
        let range = self.ability_with_code(h, "Adro").and_then(|(a, l)| self.objects.abilities.get(a).map(|o| o.level_real("Rng", l))).unwrap_or(80.0);
        let Some(u) = self.unit(h) else { return };
        if u.extra.cargo.is_empty() {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        if (u.x - x).hypot(u.y - y) > range && !u.is_building {
            if u.path.is_empty() || u.repath_timer <= 0.0 {
                self.path_to(h, (x, y));
            }
            if !self.walk(h, dt) {
                return;
            }
        }
        let interval = self.cargo_hold(h).map_or(0.5, |c| c.2);
        let u = self.unit_mut(h).unwrap();
        u.path.clear();
        u.work_timer -= dt;
        if u.work_timer > 0.0 {
            return;
        }
        u.work_timer = interval;
        if let Some(&c) = u.extra.cargo.first() {
            self.disembark_at(c, Some((x, y)));
        }
    }

    /// Everything aboard comes off at once (unloadallinstant, a burrow standing down).
    pub fn unload_all(&mut self, h: u32) {
        for c in self.unit(h).map(|u| u.extra.cargo.clone()).unwrap_or_default() {
            self.disembark(c);
        }
    }

    /// A dead transport's cargo dies with it when it has Cargo Hold Death (Achd), and
    /// comes out otherwise.
    pub fn update_cargo(&mut self, vm: &mut Vm) {
        let carriers: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.extra.cargo.is_empty())).collect();
        for t in carriers {
            let Some(dead) = self.unit(t).map(|u| u.dead) else { continue };
            let cargo = self.unit(t).unwrap().extra.cargo.clone();
            if dead {
                let lost = self.ability_with_code(t, "Achd").is_some();
                for c in cargo {
                    self.disembark(c);
                    if lost {
                        crate::natives::units::kill_unit(self, vm, c, 0);
                    }
                }
                if let Some(u) = self.unit_mut(t) {
                    u.extra.cargo.clear();
                }
            }
        }
    }
}
