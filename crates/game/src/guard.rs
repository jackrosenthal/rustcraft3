//! Creeps keep to their camps, and units call their friends to a fight (MiscGame):
//! a creep that has strayed GuardDistance from home and chased GuardReturnTime
//! seconds without being attacked heads home, as does one beyond MaxGuardDistance
//! whatever attacks it, or one left idle away from home.  A unit attacked calls the
//! idle friends within CallForHelp (CreepCallForHelp for creeps) to it.

use crate::types::*;
use crate::world::World;

impl World {
    pub fn update_guards(&mut self) {
        let (guard, max, wait) = (self.misc_f32("GuardDistance", 600.0), self.misc_f32("MaxGuardDistance", 1000.0), self.misc_f32("GuardReturnTime", 5.0));
        let now = self.time;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && u.extra.home.is_some())).collect();
        for h in units {
            let u = self.unit_mut(h).unwrap();
            if u.extra.home.is_some_and(|p| p.0.is_nan()) {
                u.extra.home = Some((u.x, u.y));
            }
            let (hx, hy) = u.extra.home.unwrap();
            let d = (u.x - hx).hypot(u.y - hy);
            let u = self.unit_mut(h).unwrap();
            if u.extra.returning {
                if !matches!(u.action, Action::Move { .. }) || d < 32.0 {
                    u.extra.returning = false;
                    u.extra.strayed = None;
                }
                continue;
            }
            if d <= guard {
                u.extra.strayed = None;
                continue;
            }
            let strayed = *u.extra.strayed.get_or_insert(now);
            let chasing = matches!(u.action, Action::Attack { .. });
            let idle = matches!(u.action, Action::Idle);
            let go = d > max || idle || chasing && now - strayed >= wait && now - u.extra.last_attacked >= wait;
            if go {
                u.extra.returning = true;
                u.action = Action::Move { x: hx, y: hy };
                u.path.clear();
                u.repath_timer = 0.0;
                u.swing = None;
            }
        }
    }

    /// The idle armed friends of an attacked unit within call-for-help range join in.
    pub(crate) fn call_for_help(&mut self, target: u32, attacker: u32) {
        let Some(t) = self.unit(target) else { return };
        let (owner, x, y) = (t.owner, t.x, t.y);
        if self.unit(attacker).is_none_or(|a| crate::natives::players::is_ally(self, owner, a.owner)) {
            return;
        }
        let key = if owner == PLAYER_NEUTRAL_AGGRESSIVE { "CreepCallForHelp" } else { "CallForHelp" };
        let range = self.misc_f32(key, 600.0);
        let friends: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&f| f != target)
            .filter(|&f| {
                self.unit(f).is_some_and(|u| {
                    u.owner == owner && !u.dead && !u.hidden && !u.is_building && !u.extra.returning && matches!(u.action, Action::Idle) && (u.x - x).hypot(u.y - y) <= range && !crate::sim::unit_weapons(self, u).is_empty()
                })
            })
            .collect();
        for f in friends {
            self.unit_mut(f).unwrap().action = Action::Attack { target: attacker };
        }
    }
}
