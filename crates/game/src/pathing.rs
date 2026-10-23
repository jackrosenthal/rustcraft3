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
}

impl PathingMap {
    pub fn parse(data: &[u8], origin: [f32; 2]) -> Option<PathingMap> {
        if data.len() < 16 || &data[0..4] != b"MP3W" {
            return None;
        }
        let w = u32::from_le_bytes(data[8..12].try_into().ok()?);
        let h = u32::from_le_bytes(data[12..16].try_into().ok()?);
        let flags = data.get(16..16 + (w * h) as usize)?.to_vec();
        Some(PathingMap { width: w, height: h, origin, flags, blocked: vec![0; (w * h) as usize] })
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
        let goal = self.cell_of(to.0, to.1);
        let n = (self.width * self.height) as usize;
        let mut g = vec![f32::INFINITY; n];
        let mut parent = vec![u32::MAX; n];
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
                if cost < g[ni] {
                    g[ni] = cost;
                    parent[ni] = i as u32;
                    open.push(Node(cost + h(nc), ni as u32));
                }
            }
        }
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
