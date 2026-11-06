//! The original's accelerator plan (NIpse's CLrPathingAcc, `FUN_6f162cb0`): a path's
//! coarse way, searched over the accelerator pyramid (`crate::accel`) by the same A*
//! and heap as the local search (`crate::lrpath`), its nodes the largest open blocks.
//! Points are in accelerator units (half pathing cells), as the path keeps them.

use crate::accel::{Pyramid, State};
use crate::lrpath::Heap;
use std::collections::HashMap;

/// The engine's integer square root (`FUN_6f1d58e0`): Newton's method from n / 12 + 1,
/// n / 200 + 21, or n / 26743 + 444, until a step's half rounds to nothing.
pub fn isqrt(n: u32) -> u32 {
    let mut x: u32 = if n < 0x100 {
        n / 12 + 1
    } else if n < 0x10000 {
        n / 200 + 21
    } else {
        n / 26743 + 444
    };
    loop {
        let q = n / x;
        let diff = x as i32 - q as i32;
        x = ((q + x) as i32 / 2) as u32;
        if diff / 2 == 0 {
            return x;
        }
    }
}

/// A step's or estimate's cost between two cells (`FUN_6f164020`): 24 a cell.
fn cost(a: (i32, i32), b: (i32, i32)) -> u32 {
    let dx = (a.0 - b.0).unsigned_abs() * 24;
    let dy = (a.1 - b.1).unsigned_abs() * 24;
    isqrt(dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Link {
    /// Never pushed (or being expanded): +0xc -1.
    New,
    /// In the heap: -2.
    Open,
    /// Expanded, in the closed list.
    Closed,
}

/// A node (0x24 bytes): the level-0 cell it was first reached from, its block's
/// level (+0x22), stamp (+8), link (+0xc), g (+0x14), h (+0x18), and parent (+0x1c).
#[derive(Clone, Debug)]
struct Node {
    x: i32,
    y: i32,
    level: u8,
    stamp: u32,
    link: Link,
    g: u32,
    h: u32,
    parent: Option<usize>,
}

/// A walker's coarse way and its place on it (a path's +0x60 points, +0x78 index), and
/// the step of its local way's last search.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Coarse {
    pub points: Vec<(f32, f32)>,
    pub index: usize,
    pub searched_at: u64,
}

/// A coarse way: its points from the goal's (first) to the start's (last), whether
/// the goal was reached (else it ends at the node nearest it), and the search's pops,
/// its cost to the budget (`FUN_6f166c30` adding the system's +0x9c).
#[derive(Clone, Debug, PartialEq)]
pub struct Way {
    pub points: Vec<(f32, f32)>,
    pub found: bool,
    pub pops: u32,
}

struct Search<'a> {
    pyr: &'a Pyramid,
    layer: usize,
    nodes: Vec<Node>,
    keys: HashMap<(u8, i64, i64), usize>,
    heap: Heap<usize>,
    goal: (i32, i32),
    goal_node: Option<usize>,
    best: usize,
    best_d: u32,
    /// Its pops (+0x9c, `FUN_6f164230` clearing it): one more for the try a limit stops.
    pops: u32,
}

