//! A player's order to several units to walk to a point, as the original's engine
//! moves them (traced: five Booty Bay peons sent across the ground together): each
//! unit's order without a path of its own, then one cluster and path for them all
//! (CLrCluster: `FUN_6f16bcf0`, `FUN_6f16de50`), each member walking straight to its
//! place in a formation about the point (`FUN_6f16a5b0`, the member's target its
//! cluster's point plus its place, `FUN_6f16e250`) and ending its walk there as an
//! arrival, the cluster's path going the step after the last does.

use serde::{Deserialize, Serialize};

use crate::efloat as ef;
use crate::world::World;

/// A cluster walking to a point: its path's objects (`l^pa`, `^clu`) and the members
/// still walking.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupWalk {
    pub path: [u32; 2],
    pub members: Vec<u32>,
    /// Whether its walk has begun.
    #[serde(default)]
    pub started: bool,
}

/// Units in a row of a rank by how many it holds, small units' then those of radius
/// 1.5 cells or more: with one rank (`DAT_6fa91d20`) and with more (`DAT_6fa91d88`).
const ROW_ONE: [u32; 26] = [0, 1, 2, 3, 2, 3, 3, 3, 3, 3, 4, 4, 4, 0, 1, 2, 3, 2, 3, 3, 3, 3, 3, 3, 3, 3];
const ROW_MORE: [u32; 26] = [0, 1, 2, 3, 4, 3, 3, 4, 4, 4, 4, 4, 4, 0, 1, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];
/// Cells between units in a row, between rows, and between ranks (read in the running
/// game: `DAT_6fd541c0`, `DAT_6fd541c4`, `DAT_6fd541bc`).
const ROW_GAP: f32 = 2.0;
const RANK_GAP: f32 = 2.5;
const NEXT_RANK: u32 = 0x40b0_0001;

