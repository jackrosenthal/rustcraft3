//! The original's long-range path search (NIpse's CLrPathingSys, read from game.dll):
//! A* over pathing cells for a unit of a size class, with the engine's integer costs,
//! heuristic, and binary heap, so that it settles ties, gives up, and picks where to
//! go instead exactly as the original does.

use std::collections::HashMap;

/// Which cells refuse the searching unit: the map's static pathing for its kind and
/// the regions of others standing on them.
pub trait Cells {
    fn open(&self, x: i32, y: i32) -> bool;
}

/// A search's end: the path to the goal, or to the reached node nearest it, each from
/// the start's cell on; or nowhere (no node reached was nearer than the start).
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Found(Vec<(i32, i32)>),
    Nearest(Vec<(i32, i32)>),
    Stay,
}

/// A unit's size class from its collision radius in cells (`FUN_6f14ad50`: below 0.5,
/// below 1, below 1.5, or more).
pub fn class_of(radius_cells: f32) -> u8 {
    if radius_cells < 0.5 {
        0
    } else if radius_cells < 1.0 {
        1
    } else if radius_cells < 1.5 {
        2
    } else {
        3
    }
}

/// Each class's ring about a node (its offset and side, `FUN_6f148d00`) and, for the
/// neighbors SW, S, SE, W, E, NW, N, NE, the ring cells that must be open to step
/// there (`FUN_6f14b890` to `FUN_6f14bc10`).
const RINGS: [(i32, i32, [u32; 8]); 4] = [
    (1, 3, [0x83, 0x2, 0xe, 0x80, 0x8, 0xe0, 0x20, 0x38]),
    (2, 4, [0xc07, 0x6, 0x3e, 0xc00, 0x30, 0xf80, 0x180, 0x1f0]),
    (2, 5, [0xe00f, 0xe, 0xfe, 0xe000, 0xe0, 0xfe00, 0xe00, 0xfe0]),
    (3, 6, [0xf001f, 0x1e, 0x3fe, 0xf0000, 0x3c0, 0xff800, 0x7800, 0x7fc0]),
];
const STEPS: [(i32, i32, u32); 8] = [(-1, -1, 21), (0, -1, 15), (1, -1, 21), (-1, 0, 15), (1, 0, 15), (-1, 1, 21), (0, 1, 15), (1, 1, 21)];

/// The ring's refused cells as bits: its bottom row left to right, its right column
/// upward, its top row right to left, its left column downward.
fn ring(cells: &impl Cells, x: i32, y: i32, r: i32, n: i32, refused: &mut bool) -> u32 {
    let (x0, y0) = (x - r, y - r);
    let mut bits = 0;
    let mut bit = 1;
    let mut test = |cx: i32, cy: i32| {
        if !cells.open(cx, cy) {
            bits |= bit;
            *refused = true;
        }
        bit <<= 1;
    };
    for i in 0..n {
        test(x0 + i, y0);
    }
    for j in 1..n {
        test(x0 + n - 1, y0 + j);
    }
    for i in (0..n - 1).rev() {
        test(x0 + i, y0 + n - 1);
    }
    for j in (1..n - 1).rev() {
        test(x0, y0 + j);
    }
    bits
}

/// The distance estimate (`FUN_6f14a560`): offsets times 15, the larger, or with the
/// smaller over a quarter of it an integer approximation of their length.
fn estimate(c: (i32, i32), goal: (i32, i32)) -> u32 {
    let (mut a, mut b) = ((c.0 - goal.0).unsigned_abs() * 15, (c.1 - goal.1).unsigned_abs() * 15);
    if a < b {
        std::mem::swap(&mut a, &mut b);
    }
    if a >> 2 < b {
        let b = b + (b >> 1);
        a = (a >> 6).wrapping_add(b >> 2).wrapping_add(a).wrapping_add((b >> 7).wrapping_sub(a >> 4));
    }
    a
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    New,
    Open,
    Done,
}

struct Node {
    stamp: u32,
    state: State,
    g: u32,
    h: u32,
    parent: Option<(i32, i32)>,
}

/// The open list: a binary heap from 1 on keys g + h (`FUN_6f1483f0` pushes, sifting
/// up past parents whose keys are no smaller; `FUN_6f148240` pops, taking the right
/// child when its key is no greater and stopping at a child whose key is no smaller).
#[derive(Default)]
struct Heap(Vec<(u32, (i32, i32), u32)>);

impl Heap {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn push(&mut self, e: (u32, (i32, i32), u32)) {
        self.0.push(e);
        let mut i = self.0.len() - 1;
        while i > 1 {
            let p = self.0[i >> 1];
            if p.0 < e.0 {
                break;
            }
            self.0[i] = p;
            i >>= 1;
        }
        self.0[i] = e;
    }