impl Search<'_> {
    fn state(&self, level: usize, x: i32, y: i32) -> Option<State> {
        if x < 0 || y < 0 {
            return None;
        }
        self.pyr.levels.get(level)?.get((x >> level) as i64, (y >> level) as i64).map(|c| c[self.layer])
    }

    /// A cell's node at `level` (`FUN_6f1625f0`): blocked -1; mixed -1 at level 0, else
    /// -2; open, the largest open block holding it, made as first reached from x, y.
    fn lookup(&mut self, level: usize, x: i32, y: i32) -> i64 {
        match self.state(level, x, y) {
            None | Some(State::Blocked) => return -1,
            Some(State::Mixed) => return if level != 0 { -2 } else { -1 },
            Some(State::Open) => {}
        }
        let mut l = level;
        while l < 3 && self.state(l + 1, x, y) == Some(State::Open) {
            l += 1;
        }
        let key = (l as u8, (x >> l) as i64, (y >> l) as i64);
        if let Some(&n) = self.keys.get(&key) {
            return n as i64;
        }
        self.nodes.push(Node { x, y, level: l as u8, stamp: 0, link: Link::New, g: 0, h: 0, parent: None });
        let n = self.nodes.len() - 1;
        self.keys.insert(key, n);
        n as i64
    }

    fn push(&mut self, n: usize) {
        let node = &mut self.nodes[n];
        node.stamp = node.stamp.wrapping_add(1);
        node.link = Link::Open;
        let e = (node.g.wrapping_add(node.h), n, node.stamp);
        self.heap.push(e);
    }

    /// `FUN_6f164020`: node `n` reached from `from`; a new one weighed as the nearest
    /// to the goal, an old one only when the way is shorter.
    fn relax(&mut self, n: usize, from: usize) {
        let (fx, fy, fg) = (self.nodes[from].x, self.nodes[from].y, self.nodes[from].g);
        let at = (self.nodes[n].x, self.nodes[n].y);
        let g = cost((fx, fy), at).wrapping_add(fg);
        let h = cost(at, self.goal);
        match self.nodes[n].link {
            Link::New => {
                let (dx, dy) = (at.0 - self.goal.0, at.1 - self.goal.1);
                let d = (dx * dx + dy * dy) as u32;
                if d < self.best_d {
                    self.best_d = d;
                    self.best = n;
                }
            }
            _ => {
                if self.nodes[n].g <= g {
                    return;
                }
                let node = &mut self.nodes[n];
                node.stamp = node.stamp.wrapping_add(1);
                node.link = Link::New;
            }
        }
        let node = &mut self.nodes[n];
        node.parent = Some(from);
        node.g = g;
        node.h = h;
        self.push(n);
    }

    /// A single cell's neighbour (`FUN_6f163ae0` and its fellows): relaxed when open.
    fn side0(&mut self, n: usize, x: i32, y: i32) -> bool {
        let r = self.lookup(0, x, y);
        if r < 0 {
            return false;
        }
        self.relax(r as usize, n);
        true
    }

    /// A level-0 cell's expansion (`FUN_6f1643d0`): its four sides (below, right,
    /// above, left), then the corners whose two sides were open.
    fn expand_cell(&mut self, n: usize) {
        let (x, y) = (self.nodes[n].x, self.nodes[n].y);
        let s = self.side0(n, x, y - 1);
        let e = self.side0(n, x + 1, y);
        let up = self.side0(n, x, y + 1);
        let w = self.side0(n, x - 1, y);
        if s && e {
            self.side0(n, x + 1, y - 1);
        }
        if e && up {
            self.side0(n, x + 1, y + 1);
        }
        if up && w {
            self.side0(n, x - 1, y + 1);
        }
        if w && s {
            self.side0(n, x - 1, y - 1);
        }
    }

    /// One side of a block (`FUN_6f163bc0` below, `FUN_6f163260` right, `FUN_6f1648f0`
    /// above, `FUN_6f165090` left): the neighbouring block at `level`, a mixed one
    /// split in halves along the side, the lower first; `first` and `last` set when
    /// the side's first and last parts were open blocks above level 0.
    fn side_walk(&mut self, n: usize, mut level: usize, x: i32, y: i32, along_x: bool, first: &mut bool, last: &mut bool) {
        let mut pos = if along_x { x } else { y };
        let at = |pos: i32| if along_x { (pos, y) } else { (x, pos) };
        let mut r = { let (a, b) = at(pos); self.lookup(level, a, b) };
        let mut dummy = false;
        let mut first_ref: &mut bool = first;
        while r < 0 {
            if r != -2 {
                return;
            }
            let old = level;
            level -= 1;
            let mid = (1 << level) + ((pos >> old) << old);
            let (lower, upper) = if mid <= pos { (mid - 1, pos) } else { (pos, mid) };
            let (a, b) = at(lower);
            let mut scratch = false;
            self.side_walk(n, level, a, b, along_x, first_ref, &mut scratch);
            first_ref = &mut dummy;
            pos = upper;
            let (a, b) = at(pos);
            r = self.lookup(level, a, b);
        }
        self.relax(r as usize, n);
        if level != 0 {
            *last = true;
            *first_ref = true;
        }
    }

    /// A block's corner (`FUN_6f163970` and its fellows): the cell's block at the
    /// level, or below while mixed.
    fn corner(&mut self, n: usize, mut level: usize, x: i32, y: i32) {
        loop {
            let r = self.lookup(level, x, y);
            if r >= 0 {
                self.relax(r as usize, n);
                return;
            }
            if r != -2 {
                return;
            }
            level -= 1;
        }
    }

    /// A block's expansion (`FUN_6f1644d0`): its four sides, then the corners whose
    /// two sides' ends were open blocks.
    fn expand_block(&mut self, n: usize) {
        let (x, y, l) = (self.nodes[n].x, self.nodes[n].y, self.nodes[n].level as usize);
        let size = 1 << l;
        let (bx, by) = ((x >> l) << l, (y >> l) << l);
        let (mut s_first, mut s_last, mut e_first, mut e_last) = (false, false, false, false);
        let (mut n_first, mut n_last, mut w_first, mut w_last) = (false, false, false, false);
        self.side_walk(n, l, x, by - 1, true, &mut s_first, &mut s_last);
        self.side_walk(n, l, bx + size, y, false, &mut e_first, &mut e_last);
        self.side_walk(n, l, x, by + size, true, &mut n_first, &mut n_last);
        self.side_walk(n, l, bx - 1, y, false, &mut w_first, &mut w_last);
        if s_last && e_first {
            self.corner(n, l, bx + size, by - 1);
        }
        if e_last && n_last {
            self.corner(n, l, bx + size, by + size);
        }
        if n_first && w_last {
            self.corner(n, l, bx - 1, by + size);
        }
        if w_first && s_first {
            self.corner(n, l, bx - 1, by - 1);
        }
    }

    /// `FUN_6f163f50`: pops (at most `limit`, stale entries counting) until the goal's
    /// node comes off the heap.
    fn run(&mut self, limit: u32) -> Option<usize> {
        while self.heap.len() > 1 {
            self.pops += 1;
            if self.pops > limit {
                break;
            }
            let (_, n, stamp) = self.heap.pop();
            if self.nodes[n].stamp != stamp {
                continue;
            }
            self.nodes[n].link = Link::New;
            self.nodes[n].stamp = self.nodes[n].stamp.wrapping_add(1);
            if Some(n) == self.goal_node {
                return Some(n);
            }
            if self.nodes[n].level == 0 {
                self.expand_cell(n);
            } else {
                self.expand_block(n);
            }
            let node = &mut self.nodes[n];
            node.stamp = node.stamp.wrapping_add(1);
            node.link = Link::Closed;
        }
        None
    }
}

