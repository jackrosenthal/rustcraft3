//! The pathing grid: `war3map.wpm` cells (32x32 world units) plus dynamic blockers
//! stamped by destructables and structures, and A* search over it.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const CELL: f32 = 32.0;

// war3map.wpm flags, as game.dll's loader (0x6f04cXXX near the 'MP3W' check) reads them.
pub const NO_WALK: u8 = 0x02;
pub const NO_FLY: u8 = 0x04;
pub const NO_BUILD: u8 = 0x08;
pub const LAND: u8 = 0x40;

/// How a unit moves (UnitData movetp): on foot (also horse and hover), flying, floating
/// on water, or amphibious.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Move {
    #[default]
    Foot,
    Fly,
    Float,
    Amphibious,
}

impl Move {
    pub fn from_movetp(t: &str) -> Move {
        match t.to_ascii_lowercase().as_str() {
            "fly" => Move::Fly,
            "float" => Move::Float,
            "amph" => Move::Amphibious,
            _ => Move::Foot,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PathingMap {
    pub width: u32,
    pub height: u32,
    /// World position of cell (0, 0)'s lower-left corner.
    pub origin: [f32; 2],
    pub flags: Vec<u8>,
    /// Count of dynamic blockers per cell (walk-blocking).
    pub blocked: Vec<u16>,
    /// The search's working arrays, kept between searches.
    #[serde(skip)]
    scratch: std::cell::RefCell<Scratch>,
    /// Paths found by [`PathingMap::find_path_memo`] since the map last changed.
    #[serde(skip)]
    memo: std::cell::RefCell<std::collections::HashMap<[u32; 6], Vec<(f32, f32)>>>,
    /// Cells reachable on foot from a start cell for a radius, since the map last
    /// changed ([`PathingMap::reaches`]).
    #[serde(skip)]
    floods: std::cell::RefCell<std::collections::HashMap<(i32, i32, u32), std::rc::Rc<Vec<bool>>>>,
}

/// A search's costs and parents, valid where `stamp` is the search's `generation`, so
/// a search need not clear the whole map's worth.
#[derive(Default)]
struct Scratch {
    g: Vec<f32>,
    parent: Vec<u32>,
    stamp: Vec<u32>,
    generation: u32,
    /// Searches, cells expanded, and searches that gave up (for W3PROF).
    pub stats: (u64, u64, u64),
}

impl PathingMap {
    /// [`PathingMap::find_path`], remembered until the map changes: the same search
    /// on the same map finds the same path.
    pub fn find_path_memo(&self, from: (f32, f32), to: (f32, f32), radius: f32, max_nodes: usize) -> Vec<(f32, f32)> {
        let key = [from.0.to_bits(), from.1.to_bits(), to.0.to_bits(), to.1.to_bits(), radius.to_bits(), max_nodes as u32];
        if let Some(p) = self.memo.borrow().get(&key) {
            return p.clone();
        }
        let p = self.find_path(from, to, radius, max_nodes);
        self.memo.borrow_mut().insert(key, p.clone());
        p
    }

    /// Whether a walker of `radius` at `from` can get within `near` of `to`: a cell it
    /// fits there is connected to where it stands (one flood of the map per start,
    /// kept until the map changes).
    pub fn reaches(&self, from: (f32, f32), to: (f32, f32), radius: f32, near: f32) -> bool {
        let mut start = self.cell_of(from.0, from.1);
        if !self.fits(start, radius) {
            // As a search starts: from the nearest cell it fits.
            let found = (1..=16i32).find_map(|r| {
                (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|(dx, dy)| dx.abs() == r || dy.abs() == r).map(|(dx, dy)| (start.0 + dx, start.1 + dy)).find(|&c| self.index(c).is_some() && self.fits(c, radius))
            });
            match found {
                Some(c) => start = c,
                None => return false,
            }
        }
        let key = (start.0, start.1, radius.to_bits());
        let flood = self.floods.borrow().get(&key).cloned();
        let flood = match flood {
            Some(f) => f,
            None => {
                let mut seen = vec![false; (self.width * self.height) as usize];
                let mut queue = std::collections::VecDeque::new();
                if let Some(i) = self.index(start) {
                    seen[i] = true;
                    queue.push_back(start);
                }
                while let Some(c) = queue.pop_front() {
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                        let nc = (c.0 + dx, c.1 + dy);
                        let Some(ni) = self.index(nc) else { continue };
                        if seen[ni] || !self.fits(nc, radius) {
                            continue;
                        }
                        if dx != 0 && dy != 0 && (!self.fits((c.0 + dx, c.1), radius) || !self.fits((c.0, c.1 + dy), radius)) {
                            continue;
                        }
                        seen[ni] = true;
                        queue.push_back(nc);
                    }
                }
                let f = std::rc::Rc::new(seen);
                self.floods.borrow_mut().insert(key, f.clone());
                f
            }
        };
        let goal = self.cell_of(to.0, to.1);
        let r = (near / CELL).ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let c = (goal.0 + dx, goal.1 + dy);
                if let Some(i) = self.index(c) {
                    let p = self.center(c);
                    if flood[i] && (p.0 - to.0).hypot(p.1 - to.1) <= near {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Searches made, cells they expanded, and how many gave up at their limit.
    pub fn search_stats(&self) -> (u64, u64, u64) {
        self.scratch.borrow().stats
    }
}

impl PathingMap {
    pub fn parse(data: &[u8], origin: [f32; 2]) -> Option<PathingMap> {
        if data.len() < 16 || &data[0..4] != b"MP3W" {
            return None;
        }
        let w = u32::from_le_bytes(data[8..12].try_into().ok()?);
        let h = u32::from_le_bytes(data[12..16].try_into().ok()?);
        let flags = data.get(16..16 + (w * h) as usize)?.to_vec();
        Some(PathingMap { width: w, height: h, origin, flags, blocked: vec![0; (w * h) as usize], scratch: Default::default(), memo: Default::default(), floods: Default::default() })
    }

    pub fn cell_of(&self, x: f32, y: f32) -> (i32, i32) {
        (((x - self.origin[0]) / CELL).floor() as i32, ((y - self.origin[1]) / CELL).floor() as i32)
    }

    pub fn center(&self, c: (i32, i32)) -> (f32, f32) {
        (self.origin[0] + (c.0 as f32 + 0.5) * CELL, self.origin[1] + (c.1 as f32 + 0.5) * CELL)
    }

    fn index(&self, c: (i32, i32)) -> Option<usize> {
        (c.0 >= 0 && c.1 >= 0 && (c.0 as u32) < self.width && (c.1 as u32) < self.height)
            .then(|| (c.1 as u32 * self.width + c.0 as u32) as usize)
    }

    /// A cell's war3map.wpm flags.
    pub fn flags_at(&self, c: (i32, i32)) -> Option<u8> {
        self.index(c).map(|i| self.flags[i])
    }

    /// Sets or clears flag bits of a cell (SetTerrainPathable).
    pub fn set_flags(&mut self, c: (i32, i32), mask: u8, on: bool) {
        self.memo.get_mut().clear();
        self.floods.get_mut().clear();
        if let Some(i) = self.index(c) {
            if on {
                self.flags[i] |= mask;
            } else {
                self.flags[i] &= !mask;
            }
        }
    }

    pub fn walkable(&self, c: (i32, i32)) -> bool {
        self.passable(c, Move::Foot)
    }

    /// Whether a cell is open to a way of moving: walkable ground, water without land
    /// (the floatability flag), either, or anywhere flyable.
    pub fn passable(&self, c: (i32, i32), kind: Move) -> bool {
        self.index(c).is_some_and(|i| {
            let f = self.flags[i];
            let free = self.blocked[i] == 0;
            match kind {
                Move::Foot => f & NO_WALK == 0 && free,
                Move::Float => f & LAND == 0 && free,
                Move::Amphibious => (f & NO_WALK == 0 || f & LAND == 0) && free,
                Move::Fly => f & NO_FLY == 0,
            }
        })
    }

    pub fn buildable(&self, c: (i32, i32)) -> bool {
        self.index(c).is_some_and(|i| self.flags[i] & (NO_BUILD | NO_WALK) == 0 && self.blocked[i] == 0)
    }

    /// Whether a unit of `radius` fits centered in a cell.
    pub fn fits(&self, c: (i32, i32), radius: f32) -> bool {
        self.fits_kind(c, radius, Move::Foot)
    }

    pub fn fits_kind(&self, c: (i32, i32), radius: f32, kind: Move) -> bool {
        let r = ((radius / CELL).floor() as i32).max(0);
        for dy in -r..=r {
            for dx in -r..=r {
                if !self.passable((c.0 + dx, c.1 + dy), kind) {
                    return false;
                }
            }
        }
        true
    }

    /// Adds (or removes, with `delta` -1) a blocker covering the given cells.
    pub fn stamp(&mut self, cells: &[(i32, i32)], delta: i32) {
        self.memo.get_mut().clear();
        self.floods.get_mut().clear();
        for &c in cells {
            if let Some(i) = self.index(c) {
                self.blocked[i] = (self.blocked[i] as i32 + delta).max(0) as u16;
            }
        }
    }

    /// Straight-line walkability between two points, sampled every half cell.
    pub fn line_clear(&self, a: (f32, f32), b: (f32, f32), radius: f32) -> bool {
        self.line_clear_kind(a, b, radius, Move::Foot)
    }

    pub fn line_clear_kind(&self, a: (f32, f32), b: (f32, f32), radius: f32, kind: Move) -> bool {
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (d / (CELL * 0.5)).ceil().max(1.0) as i32;
        (0..=n).all(|i| {
            let t = i as f32 / n as f32;
            self.fits_kind(self.cell_of(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t), radius, kind)
        })
    }

    /// Finds a path from `from` toward `to`; when the goal is unreachable the path ends at
    /// the reachable cell closest to it.  Returns world-space waypoints (excluding the start).
    pub fn find_path(&self, from: (f32, f32), to: (f32, f32), radius: f32, max_nodes: usize) -> Vec<(f32, f32)> {
        self.find_path_kind(from, to, radius, max_nodes, Move::Foot)
    }

    pub fn find_path_kind(&self, from: (f32, f32), to: (f32, f32), radius: f32, max_nodes: usize, kind: Move) -> Vec<(f32, f32)> {
        if self.line_clear_kind(from, to, radius, kind) {
            return vec![to];
        }
        let mut start = self.cell_of(from.0, from.1);
        // A unit pushed onto ground it cannot stand on first steps off it to the
        // nearest cell it fits.
        let mut lead = None;
        if !self.fits_kind(start, radius, kind) {
            'ring: for r in 1..=16i32 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let c = (start.0 + dx, start.1 + dy);
                        if self.index(c).is_some() && self.fits_kind(c, radius, kind) {
                            start = c;
                            lead = Some(self.center(c));
                            break 'ring;
                        }
                    }
                }
            }
            if let Some(p) = lead {
                let mut rest = self.find_path_kind(p, to, radius, max_nodes, kind);
                rest.insert(0, p);
                return rest;
            }
        }
        // A goal the unit cannot stand on (a tree, a mine, a structure) is sought at the
        // nearest cell around it that it fits, which a search can reach.
        let mut goal = self.cell_of(to.0, to.1);
        if !self.fits_kind(goal, radius, kind) {
            let mut nearest: Option<((i32, i32), i32)> = None;
            for r in 1..=24i32 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let c = (goal.0 + dx, goal.1 + dy);
                        let d = dx * dx + dy * dy;
                        if nearest.is_none_or(|(_, b)| d < b) && self.index(c).is_some() && self.fits_kind(c, radius, kind) {
                            nearest = Some((c, d));
                        }
                    }
                }
                if nearest.is_some_and(|(_, b)| b <= (r * r)) {
                    break;
                }
            }
            if let Some((c, _)) = nearest {
                goal = c;
            }
        }
        let n = (self.width * self.height) as usize;
        let mut scratch = self.scratch.borrow_mut();
        let sc = &mut *scratch;
        if sc.stamp.len() != n {
            sc.g = vec![f32::INFINITY; n];
            sc.parent = vec![u32::MAX; n];
            sc.stamp = vec![0; n];
            sc.generation = 0;
        }
        sc.generation = sc.generation.wrapping_add(1);
        if sc.generation == 0 {
            sc.stamp.iter_mut().for_each(|s| *s = 0);
            sc.generation = 1;
        }
        let generation = sc.generation;
        let (g, parent, stamp) = (&mut sc.g, &mut sc.parent, &mut sc.stamp);
        let cost_of = |stamp: &[u32], g: &[f32], i: usize| if stamp[i] == generation { g[i] } else { f32::INFINITY };
        let Some(si) = self.index(start) else { return vec![] };
        let h = |c: (i32, i32)| {
            let (dx, dy) = ((c.0 - goal.0).abs() as f32, (c.1 - goal.1).abs() as f32);
            dx.max(dy) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dy)
        };
        #[derive(PartialEq, serde::Serialize, serde::Deserialize)]
        struct Node(f32, u32);
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        let mut open = BinaryHeap::new();
        g[si] = 0.0;
        parent[si] = u32::MAX;
        stamp[si] = generation;
        open.push(Node(h(start), si as u32));
        let mut best = (h(start), si);
        let mut expanded = 0;
        while let Some(Node(_, i)) = open.pop() {
            let i = i as usize;
            let c = ((i as u32 % self.width) as i32, (i as u32 / self.width) as i32);
            if c == goal {
                best = (0.0, i);
                break;
            }
            expanded += 1;
            if expanded > max_nodes {
                break;
            }
            let hc = h(c);
            if hc < best.0 {
                best = (hc, i);
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let nc = (c.0 + dx, c.1 + dy);
                let Some(ni) = self.index(nc) else { continue };
                if !self.fits_kind(nc, radius, kind) {
                    continue;
                }
                if dx != 0 && dy != 0 && (!self.fits_kind((c.0 + dx, c.1), radius, kind) || !self.fits_kind((c.0, c.1 + dy), radius, kind)) {
                    continue;
                }
                let cost = g[i] + if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
                if cost < cost_of(stamp, g, ni) {
                    g[ni] = cost;
                    parent[ni] = i as u32;
                    stamp[ni] = generation;
                    open.push(Node(cost + h(nc), ni as u32));
                }
            }
        }
        sc.stats.0 += 1;
        sc.stats.1 += expanded as u64;
        sc.stats.2 += (expanded > max_nodes) as u64;
        let parent = &sc.parent;
        // Rebuild and smooth.
        let mut cells = Vec::new();
        let mut i = best.1;
        while i != si && parent[i] != u32::MAX {
            cells.push(i);
            i = parent[i] as usize;
        }
        cells.reverse();
        let mut pts: Vec<(f32, f32)> = cells
            .iter()
            .map(|&i| self.center(((i as u32 % self.width) as i32, (i as u32 / self.width) as i32)))
            .collect();
        if best.1 == self.index(goal).unwrap_or(usize::MAX) {
            if let Some(last) = pts.last_mut() {
                *last = to;
            }
        }
        let mut out = Vec::new();
        let mut cur = from;
        let mut k = 0;
        while k < pts.len() {
            let mut far = k;
            for j in (k..pts.len()).rev() {
                if self.line_clear_kind(cur, pts[j], radius, kind) {
                    far = j;
                    break;
                }
            }
            out.push(pts[far]);
            cur = pts[far];
            k = far + 1;
        }
        out
    }
}