/// The places of a cluster's members about its point, in cells, in the order they
/// joined (`FUN_6f16a5b0`): each rank's units (all one rank here) sorted to the front
/// by how far forward of the others they stand along `heading`, filling rows of the
/// table's width from the front, each row's units spread across the widest row's
/// width by how far to its side they stand; then the whole centered on its middle and
/// turned to `heading` (`FUN_6f16cb80`, `FUN_6f16d270`, `FUN_6f16a6e0`,
/// `FUN_6f16cf40`, `FUN_6f169e10`).
pub fn formation(at: &[(u32, u32)], radius: u32, heading: u32) -> Vec<(u32, u32)> {
    let f = |v: f32| v.to_bits();
    let lt = |a: u32, b: u32| f32::from_bits(a) < f32::from_bits(b);
    let n = at.len();
    if n == 1 {
        return vec![(0, 0)];
    }
    // Each member's place turned into the formation's frame (forward, side).
    let (s, c) = ef::sin_cos(heading ^ 0x8000_0000);
    let mut order: Vec<usize> = (0..n).collect();
    let mut fw: Vec<u32> = at.iter().map(|&(x, y)| ef::sub(ef::mul(x, c), ef::mul(y, s))).collect();
    let mut side: Vec<u32> = at.iter().map(|&(x, y)| ef::add(ef::mul(y, c), ef::mul(x, s))).collect();
    // The rank's width (`FUN_6f16d270`).
    let d = ef::add(radius, radius);
    let large = !lt(radius, f(1.5));
    let per_row = ROW_ONE[(large as usize) * 13 + n.min(12)].min(n as u32);
    let per_row_f = ef::from_int(per_row as i32);
    let width = ef::add(ef::mul(d, per_row_f), ef::mul(ef::add(d, f(ROW_GAP)), ef::sub(per_row_f, f(1.0))));
    let _ = ROW_MORE;
    // Its units by how far forward, the farthest first (`FUN_6f16dfd0`'s selection).
    let mut k = n;
    while k > 1 {
        k -= 1;
        let mut low = fw[k];
        let mut at_low = k;
        for j in (0..k).rev() {
            if lt(fw[j], low) {
                low = fw[j];
                at_low = j;
            }
        }
        if at_low != k {
            order.swap(k, at_low);
            fw.swap(k, at_low);
            side.swap(k, at_low);
        }
    }
    // Row by row (`FUN_6f16a6e0`, `FUN_6f16cf40`).
    let mut place = vec![(0u32, 0u32); n];
    let step_back = ef::add(d, f(RANK_GAP));
    let mut depth = 0u32;
    let mut placed = 0usize;
    loop {
        let k = (per_row as usize).min(n - placed);
        if k == 1 {
            place[order[placed]] = (depth, ef::mul(width, f(0.5)));
        } else {
            // The row's units by how far to the side, the least first (`FUN_6f16e090`).
            let mut end = placed + k;
            while placed + 1 < end {
                end -= 1;
                let mut high = side[end];
                let mut at_high = end;
                for j in (placed..end).rev() {
                    if lt(high, side[j]) {
                        high = side[j];
                        at_high = j;
                    }
                }
                if at_high != end {
                    order.swap(end, at_high);
                    fw.swap(end, at_high);
                    side.swap(end, at_high);
                }
            }
            let gap = ef::div(ef::sub(width, d), ef::from_int(k as i32 - 1));
            let start = ef::mul(d, f(0.5));
            for i in 0..k {
                place[order[placed + i]] = (depth, ef::add(start, ef::mul(gap, ef::from_int(i as i32))));
            }
        }
        placed += k;
        if placed >= n {
            break;
        }
        depth = ef::sub(depth, step_back);
    }
    let _ = NEXT_RANK;
    // Centered and turned (`FUN_6f16bb40`, `FUN_6f169e10`).
    let (mut mx, mut my) = (0u32, 0u32);
    for &(x, y) in &place {
        mx = ef::add(mx, x);
        my = ef::add(my, y);
    }
    let inv = ef::recip(ef::from_int(n as i32));
    let (mx, my) = (ef::mul(mx, inv), ef::mul(my, inv));
    let (s, c) = ef::sin_cos(heading);
    place
        .into_iter()
        .map(|(x, y)| {
            let (x, y) = (ef::sub(x, mx), ef::sub(y, my));
            (ef::sub(ef::mul(x, c), ef::mul(y, s)), ef::add(ef::mul(y, c), ef::mul(x, s)))
        })
        .collect()
}

impl World {
    /// A member's order of a group walk: as a player's walk's without its cluster and
    /// path, 14 objects (`Tags::member_move_issued`).
    pub(crate) fn start_group_member(&mut self, h: u32) {
        let serial = self.tags.born;
        let slots = self.tags.member_move_issued();
        if let Some(u) = self.unit_mut(h) {
            u.extra.order_tags = slots;
            u.extra.walk_serial = serial;
            u.extra.player_walk = true;
            u.extra.mover = None;
            u.path.clear();
        }
        if let Some(m) = self.group_walk_members.as_mut() {
            m.push(h);
        }
    }