/// A path's cost from `start` to `goal` (accelerator units), as `FUN_6f1627e0` gives
/// it: within one node, the two places' straight length doubled (`isqrt` of the
/// doubled whole units' squares); else, the goal's node found within `limit` pops,
/// the way back's legs so (`FUN_6f163440`, each from a node's remembered cell to its
/// parent's); none found, none.  Provisional: a leg the original flags (its +0x23)
/// counts 2, not yet told apart here.
pub fn plan_cost(pyr: &Pyramid, layer: usize, start: (f32, f32), goal: (f32, f32), limit: u32) -> Option<u32> {
    let s = (start.0.floor() as i32, start.1.floor() as i32);
    let g = (goal.0.floor() as i32, goal.1.floor() as i32);
    let straight = |a: (i32, i32), b: (i32, i32)| {
        let (dx, dy) = ((a.0 - b.0).abs() * 2, (a.1 - b.1).abs() * 2);
        isqrt((dx * dx + dy * dy) as u32)
    };
    let mut search = Search { pyr, layer, nodes: Vec::new(), keys: HashMap::new(), heap: Heap(vec![(0, 0, 0)]), goal: g, goal_node: None, best: 0, best_d: 0, pops: 0 };
    search.state(0, s.0, s.1)?;
    if s == g {
        return Some(0);
    }
    // (A start in no node is costed straight too: `FUN_6f164c30` answering 1.)
    let sn = search.lookup(0, s.0, s.1);
    if sn < 0 {
        return Some(straight(s, g));
    }
    let gn = search.lookup(0, g.0, g.1);
    if gn == sn {
        return Some(straight(s, g));
    }
    let sn = sn as usize;
    search.goal_node = (gn >= 0).then_some(gn as usize);
    search.best = sn;
    search.best_d = ((s.0 - g.0) * (s.0 - g.0) + (s.1 - g.1) * (s.1 - g.1)) as u32;
    search.nodes[sn].h = cost(s, g);
    search.push(sn);
    let end = search.run(limit)?;
    let mut total = 0;
    let mut at = end;
    while let Some(p) = search.nodes[at].parent {
        let (a, b) = (&search.nodes[at], &search.nodes[p]);
        total += straight((a.x, a.y), (b.x, b.y));
        at = p;
    }
    Some(total)
}

