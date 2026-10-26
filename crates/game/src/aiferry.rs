//! A computer player's transports (FUN_6f607950 and the captain's boarding,
//! FUN_6f9d5e00): a ground unit the AI sends where it cannot walk boards one of its
//! player's transports, which takes it to a drop point it can walk on near the goal
//! and unloads it there.

use w3_jass::Vm;

use crate::objects::id_from_str;
use crate::pathing::{Move, CELL};
use crate::types::*;
use crate::world::World;

impl World {
    /// Whether a unit must be carried to reach a point: it walks, and no path joins
    /// where it stands to the nearest place it could be set down by the point (a
    /// building's own spot is never walkable).
    pub(crate) fn ai_needs_ferry(&self, h: u32, to: (f32, f32)) -> bool {
        let Some(u) = self.unit(h) else { return false };
        if u.extra.transport != 0 || self.move_kind(h) == Move::Fly || self.cargo_hold(h).is_some() {
            return false;
        }
        let Some(map) = &self.pathing else { return false };
        // Asked at the smallest size, so only water or cliffs part them, never a gap
        // between buildings too narrow for this unit.
        let at = self.ai_drop_point(to);
        !map.reaches((u.x, u.y), at, 16.0, 16.0 + 2.0 * CELL)
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

    /// The point near `to` where a walker can be set down: the nearest cell it fits,
    /// searched outward (beside a building, when `to` is one).
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
                        return map.center(cell);
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

    /// LoadZepWave (FUN_6f9d3a00): every unit of the attack captain boards a transport
    /// bound for a point, where one will take it.
    pub(crate) fn ai_load_zep_wave(&mut self, p: u8, to: (f32, f32)) {
        let units = self.ai.get(&p).map(|s| s.attack.units.clone()).unwrap_or_default();
        for h in units {
            if self.unit(h).is_some_and(|u| !u.dead && u.extra.transport == 0) && self.cargo_hold(h).is_none() {
                self.ai_ferry(p, h, to);
            }
        }
    }

    /// PurchaseZeppelin (FUN_6f9d5190): the attack captain goes to a shop that sells
    /// goblin zeppelins, the first found kept for later purchases, to buy one there.
    pub(crate) fn ai_purchase_zeppelin(&mut self, p: u8) {
        let nzep = id_from_str("nzep");
        let kept = self.ai.get(&p).map_or(0, |s| s.zep_shop);
        let sells = |w: &World, h: u32| w.unit(h).is_some_and(|u| !u.dead) && w.shop_goods(h).1.contains(&nzep);
        let shop = if kept != 0 && sells(self, kept) {
            kept
        } else {
            let home = self.ai.get(&p).map_or((0.0, 0.0), |s| s.attack.home);
            let near = |w: &World, h: u32| w.unit(h).map_or(f32::MAX, |u| (u.x - home.0).hypot(u.y - home.1));
            let Some(h) = self.unit_order.iter().copied().filter(|&h| sells(self, h)).min_by(|&a, &b| near(self, a).total_cmp(&near(self, b))) else { return };
            h
        };
        let Some(at) = self.unit(shop).map(|u| (u.x, u.y)) else { return };
        if let Some(st) = self.ai.get_mut(&p) {
            st.zep_shop = shop;
            st.zep_buying = true;
            st.attack_player = None;
            st.attack.goal = Some(at);
            st.attack.target = 0;
            st.attack.retreating = false;
        }
    }

    /// A purchase PurchaseZeppelin sent the attack captain to make: made once one of its
    /// player's units is near the shop, given up when the captain is sent elsewhere.
    pub(crate) fn ai_zeppelin_tick(&mut self, vm: &mut Vm) {
        let buying: Vec<(u8, u32)> = self.ai.iter().filter(|(_, s)| s.zep_buying).map(|(&p, s)| (p, s.zep_shop)).collect();
        for (p, shop) in buying {
            let at = self.unit(shop).filter(|u| !u.dead).map(|u| (u.x, u.y));
            let still = at.is_some() && self.ai.get(&p).is_some_and(|s| s.attack.goal == at);
            if !still || self.hire_unit(vm, p, shop, id_from_str("nzep")).is_ok() {
                if let Some(st) = self.ai.get_mut(&p) {
                    st.zep_buying = false;
                }
            }
        }
    }
}
