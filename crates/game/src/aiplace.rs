//! Where a computer player puts a structure (FUN_6f9ba3f0 and the CAIBuildGrid around
//! it, FUN_6f9b6cd0 to FUN_6f9b8120).
//!
//! A grid of 101 by 101 cells of 32 is laid around the town's center, the midpoint of
//! its hall and gold mine (FUN_6f9bfe80).  A cell is free (5) when it lies in the
//! center's pathing region and can be built on, unbuildable (-100) when it cannot, and
//! apart (-98) in another region.  Every structure then marks its footprint taken (-99)
//! and lowers the cells within its buffRadius of it to its buffType's class: a town hall
//! 0, a resource 1, a factory 2, and a buffer or anything else 3 (anything else within 6
//! cells).  Placing a Scout or Guard Tower (hwtw) or Ziggurat (uzig), buffers keep 15
//! cells around them once the player has more than one, else 5 (FUN_6f9bcc70).
//!
//! From the center the cells are visited breadth first, eight neighbors at a time.
//! The first free cell where the structure can be placed (and, on the first pass, the
//! builder can walk to) is taken; failing every free cell, the first such cell found
//! in each class is kept and the highest class's is taken.  When the first pass finds
//! nothing and some cell was out of the builder's reach, a second pass drops the walk.

use crate::objects::{Id, id_from_str};
use crate::world::World;

const SIZE: usize = 101;
const CELL: f32 = 32.0;
const HALF: f32 = 1600.0;
const FREE: i8 = 5;
const UNBUILDABLE: i8 = -100;
const TAKEN: i8 = -99;
const APART: i8 = -98;

#[derive(Clone)]
struct Grid {
    ox: f32,
    oy: f32,
    class: Vec<i8>,
}

impl Grid {
    fn at(&self, i: i32, j: i32) -> Option<usize> {
        ((0..SIZE as i32).contains(&i) && (0..SIZE as i32).contains(&j)).then(|| j as usize * SIZE + i as usize)
    }

    fn cell_of(&self, x: f32, y: f32) -> (i32, i32) {
        (((x - self.ox) / CELL) as i32, ((y - self.oy) / CELL) as i32)
    }

    /// A footprint cell (FUN_6f9bc9e0): taken itself, and the square `r` around it at
    /// most `class`.
    fn stamp(&mut self, i: i32, j: i32, r: i32, class: i8) {
        if let Some(k) = self.at(i, j) {
            if self.class[k] > TAKEN {
                self.class[k] = TAKEN;
            }
        }
        for dj in -r..=r {
            for di in -r..=r {
                if let Some(k) = self.at(i + di, j + dj) {
                    if self.class[k] > class {
                        self.class[k] = class;
                    }
                }
            }
        }
    }
}

impl World {
    /// The center of a computer player's town: between its hall and its gold mine.
    pub(crate) fn town_center(&self, p: u8, town: i32) -> (f32, f32) {
        let (hx, hy) = self.town_pos_pub(p, town);
        match self.mine_near_pub(hx, hy).and_then(|m| self.unit(m)) {
            Some(m) => ((hx + m.x) * 0.5, (hy + m.y) * 0.5),
            None => (hx, hy),
        }
    }