    /// The cluster of units given a walk to `goal` together: its objects, and each
    /// member's place in the formation, toward which it walks.  The formation faces
    /// from the first member to the point (read: five peons' rows square to the line
    /// from the nearest of them, the first to take the order, to it).
    pub(crate) fn start_group_walk(&mut self, members: Vec<u32>, goal: (f32, f32)) {
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return };
        let f = |v: f32| v.to_bits();
        let cell = |w: f32, o: f32| ef::div(ef::sub(f(w), f(o)), f(crate::pathing::CELL));
        let cluster = self.tags.take(b"^clu");
        let path = [self.tags.take(b"l^pa"), cluster];
        let at: Vec<(u32, u32)> = members
            .iter()
            .filter_map(|&h| self.unit(h))
            .map(|u| match (u.extra.mover, u.extra.cell_at) {
                (Some(m), _) => (m.x, m.y),
                (None, Some((cx, cy, wx, wy))) if wx == u.x && wy == u.y => (cx, cy),
                _ => (cell(u.x, origin[0]), cell(u.y, origin[1])),
            })
            .collect();
        if at.len() != members.len() || at.is_empty() {
            return;
        }
        let g = (cell(goal.0, origin[0]), cell(goal.1, origin[1]));
        let (dx, dy) = (ef::sub(g.0, at[0].0), ef::sub(g.1, at[0].1));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let heading = if f32::from_bits(dist) > 0.0 { ef::heading(dx, dy, dist) } else { 0 };
        let radius = members.iter().filter_map(|&h| self.unit(h)).map(|u| (u.collision / crate::pathing::CELL).to_bits()).fold(0u32, |a, r| if f32::from_bits(r) > f32::from_bits(a) { r } else { a });
        let places = formation(&at, radius, heading);
        let world = |c: u32, o: f32| f32::from_bits(ef::add(ef::mul(c, f(crate::pathing::CELL)), f(o)));
        for (&h, &(px, py)) in members.iter().zip(&places) {
            let (tx, ty) = (ef::add(g.0, px), ef::add(g.1, py));
            let (wx, wy) = (world(tx, origin[0]), world(ty, origin[1]));
            if let Some(u) = self.unit_mut(h) {
                u.action = crate::types::Action::Move { x: wx, y: wy };
                u.path = vec![(wx, wy)];
                u.extra.group_walk = Some(path);
            }
        }
        self.group_walks.push(GroupWalk { path, members, started: false });
    }

    /// A member's first step: the cluster's first draws from the main generator once
    /// (read: the keepalives agree only with one draw as its walk begins, the turn after
    /// the order, none at the order).
    pub(crate) fn group_member_steps(&mut self, h: u32) {
        let Some(path) = self.unit(h).and_then(|u| u.extra.group_walk) else { return };
        if let Some(g) = self.group_walks.iter_mut().find(|g| g.path == path && !g.started) {
            g.started = true;
            self.rng.next_u32();
        }
    }

    /// A member's walk ended: its order's objects as an arrival's, without a path; the
    /// cluster's path the step after its last member's (read: turn 1732, the step after
    /// the last of five peons arrived).
    pub(crate) fn group_member_done(&mut self, h: u32, slots: &[u32], moved: bool) {
        if moved {
            for k in [7, 6, 5, 4, 3, 2, 0] {
                self.tags.release_in_step(slots[k]);
            }
        } else {
            for k in [7, 6, 5, 4, 3, 2, 0] {
                self.tags.release(slots[k]);
            }
        }
        let Some(path) = self.unit_mut(h).and_then(|u| u.extra.group_walk.take()) else { return };
        let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
        if let Some(i) = self.group_walks.iter().position(|g| g.path == path) {
            self.group_walks[i].members.retain(|&m| m != h);
            if self.group_walks[i].members.is_empty() {
                self.group_walks.remove(i);
                self.tags.walk_ended(path, serial);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five Booty Bay peons sent to (663.576, 253.778) cells together: their places as
    /// the original set them (the members' targets, +0x18 and +0x1c, read under winedbg).
    #[test]
    fn five_peons_take_their_places() {
        let f = |v: f32| v.to_bits();
        let cell = |w: f32, o: f32| ef::div(ef::sub(f(w), f(o)), f(32.0));
        // In the order they took it, nearest the point first.
        let at = [(668.796875f32, 262.0f32), (671.195313, 262.0), (668.0, 264.296875), (670.0, 266.0), (672.0, 264.296875)].map(|(x, y)| (f(x), f(y)));
        let g = (cell(f32::from_bits(0x4607_c9bf), -12544.0), cell(f32::from_bits(0x44d7_1d2d), -6400.0));
        let (dx, dy) = (ef::sub(g.0, at[0].0), ef::sub(g.1, at[0].1));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let places = formation(&at, f(0.5), ef::heading(dx, dy, dist));
        let got: Vec<String> = places.iter().map(|&(x, y)| format!("{:.6} {:.6}", f32::from_bits(ef::add(g.0, x)), f32::from_bits(ef::add(g.1, y)))).collect();
        assert_eq!(got, ["662.825562 252.596634", "666.202271 250.452377", "659.448853 254.740891", "661.325134 257.695496", "668.078552 253.406998"]);
    }
}
