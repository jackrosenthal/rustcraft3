//! A computer player's transports (FUN_6f607950 and the captain's boarding,
//! FUN_6f9d5e00): a ground unit the AI sends where it cannot walk boards one of its
//! player's transports, which takes it to a drop point it can walk on near the goal
//! and unloads it there.

use crate::pathing::{Move, CELL};
use crate::types::*;
use crate::world::World;

impl World {
    /// Whether a unit must be carried to reach a point: it walks, and no path joins
    /// where it stands to the point.
    pub(crate) fn ai_needs_ferry(&self, h: u32, to: (f32, f32)) -> bool {
        let Some(u) = self.unit(h) else { return false };
        if u.extra.transport != 0 || self.move_kind(h) == Move::Fly || self.cargo_hold(h).is_some() {
            return false;
        }
        let Some(map) = &self.pathing else { return false };
        let radius = u.collision.max(16.0);
        !map.reaches((u.x, u.y), to, radius, radius + 2.0 * CELL)
    }

    /// Puts a unit aboard one of its player's transports bound for `to`: the nearest
    /// with room that can come to it.  False if there is none.
    pub(crate) fn ai_ferry(&mut self, p: u8, h: u32, to: (f32, f32)) -> bool {
        let Some((ux, uy)) = self.unit(h).map(|u| (u.x, u.y)) else { return false };
        let busy: Vec<u32> = self.ai.get(&p).map(|s| s.ferries.iter().filter(|f| f.1 != to).map(|f| f.0).collect()).unwrap_or_default();
        let mut best: Option<(f32, u32)> = None;
        for &t in &self.unit_order {
            let Some(tu) = self.unit(t) else { continue };
            if tu.owner != p || tu.dead || busy.contains(&t) || !self.can_board(t, h) || matches!(tu.action, Action::Unload { .. }) {
                continue;
            }
            let comes = self.move_kind(t) == Move::Fly || self.pathing.as_ref().is_none_or(|m| m.reaches((tu.x, tu.y), (ux, uy), tu.collision.max(16.0), 300.0));
            if !comes {
                continue;
            }
            let d = (tu.x - ux).hypot(tu.y - uy);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, t));
            }
        }
        let Some((_, t)) = best else { return false };
        if let Some(u) = self.unit_mut(h) {
            u.action = Action::Board { transport: t };
            u.order = Order { id: crate::natives::basic::order_id("board"), target_x: 0.0, target_y: 0.0, target: t };
            u.path.clear();
        }
        if let Some(st) = self.ai.get_mut(&p) {
            if !st.ferries.iter().any(|f| f.0 == t) {
                st.ferries.push((t, to));
            }
        }
        true
    }

    /// The point near `to` where a walker can be set down and walk to it: the nearest
    /// cell it fits that joins `to`, searched outward.
    fn ai_drop_point(&self, to: (f32, f32)) -> (f32, f32) {
        let Some(map) = &self.pathing else { return to };
        let c = map.cell_of(to.0, to.1);
        if map.fits(c, 16.0) {
            return to;
        }
        for r in 1..64i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let cell = (c.0 + dx * 2, c.1 + dy * 2);
                    if map.fits(cell, 16.0) {
                        let at = map.center(cell);
                        if map.reaches(at, to, 16.0, 16.0 + 2.0 * CELL) {
                            return at;
                        }
                    }
                }
            }
        }
        to
    }

    /// A player's transports underway: once no unit is still coming aboard, a loaded
    /// one goes to its drop point and unloads; an empty one that has done so is free.
    pub(crate) fn ai_ferries_update(&mut self, p: u8) {
        let ferries = self.ai.get(&p).map(|s| s.ferries.clone()).unwrap_or_default();
        let mut keep = Vec::new();
        for (t, to) in ferries {
            let Some(tu) = self.unit(t).filter(|u| !u.dead) else { continue };
            let (loaded, unloading) = (!tu.extra.cargo.is_empty(), matches!(tu.action, Action::Unload { .. }));
            let coming = self.unit_order.iter().any(|&h| self.unit(h).is_some_and(|u| !u.dead && matches!(u.action, Action::Board { transport } if transport == t)));
            if coming {
                keep.push((t, to));
                continue;
            }
            if loaded && !unloading {
                let (x, y) = self.ai_drop_point(to);
                if let Some(u) = self.unit_mut(t) {
                    u.action = Action::Unload { x, y };
                    u.order = Order { id: crate::natives::basic::order_id("unloadall"), target_x: x, target_y: y, target: 0 };
                    u.path.clear();
                    u.work_timer = 0.0;
                }
                keep.push((t, to));
            } else if loaded || unloading {
                keep.push((t, to));
            }
        }
        if let Some(st) = self.ai.get_mut(&p) {
            st.ferries = keep;
        }
    }

    /// Whether a unit is a transport carrying for its AI.
    pub(crate) fn ai_ferrying(&self, p: u8, h: u32) -> bool {
        self.ai.get(&p).is_some_and(|s| s.ferries.iter().any(|f| f.0 == h))
    }
}