    /// The connected walkable region of each pathing cell, worked out once.
    fn region_at(&self, x: f32, y: f32) -> Option<u32> {
        let map = self.pathing.as_ref()?;
        let regions = self.regions.get_or_init(|| {
            let (w, h) = (map.width as i32, map.height as i32);
            let mut out = vec![u32::MAX; (w * h) as usize];
            let mut next = 0;
            for start in 0..(w * h) {
                let c = (start % w, start / w);
                if out[start as usize] != u32::MAX || map.flags_at(c).is_none_or(|f| f & crate::pathing::NO_WALK != 0) {
                    continue;
                }
                let mut stack = vec![c];
                out[start as usize] = next;
                while let Some((cx, cy)) = stack.pop() {
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let n = (cx + dx, cy + dy);
                        if n.0 < 0 || n.1 < 0 || n.0 >= w || n.1 >= h {
                            continue;
                        }
                        let k = (n.1 * w + n.0) as usize;
                        if out[k] == u32::MAX && map.flags_at(n).is_some_and(|f| f & crate::pathing::NO_WALK == 0) {
                            out[k] = next;
                            stack.push(n);
                        }
                    }
                }
                next += 1;
            }
            out
        });
        let c = map.cell_of(x, y);
        let k = (c.0 >= 0 && c.1 >= 0 && (c.0 as u32) < map.width && (c.1 as u32) < map.height).then(|| (c.1 as u32 * map.width + c.0 as u32) as usize)?;
        Some(regions[k]).filter(|&r| r != u32::MAX)
    }

    fn statically_buildable(&self, x: f32, y: f32) -> bool {
        self.pathing.as_ref().is_none_or(|m| m.flags_at(m.cell_of(x, y)).is_some_and(|f| f & (crate::pathing::NO_BUILD | crate::pathing::NO_WALK) == 0))
    }

    fn ai_grid(&mut self, p: u8, id: Id, cx: f32, cy: f32) -> Grid {
        let (ox, oy) = (cx - HALF, cy - HALF);
        let home = self.region_at(cx, cy);
        let mut class = vec![APART; SIZE * SIZE];
        for j in 0..SIZE {
            for i in 0..SIZE {
                let (x, y) = (ox + i as f32 * CELL, oy + j as f32 * CELL);
                if self.region_at(x, y) == home {
                    class[j * SIZE + i] = if self.statically_buildable(x, y) { FREE } else { UNBUILDABLE };
                }
            }
        }
        let mut grid = Grid { ox, oy, class };
        let tower = id == id_from_str("hwtw") || id == id_from_str("uzig");
        let many = tower && self.unit_count(p, id, false, None) > 1;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead)).collect();
        for h in units {
            let Some(u) = self.unit(h) else { continue };
            let (x, y, facing, type_id) = (u.x, u.y, u.facing, u.type_id);
            let Some(o) = self.objects.units.get(type_id) else { continue };
            let tex = o.str("pathTex");
            let radius = o.real("buffRadius") as i32;
            let (r, c) = match o.str("buffType").trim().to_ascii_lowercase().as_str() {
                "townhall" => (radius, 0),
                "resource" => (radius, 1),
                "factory" => (radius, 2),
                "buffer" => (if many { 15 } else if tower { 5 } else { radius }, 3),
                _ => (6, 3),
            };
            let cells = self.pathing_cells(&tex, x, y, facing);
            let Some(map) = &self.pathing else { continue };
            for cell in cells {
                let (wx, wy) = map.center(cell);
                let (i, j) = grid.cell_of(wx, wy);
                grid.stamp(i, j, r, c);
            }
        }
        grid
    }

    /// One pass of the search (FUN_6f9b8120); with `walk`, cells the builder cannot
    /// walk to are passed over, and the second value tells whether any was.
    fn ai_grid_search(&mut self, grid: &mut Grid, id: Id, builder: Option<(f32, f32, f32)>, walk: bool) -> (Option<(f32, f32)>, bool) {
        let mut visit = vec![5u8; SIZE * SIZE];
        let mut queue = std::collections::VecDeque::from([(50i32, 50i32)]);
        let mut fallback: [Option<(f32, f32)>; 4] = [None; 4];
        let mut unreachable = false;
        while let Some((ci, cj)) = queue.pop_front() {
            let (x, y) = self.snap_placement(grid.ox + ci as f32 * CELL, grid.oy + cj as f32 * CELL);
            let (si, sj) = grid.cell_of(x, y);
            if let Some(k) = grid.at(si, sj) {
                let (a, b) = (grid.class[k], visit[k]);
                let wanted = b != 7 && (a == FREE || (0..4).contains(&a) && fallback[a as usize].is_none());
                if wanted {
                    let reached = !walk || builder.is_none_or(|(bx, by, br)| self.walk_reaches((bx, by), (x, y), br));
                    if !reached {
                        unreachable = true;
                    } else if self.can_place(id, x, y) {
                        if a == FREE {
                            grid.class[k] = 4;
                            return (Some((x, y)), unreachable);
                        }
                        fallback[a as usize] = Some((x, y));
                    }
                }
            }
            for (di, dj) in [(-1, 0), (-1, -1), (-1, 1), (1, 0), (1, -1), (1, 1), (0, -1), (0, 1)] {
                if let Some(k) = grid.at(ci + di, cj + dj) {
                    if visit[k] == 5 {
                        visit[k] = 6;
                        queue.push_back((ci + di, cj + dj));
                    }
                }
            }
            if let Some(k) = grid.at(si, sj) {
                visit[k] = 7;
            }
        }
        (fallback.iter().rev().flatten().next().copied(), unreachable)
    }

    /// Where a computer player's builder puts a structure of type `id` in `town`
    /// (FUN_6f9ba3f0).
    pub(crate) fn ai_build_site(&mut self, p: u8, id: Id, town: i32, builder: u32) -> Option<(f32, f32)> {
        let (cx, cy) = self.town_center(p, town);
        let mut grid = self.ai_grid(p, id, cx, cy);
        let mut copy = grid.clone();
        let who = self.unit(builder).map(|u| (u.x, u.y, u.collision));
        let (site, unreachable) = self.ai_grid_search(&mut grid, id, who, true);
        if site.is_some() || !unreachable {
            return site;
        }
        self.ai_grid_search(&mut copy, id, who, false).0
    }

    /// Whether a unit of `radius` can walk from `from` to `to`.
    fn walk_reaches(&self, from: (f32, f32), to: (f32, f32), radius: f32) -> bool {
        let Some(map) = &self.pathing else { return true };
        let end = map.find_path(from, to, radius, 200_000).last().copied().unwrap_or(from);
        (end.0 - to.0).hypot(end.1 - to.1) <= radius + 2.0 * CELL
    }
}
