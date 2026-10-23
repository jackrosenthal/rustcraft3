//! Waygates (CAbilityWarp): an active waygate with a destination (WaygateSetDestination,
//! WaygateActivate) makes a warp in the pathing (NIpse::CPaWarp) over the box DataA
//! wide and DataB tall centered on it (half of each, CAbilityWarp vf32); a ground unit
//! that walks into the box comes out at the destination, at the nearest spot it fits.

use crate::types::*;
use crate::world::World;

impl World {
    /// The active waygates: their handle, box, and destination.
    fn warps(&self) -> Vec<(u32, [f32; 4], (f32, f32))> {
        self.unit_order
            .iter()
            .filter_map(|&h| {
                let u = self.unit(h)?;
                let (dx, dy, on) = u.extra.waygate?;
                let (a, lvl) = self.ability_with_code(h, "Awrp")?;
                if !on || u.dead {
                    return None;
                }
                let lvl = lvl.max(1) as u32;
                let (hw, hh) = (self.data(a, "DataA", lvl) * 0.5, self.data(a, "DataB", lvl) * 0.5);
                Some((h, [u.x - hw, u.y - hh, u.x + hw, u.y + hh], (dx, dy)))
            })
            .collect()
    }

    pub fn update_waygates(&mut self) {
        let warps = self.warps();
        if warps.is_empty() {
            return;
        }
        let inside = |b: &[f32; 4], x: f32, y: f32| x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3];
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && !u.is_building)).collect();
        for h in units {
            if self.move_kind(h) == crate::pathing::Move::Fly {
                continue;
            }
            let Some(u) = self.unit(h) else { continue };
            let (x, y, last) = (u.x, u.y, u.extra.warp_inside);
            let gate = warps.iter().find(|w| w.0 != h && inside(&w.1, x, y));
            let Some(&(g, _, (dx, dy))) = gate else {
                if last != 0 {
                    self.unit_mut(h).unwrap().extra.warp_inside = 0;
                }
                continue;
            };
            // Only on the way in: one that came out inside a gate's box stays.
            if last == g {
                continue;
            }
            let radius = u.collision;
            let (nx, ny) = self.free_point(dx, dy, radius);
            let arrived_in = warps.iter().find(|w| inside(&w.1, nx, ny)).map_or(0, |w| w.0);
            let u = self.unit_mut(h).unwrap();
            u.x = nx;
            u.y = ny;
            u.path.clear();
            u.repath_timer = 0.0;
            u.extra.warp_inside = arrived_in;
            // A unit sent into the gate itself is done.
            if matches!(u.action, Action::Move { x, y } if warps.iter().any(|w| w.0 == g && inside(&w.1, x, y))) || matches!(u.action, Action::Follow { target } if target == g) {
                u.action = Action::Idle;
            }
        }
    }

    /// The nearest spot to a point where a unit of `radius` fits.
    pub(crate) fn free_point(&self, x: f32, y: f32, radius: f32) -> (f32, f32) {
        let Some(p) = &self.pathing else { return (x, y) };
        let c = p.cell_of(x, y);
        if p.fits(c, radius * 0.5) {
            return (x, y);
        }
        for r in 1..64i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let cc = (c.0 + dx, c.1 + dy);
                    if p.fits(cc, radius * 0.5) {
                        return p.center(cc);
                    }
                }
            }
        }
        (x, y)
    }
}