/// A path's coarse way from `start` to `goal` (accelerator units) for a unit of size 1
/// on `layer`, popping at most `limit` nodes (`FUN_6f164c30`, `FUN_6f164a20`,
/// `FUN_6f162a30`): the goal alone when both stand in one node (or the start is off
/// the map); else each node of the way back from the goal's node, or from the node
/// nearest it, a point its remembered cell plus 0.75, the first the goal (or the
/// nearest node's middle) and the last the start.
pub fn plan(pyr: &Pyramid, layer: usize, start: (f32, f32), goal: (f32, f32), limit: u32) -> Way {
    let s = (start.0.floor() as i32, start.1.floor() as i32);
    let g = (goal.0.floor() as i32, goal.1.floor() as i32);
    let mut search = Search { pyr, layer, nodes: Vec::new(), keys: HashMap::new(), heap: Heap(vec![(0, 0, 0)]), goal: g, goal_node: None, best: 0, best_d: 0, pops: 0 };
    if search.state(0, s.0, s.1).is_none() || s == g {
        return Way { points: vec![goal], found: true, pops: 0 };
    }
    let sn = search.lookup(0, s.0, s.1);
    if sn < 0 {
        return Way { points: vec![goal], found: true, pops: 0 };
    }
    let gn = search.lookup(0, g.0, g.1);
    if gn == sn {
        return Way { points: vec![goal], found: true, pops: 0 };
    }
    let sn = sn as usize;
    search.goal_node = (gn >= 0).then_some(gn as usize);
    search.best = sn;
    search.best_d = ((s.0 - g.0) * (s.0 - g.0) + (s.1 - g.1) * (s.1 - g.1)) as u32;
    search.nodes[sn].h = cost(s, g);
    search.push(sn);
    let (end, end_point, found) = match search.run(limit) {
        Some(n) => (n, goal, true),
        None if search.best == sn => return Way { points: vec![start], found: false, pops: search.pops },
        None => {
            let b = &search.nodes[search.best];
            (search.best, (b.x as f32 + 0.5, b.y as f32 + 0.5), false)
        }
    };
    let mut points = Vec::new();
    let mut at = Some(end);
    while let Some(n) = at {
        let node = &search.nodes[n];
        points.push((node.x as f32 + 0.75, node.y as f32 + 0.75));
        at = node.parent;
    }
    if let Some(p) = points.last_mut() {
        *p = start;
    }
    points[0] = end_point;
    Way { points, found, pops: search.pops }
}

/// The coarse way's next target for a walker at `at` (`FUN_6f167ae0`): from the point
/// below `index` toward the goal, the first at least `reach` away (10, `DAT_6fd54194`);
/// none before the goal, the goal (index 0).
pub fn next_target(points: &[(f32, f32)], index: usize, at: (f32, f32), reach: f32) -> usize {
    let mut i = index;
    while i > 1 {
        i -= 1;
        let (dx, dy) = (points[i].0 - at.0, points[i].1 - at.1);
        if (dx * dx + dy * dy).sqrt() >= reach {
            return i;
        }
    }
    0
}

/// The engine's pick of a path's next target on its coarse way (`FUN_6f167ae0`): from
/// the point below `index` toward the goal (index 0), the first where the way's legs
/// from `index`, summed (each the engine's root of its squares, `FUN_6f071480`), come to
/// `reach` (10, `DAT_6fd54194`); none before the goal, the goal.
pub fn pick(points: &[(f32, f32)], index: usize, reach: f32) -> usize {
    use crate::efloat as ef;
    let mut sum = 0u32;
    let mut i = index.min(points.len().saturating_sub(1));
    while i > 1 {
        i -= 1;
        let (a, b) = (points[i], points[i + 1]);
        let (dx, dy) = (ef::sub(b.0.to_bits(), a.0.to_bits()), ef::sub(b.1.to_bits(), a.1.to_bits()));
        sum = ef::add(sum, ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy))));
        if f32::from_bits(sum) >= reach {
            return i;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pick sums the way's legs (read: Mafarazzo TD's creeps, their coarse ways
    /// down x = 35.75 by 8 from 135.25, each first walking to 119.75).
    #[test]
    fn picks_by_the_way_walked() {
        let way = [(35.5, 79.0), (35.75, 87.75), (35.75, 95.75), (35.75, 103.75), (35.75, 111.75), (35.75, 119.75), (35.75, 127.75), (35.75, 135.25)];
        assert_eq!(pick(&way, 7, 10.0), 5);
        assert_eq!(pick(&way, 5, 10.0), 3);
        assert_eq!(pick(&way, 3, 10.0), 1);
        assert_eq!(pick(&way, 1, 10.0), 0);
        // A bend: two legs of 6 at right angles reach 10 where the straight line does not.
        let bend = [(0.0, 0.0), (6.0, 6.0), (6.0, 0.0), (0.0, 0.0)];
        assert_eq!(pick(&bend, 3, 10.0), 1);
    }

    #[test]
    fn isqrt_is_the_engines() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(576), 24);
        assert_eq!(isqrt(1152), 33);
        assert_eq!(isqrt(100_000_000), 10000);
    }

    /// A map all open: the way is the goal alone when both stand in one block, else
    /// runs between the blocks.
    #[test]
    fn plans_over_open_ground() {
        let p = Pyramid::build(64, 64, |_, _, _| false);
        let w = plan(&p, 0, (1.25, 1.25), (1.5, 1.75), 100);
        assert_eq!(w.points, vec![(1.5, 1.75)]);
        let w = plan(&p, 0, (1.25, 1.25), (20.5, 20.5), 1000);
        assert!(w.found);
        assert_eq!(w.points.first(), Some(&(20.5, 20.5)));
        assert_eq!(w.points.last(), Some(&(1.25, 1.25)));
    }
}