    fn pop(&mut self) -> (u32, (i32, i32), u32) {
        let top = self.0[1];
        let last = self.0.pop().unwrap();
        let m = self.0.len() - 1;
        if m >= 1 {
            let mut i = 1;
            loop {
                let mut ch = i * 2;
                if ch > m {
                    break;
                }
                if ch < m && self.0[ch + 1].0 <= self.0[ch].0 {
                    ch += 1;
                }
                if last.0 <= self.0[ch].0 {
                    break;
                }
                self.0[i] = self.0[ch];
                i = ch;
                if ch > m >> 1 {
                    break;
                }
            }
            self.0[i] = last;
        }
        top
    }
}

/// Searches from the start's cell toward the goal's for a unit of `class`, popping at
/// most `limit` nodes (`FUN_6f14aa10`, `FUN_6f14a4c0`, `FUN_6f148100`).
pub fn search(cells: &impl Cells, start: (i32, i32), goal: (i32, i32), class: u8, limit: u32) -> Outcome {
    search_seeing(cells, start, goal, class, limit).0
}

/// `search`, and whether it found any cell refused on its way (`FUN_6f1489a0` sets the
/// searcher's +0xd0 as it refuses one; a walker whose search met none heads straight
/// for its way's end, `FUN_6f166e90`).
pub fn search_seeing(cells: &impl Cells, start: (i32, i32), goal: (i32, i32), class: u8, limit: u32) -> (Outcome, bool) {
    let (out, refused, _) = search_costing(cells, start, goal, class, limit);
    (out, refused)
}

/// `search_seeing`, and its cost: the passes of its loop (the searcher's +0x6c, counted
/// as each pass begins with nodes open, the one that finds its limit reached too; read:
/// a sheep's search stopped by its 32 cost 33).
pub fn search_costing(cells: &impl Cells, start: (i32, i32), goal: (i32, i32), class: u8, limit: u32) -> (Outcome, bool, u32) {
    search_until(cells, start, goal, class, limit, |c| c == goal)
}

/// Whether a node's ring for `class` (its cells' border, as `ring` walks it) holds a
/// cell of `region` (`FUN_6f148790`, the contact test of `FUN_6f166310`).
pub fn ring_touches(c: (i32, i32), class: u8, region: impl Fn(i32, i32) -> bool) -> bool {
    let (r, n, _) = RINGS[class.min(3) as usize];
    let (x0, y0) = (c.0 - r, c.1 - r);
    (0..n).any(|i| region(x0 + i, y0) || region(x0 + i, y0 + n - 1)) || (1..n - 1).any(|j| region(x0, y0 + j) || region(x0 + n - 1, y0 + j))
}

/// `search_costing` toward a structure: the goal its middle's cell, found at the first
/// node popped whose ring touches its footprint (read: a peon from cell 670, 266 to a
/// gold mine whose footprint is 676 to 683 by 260 to 267, its way's end at 675, 265; from
/// 675, 265 to a great hall's, 650 to 661 by 258 to 269, at 663, 265).
pub fn search_to_region(cells: &impl Cells, start: (i32, i32), goal: (i32, i32), class: u8, limit: u32, region: impl Fn(i32, i32) -> bool) -> (Outcome, bool, u32) {
    search_until(cells, start, goal, class, limit, |c| ring_touches(c, class, &region))
}

