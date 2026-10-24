//! Terrain painting (SetTerrainType, GetTerrainType, GetTerrainVariance), as the
//! terrain does it (FUN_6f7396d0): the corners within `area` - 1 of the one nearest the
//! point, in a circle or a square, take the tileset (added to the map's when it lacks
//! it, FUN_6f740650) and the variation, or one picked by the world editor's
//! [TerrainCellRarity] weights for -1 (FUN_6f71e8e0, FUN_6f74ccc0).

use crate::world::World;

/// The tileset that stands for no ground ('NOTH'), which this does not paint.
const NOTHING: [u8; 4] = *b"NOTH";

impl World {
    /// The corner nearest a point (FUN_6f7408e0), if on the map.
    pub fn terrain_corner(&self, x: f32, y: f32) -> Option<usize> {
        let t = self.terrain_heights.as_ref()?;
        let cx = ((x - t.offset[0]) / 128.0 + 0.5).floor();
        let cy = ((y - t.offset[1]) / 128.0 + 0.5).floor();
        if cx < 0.0 || cy < 0.0 || cx as u32 >= t.width || cy as u32 >= t.height {
            return None;
        }
        let i = cy as usize * t.width as usize + cx as usize;
        (i < self.ground.len()).then_some(i)
    }

    pub fn set_terrain_type(&mut self, x: f32, y: f32, tileset: [u8; 4], variation: i32, area: i32, square: bool) {
        if tileset == [0; 4] || tileset == NOTHING {
            return;
        }
        let Some(center) = self.terrain_corner(x, y) else { return };
        let Some((w, h)) = self.terrain_heights.as_ref().map(|t| (t.width as i32, t.height as i32)) else { return };
        let index = match self.ground_tilesets.iter().position(|t| *t == tileset) {
            Some(i) => i,
            // Up to sixteen tilesets fit a corner's four bits.
            None if self.ground_tilesets.len() < 16 && self.objects_terrain_has(tileset) => {
                self.ground_tilesets.push(tileset);
                self.ground_tilesets.len() - 1
            }
            None => return,
        };
        let weights = if variation == -1 { self.cell_rarity() } else { Vec::new() };
        let (cx, cy) = (center as i32 % w, center as i32 / w);
        let r = area - 1;
        for vy in (cy - r).max(0)..=(cy + r).min(h - 1) {
            for vx in (cx - r).max(0)..=(cx + r).min(w - 1) {
                let (dx, dy) = ((vx - cx) as f32, (vy - cy) as f32);
                let mut d = (dx * dx + dy * dy).sqrt();
                if area > 1 {
                    d /= r as f32;
                }
                if !square && d > 1.0 {
                    continue;
                }
                let v = if variation == -1 { self.pick_cell(&weights) } else { variation as u8 & 0x1f };
                self.ground[(vy * w + vx) as usize] = (index as u8, v);
            }
        }
        self.ground_serial = self.ground_serial.wrapping_add(1);
    }

    /// Whether TerrainArt\Terrain.slk has a tileset.
    fn objects_terrain_has(&self, tileset: [u8; 4]) -> bool {
        let key = String::from_utf8_lossy(&tileset).into_owned();
        self.chain.read_string("TerrainArt\\Terrain.slk").is_ok_and(|s| s.lines().any(|l| l.contains(&format!("\"{key}\""))))
    }

    /// The eighteen cells' weights, from UI\WorldEditData.txt [TerrainCellRarity].
    fn cell_rarity(&self) -> Vec<i32> {
        let p = self.chain.read_string("UI\\WorldEditData.txt").map(|s| w3_formats::profile::Profile::parse(&s)).ok();
        (0..18).map(|i| p.as_ref().and_then(|p| p.get_str("TerrainCellRarity", &format!("{i:02}"))).and_then(|v| v.trim().parse().ok()).unwrap_or(0)).collect()
    }

    /// A cell by weight: a number from one to the weights' sum, and the first cell the
    /// running sum reaches.
    fn pick_cell(&mut self, weights: &[i32]) -> u8 {
        let sum: i32 = weights.iter().sum();
        if sum <= 0 {
            return 0;
        }
        let n = self.rng.int(1, sum);
        let mut run = 0;
        for (i, &wt) in weights.iter().enumerate() {
            run += wt;
            if n <= run {
                return i as u8;
            }
        }
        0
    }
}
