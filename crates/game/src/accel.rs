//! The original's accelerator map (NIpse's CAcceleratorMap, read from game.dll and
//! measured against Booty Bay's): a pyramid of four levels over the pathing cells, each
//! cell holding, for each of four movement layers, whether the ground it stands for is
//! all open, all blocked, or mixed.  Long paths are planned over its open blocks.

/// A cell's state for a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Open,
    Blocked,
    Mixed,
}

/// The layers' masks over the pathing map's static byte (the map file's flags with
/// 0x01 added, and 0x10 where 0x02 is set) and over static regions' types: ground,
/// then three others (`DAT_6fce4570`: 0x06000006, 0x80000080, 0x40000040,
/// 0x04000004).
pub const LAYERS: [u8; 4] = [0x06, 0x80, 0x40, 0x04];

/// One level: its size and each cell's states by layer.
#[derive(Clone, Debug, Default)]
pub struct Level {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<[State; 4]>,
}

impl Level {
    pub fn get(&self, x: i64, y: i64) -> Option<&[State; 4]> {
        (x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height).then(|| &self.cells[y as usize * self.width + x as usize])
    }
}

/// The pyramid.
#[derive(Clone, Debug, Default)]
pub struct Pyramid {
    pub levels: Vec<Level>,
}

impl Pyramid {
    /// Builds the pyramid for a pathing map `width` by `height` cells, where
    /// `refused(x, y, layer)` says whether a cell inside the map refuses a layer (its
    /// static byte against the layer's mask, or a static region of the layer's types).
    /// Level 0's cell x, y stands for the pathing cells 2x to 2x + 1 by 2y to 2y + 1,
    /// built for cells 0 to width / 2 by 0 to height / 2 (those past the map refusing),
    /// the rest left open (`FUN_6f15cf80`, `FUN_6f15d0e0`); each level above halves the
    /// one below (`FUN_6f15d470`, `FUN_6f15d1c0`).  Level 0 is 9 cells wider and higher
    /// than half the map (measured on Booty Bay's, 393 by 201 for 768 by 384; other
    /// maps' sizes are not yet measured).
    pub fn build(width: usize, height: usize, refused: impl Fn(i64, i64, usize) -> bool) -> Pyramid {
        let (w0, h0) = (width / 2 + 9, height / 2 + 9);
        let mut l0 = Level { width: w0, height: h0, cells: vec![[State::Open; 4]; w0 * h0] };
        let blocked = |x: i64, y: i64, layer: usize| x < 0 || y < 0 || x >= width as i64 || y >= height as i64 || refused(x, y, layer);
        for ay in 0..=(height / 2) {
            for ax in 0..=(width / 2) {
                let (x, y) = (2 * ax as i64, 2 * ay as i64);
                for layer in 0..4 {
                    let n = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].iter().filter(|&&(px, py)| blocked(px, py, layer)).count();
                    l0.cells[ay * w0 + ax][layer] = match n {
                        0 => State::Open,
                        4 => State::Blocked,
                        _ => State::Mixed,
                    };
                }
            }
        }
        let mut levels = vec![l0];
        for _ in 1..4 {
            let below = levels.last().unwrap();
            let (w, h) = (below.width / 2, below.height / 2);
            let mut up = Level { width: w, height: h, cells: vec![[State::Open; 4]; w * h] };
            for y in 0..h {
                for x in 0..w {
                    for layer in 0..4 {
                        let kids = [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x + 1, 2 * y + 1), (2 * x, 2 * y + 1)].map(|(a, b)| below.get(a as i64, b as i64).map(|c| c[layer]));
                        up.cells[y * w + x][layer] = if kids.iter().any(|k| *k == Some(State::Mixed)) {
                            State::Mixed
                        } else {
                            match kids.iter().filter(|k| k.is_none_or(|s| s == State::Blocked)).count() {
                                0 => State::Open,
                                4 => State::Blocked,
                                _ => State::Mixed,
                            }
                        };
                    }
                }
            }
            levels.push(up);
        }
        Pyramid { levels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map 8 by 8 cells, open but for its left column refusing the ground layer:
    /// level 0 (2 by 2 cells each) is mixed down its left edge and open elsewhere within
    /// the map, its last column and row (past the map) blocked; level 1 mixed where
    /// it covers a mixed cell, open where all its four are open.
    #[test]
    fn levels_follow_the_cells_below() {
        let p = Pyramid::build(8, 8, |x, _, layer| layer == 0 && x == 0);
        let l0 = &p.levels[0];
        assert_eq!((l0.width, l0.height), (13, 13));
        assert_eq!(l0.get(0, 0).unwrap()[0], State::Mixed);
        assert_eq!(l0.get(1, 0).unwrap()[0], State::Open);
        assert_eq!(l0.get(4, 1).unwrap()[0], State::Blocked);
        assert_eq!(l0.get(5, 5).unwrap()[0], State::Open);
        assert_eq!(l0.get(0, 0).unwrap()[1], State::Open);
        let l1 = &p.levels[1];
        assert_eq!(l1.get(0, 0).unwrap()[0], State::Mixed);
        assert_eq!(l1.get(1, 0).unwrap()[0], State::Open);
        assert_eq!(l1.get(2, 0).unwrap()[0], State::Mixed);
    }
}