fn search_until(cells: &impl Cells, start: (i32, i32), goal: (i32, i32), class: u8, limit: u32, reached: impl Fn((i32, i32)) -> bool) -> (Outcome, bool, u32) {
    let mut refused = false;
    if reached(start) {
        return (Outcome::Found(vec![start]), refused, 1);
    }
    let (r, n, masks) = RINGS[class.min(3) as usize];
    let mut nodes: HashMap<(i32, i32), Node> = HashMap::new();
    let mut heap = Heap(vec![(0, (0, 0), 0)]);
    let dist2 = |c: (i32, i32)| ((c.0 - goal.0) as i64).pow(2) + ((c.1 - goal.1) as i64).pow(2);
    let mut best = (dist2(start), start);
    let push = |nodes: &mut HashMap<(i32, i32), Node>, heap: &mut Heap, c: (i32, i32)| {
        let nd = nodes.get_mut(&c).unwrap();
        nd.stamp += 1;
        nd.state = State::Open;
        heap.push((nd.g.wrapping_add(nd.h), c, nd.stamp));
    };
    nodes.insert(start, Node { stamp: 0, state: State::New, g: 0, h: estimate(start, goal), parent: None });
    push(&mut nodes, &mut heap, start);
    let mut pops = 0;
    let mut found = None;
    while heap.len() >= 2 && pops < limit {
        pops += 1;
        let (_, c, stamp) = heap.pop();
        let nd = nodes.get_mut(&c).unwrap();
        if nd.stamp != stamp {
            continue;
        }
        nd.stamp += 1;
        nd.state = State::New;
        if reached(c) {
            found = Some(c);
            break;
        }
        let g = nd.g;
        let bits = ring(cells, c.0, c.1, r, n, &mut refused);
        for (k, &(dx, dy, cost)) in STEPS.iter().enumerate() {
            if bits & masks[k] != 0 {
                continue;
            }
            let q = (c.0 + dx, c.1 + dy);
            let m = nodes.entry(q).or_insert(Node { stamp: 0, state: State::New, g: 0, h: 0, parent: None });
            let gn = g + cost;
            if m.state == State::New {
                let d = dist2(q);
                if d < best.0 {
                    best = (d, q);
                }
            } else {
                if m.g <= gn {
                    continue;
                }
                if m.state == State::Open {
                    m.stamp += 1;
                }
                m.state = State::New;
            }
            m.parent = Some(c);
            m.g = gn;
            m.h = estimate(q, goal);
            push(&mut nodes, &mut heap, q);
        }
        nodes.get_mut(&c).unwrap().state = State::Done;
    }
    let path = |end: (i32, i32)| {
        let mut out = vec![end];
        let mut c = end;
        while let Some(p) = nodes.get(&c).and_then(|n| n.parent) {
            out.push(p);
            c = p;
        }
        out.reverse();
        out
    };
    let out = if let Some(end) = found {
        Outcome::Found(path(end))
    } else if best.1 == start {
        Outcome::Stay
    } else {
        Outcome::Nearest(path(best.1))
    };
    let cost = pops + u32::from(found.is_none() && heap.len() >= 2);
    (out, refused, cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells from rows of text, top row first: '#' refused.
    struct Rows {
        x0: i32,
        y_top: i32,
        rows: Vec<&'static str>,
    }

    impl Cells for Rows {
        fn open(&self, x: i32, y: i32) -> bool {
            let (i, j) = (x - self.x0, self.y_top - y);
            if i < 0 || j < 0 {
                return false;
            }
            self.rows.get(j as usize).and_then(|r| r.as_bytes().get(i as usize)).is_some_and(|&c| c == b'.')
        }
    }

    /// Booty Bay's first wandering sheep, boxed in by its flock and the ground about
    /// it: the original's map about it as its search saw it (read under Wine, the
    /// sheep's own region excused), from cell 430, 46 toward 426, 40 with its 32-node
    /// limit.  It may step only south, to the cell the original's sheep then walks to
    /// (it stood at 430.49, 45.94 when its next search found nothing nearer).
    #[test]
    fn the_flock_boxes_the_sheep_in() {
        let cells = Rows {
            x0: 420,
            y_top: 55,
            rows: vec![
                "####........##########", // 55
                "####........##########", // 54
                "................######", // 53
                "................######", // 52
                "....................##", // 51
                ".........##.........##", // 50
                ".......####.........##", // 49
                ".....####.##........##", // 48
                ".....####.##....######", // 47
                ".......##..##...######", // 46
                "####...##..##...######", // 45
                "####.####.......######", // 44
                "####.##.##############", // 43
                "####....##############", // 42
                "........##############", // 41
                "........##############", // 40
                "######################", // 39
                "######################", // 38
            ],
        };
        assert_eq!(class_of(0.5), 1);
        assert_eq!(search(&cells, (430, 46), (426, 40), 1, 32), Outcome::Nearest(vec![(430, 46), (430, 45)]));
        assert_eq!(search(&cells, (430, 45), (426, 40), 1, 32), Outcome::Stay);
    }

    /// Booty Bay's north-east base: a peon at cell 670, 266 sent to the gold mine (its
    /// footprint 676 to 683 by 260 to 267), the four other peons' regions refusing their
    /// cells; the original's way (read from its path's points under Wine) runs east
    /// along row 266 and ends at 675, 265, whose ring touches the footprint.  Home from
    /// there to the great hall (650 to 661 by 258 to 269), searching past the peons, it
    /// runs west along row 265 to 663.
    #[test]
    fn a_peons_ways_end_at_the_footprints() {
        struct Base(bool);
        impl Cells for Base {
            fn open(&self, x: i32, y: i32) -> bool {
                let mine = (676..=683).contains(&x) && (260..=267).contains(&y);
                let hall = (650..=661).contains(&x) && (258..=269).contains(&y);
                let peons = [(671, 263), (667, 263), (670, 261), (667, 261)].iter().any(|&(px, py)| (px..=px + 1).contains(&x) && (py..=py + 1).contains(&y));
                !mine && !hall && !(self.0 && peons)
            }
        }
        let mine = |x: i32, y: i32| (676..=683).contains(&x) && (260..=267).contains(&y);
        let hall = |x: i32, y: i32| (650..=661).contains(&x) && (258..=269).contains(&y);
        let (out, refused, _) = search_to_region(&Base(true), (670, 266), (680, 264), 1, 700, mine);
        assert_eq!(out, Outcome::Found(vec![(670, 266), (671, 266), (672, 266), (673, 266), (674, 266), (675, 265)]));
        assert!(refused);
        let (out, _, _) = search_to_region(&Base(false), (675, 265), (656, 264), 1, 700, hall);
        assert_eq!(out, Outcome::Found((663..=675).rev().map(|x| (x, 265)).collect()));
    }

    #[test]
    fn the_estimate_is_the_engines() {
        assert_eq!(estimate((0, 0), (10, 0)), 150);
        // 150 and 60: 60 > 150 / 4, so 150 + 2 + 22 + 0 - 9.
        assert_eq!(estimate((0, 0), (10, 4)), 150 + 2 + 22 + 0 - 9);
    }
}
