//! Terrain deformations (TerrainDeformCrater, Ripple, Wave, and Random): each moves the
//! ground's corners within its area by a formula of the corner's place and the time,
//! re-evaluated every update and summed with the others into the ground's height
//! (CTerrainDeformation, game.dll 0x6f75d150).  A permanent one leaves its last shape
//! in the ground when it ends.

use crate::collections::HashMap;
use crate::world::World;

/// The formula a deformation follows, with the original's parameter arrays.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    /// 'crat' (FUN_6f78df00): x, y, radius, depth, in, out, base.
    Crater,
    /// 'ripp' (FUN_6f78e500): x, y, radius, depth, space waves times pi, time waves
    /// times pi, starting radius share, limit to negative.
    Ripple,
    /// 'mvwv' (FUN_6f78e050): start x, y, end x, y, radius, depth, trail share, travel
    /// share, and the unit normal to the path.
    Wave,
    /// 'rand' (FUN_6f78e350): x, y, radius, least and most change.
    Random,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Deformation {
    pub handle: u32,
    pub kind: Kind,
    pub params: Vec<f32>,
    /// Milliseconds a cycle lasts; repeating without end when it was given negative.
    pub duration: i64,
    pub endless: bool,
    /// Milliseconds between evaluations (0: every update).
    pub interval: i64,
    pub count: i32,
    pub permanent: bool,
    /// Only corners within `radius` of the center move.
    pub circular: bool,
    pub center: (f32, f32),
    pub radius: f32,
    /// The corners it covers, inclusive: x0, y0, x1, y1.
    pub area: (u32, u32, u32, u32),
    pub start: i64,
    pub cycle: i32,
    pub last: i64,
    /// TerrainDeformStop: when it began fading and over how many milliseconds.
    pub stop: Option<(i64, i64)>,
    /// Each covered corner's change, row by row.
    pub deltas: Vec<f32>,
}

impl Deformation {
    fn delta_at(&self, x: u32, y: u32) -> f32 {
        let (x0, y0, x1, y1) = self.area;
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return 0.0;
        }
        self.deltas[((y - y0) * (x1 - x0 + 1) + (x - x0)) as usize]
    }
}

/// The deformation's height at a point, given the time through its cycle `t` and its
/// progress over all its cycles.
fn evaluate(kind: Kind, p: &[f32], (x, y): (f32, f32), t: f32, progress: f32, rng: &mut u32) -> f32 {
    use std::f32::consts::PI;
    let unit = |v: f32| if v >= 1.0 { 1.0 } else if v <= 0.0 { 0.0 } else { v };
    match kind {
        Kind::Crater => {
            let d = unit((x - p[0]).hypot(y - p[1]) / p[2]);
            let envelope = if t < p[4] {
                t / p[4]
            } else if p[5] < t {
                1.0 - (t - p[5]) / (1.0 - p[5])
            } else {
                1.0
            };
            -((d * 0.5 * PI).cos() * (envelope * p[3] + p[6]))
        }
        Kind::Ripple => {
            let dist = (x - p[0]).hypot(y - p[1]);
            if (p[2] - p[6] * p[2]) * progress + p[6] * p[2] < dist {
                return 0.0;
            }
            let d = unit(dist / p[2]);
            let h = -((t * p[5] + d * p[4]).sin() * p[3]) * (1.0 - progress);
            if p[7] != 0.0 && h >= 0.0 { 0.0 } else { h }
        }
        Kind::Wave => {
            let (sx, sy, ex, ey) = (p[0], p[1], p[2], p[3]);
            let (dx, dy) = (x - sx, y - sy);
            let across = dy * p[9] + dx * p[8];
            let (mut fx, mut fy) = (dx - across * p[8] + sx, dy - across * p[9] + sy);
            // The lit stretch of the path runs from its trailing to its leading point,
            // measured along x as the original does.
            let lead = if t <= p[7] { (t / p[7]) * (ex - sx) + sx } else { ex };
            let trail = if p[6] <= t { ((t - p[6]) / (1.0 - p[6])) * (ex - sx) + sx } else { sx };
            let slope = (ey - sy) / (ex - sx);
            if (lead < fx && sx < ex) || (fx < lead && ex < sx) {
                fy = (lead - sx) * slope + sy;
                fx = lead;
            } else if (fx < trail && sx < ex) || (trail < fx && ex < sx) {
                fy = (trail - sx) * slope + sy;
                fx = trail;
            }
            let dist = (x - fx).hypot(y - fy);
            if dist > p[4] {
                return 0.0;
            }
            let mut h = ((unit(dist / p[4]) * PI).cos() + 1.0) * p[5] * 0.5;
            if t > 0.9 {
                h *= 1.0 - (t - 0.9) / 0.1;
            }
            -h
        }
        Kind::Random => {
            let dist = (x - p[0]).hypot(y - p[1]);
            if p[2] < dist {
                return 0.0;
            }
            let d = unit(dist / p[2]);
            *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let frac = f32::from_bits((*rng >> 9) | 0x3f80_0000) - 1.0;
            let mut h = (p[4] - p[3]) * (1.0 - frac) + p[3];
            if d > 0.8 {
                h *= 1.0 - (d - 0.8) / 0.2;
            }
            if t > 0.9 {
                h *= 1.0 - (t - 0.9) / 0.1;
            }
            h
        }
    }
}

impl World {
    /// Starts a deformation over the box (x0, y0)-(x1, y1); returns its handle
    /// (FUN_6f730400).
    #[allow(clippy::too_many_arguments)]
    pub fn start_deformation(&mut self, kind: Kind, params: Vec<f32>, bounds: [f32; 4], circular: bool, duration: i64, interval: i64, count: i32, permanent: bool) -> u32 {
        let h = self.handles.insert(crate::types::Obj::Opaque("terraindeformation".to_string()));
        let [min_x, min_y, max_x, max_y] = bounds;
        let center = ((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
        let radius = (max_y - min_y).max(max_x - min_x) * 0.5;
        let area = match self.terrain_heights.as_ref() {
            Some(t) => {
                // The corners strictly past the low edges and up to the high ones.
                let first = |v: f32, o: f32, n: u32| ((1 - (-(v - o) / 128.0) as i32).max(0) as u32).min(n - 1);
                let last = |v: f32, o: f32, n: u32| (((v - o) / 128.0) as i32).clamp(0, n as i32 - 1) as u32;
                (first(min_x, t.offset[0], t.width), first(min_y, t.offset[1], t.height), last(max_x, t.offset[0], t.width), last(max_y, t.offset[1], t.height))
            }
            None => (1, 1, 0, 0),
        };
        let cells = if area.2 >= area.0 && area.3 >= area.1 { ((area.2 - area.0 + 1) * (area.3 - area.1 + 1)) as usize } else { 0 };
        let now = self.deform_clock;
        self.deformations.push(Deformation {
            handle: h,
            kind,
            params,
            duration: duration.abs(),
            endless: duration < 0,
            interval,
            count: count.max(1),
            permanent,
            circular,
            center,
            radius,
            area,
            start: now,
            cycle: 0,
            last: now,
            stop: None,
            deltas: vec![0.0; cells],
        });
        h
    }

    /// TerrainDeformStop: fades the deformation out over `fade` milliseconds, or ends it
    /// at once (FUN_6f75b650).
    pub fn stop_deformation(&mut self, h: u32, fade: i64) {
        let now = self.deform_clock;
        if let Some(d) = self.deformations.iter_mut().find(|d| d.handle == h) {
            d.stop = Some((now, fade));
        }
    }

    /// Advances the deformations by `dt` seconds and gathers their heights
    /// (FUN_6f75d150, FUN_6f75ced0).
    pub fn update_deformations(&mut self, dt: f32) {
        self.deform_clock += (dt * 1000.0).round() as i64;
        if self.deformations.is_empty() {
            if self.deform_offsets != self.deform_baked {
                self.gather_deformations();
            }
            return;
        }
        let now = self.deform_clock;
        let Some(t) = self.terrain_heights.as_ref() else { return };
        let (offset, width) = (t.offset, t.width);
        let mut finished: Vec<usize> = Vec::new();
        let mut rng = self.deform_rng;
        for (i, d) in self.deformations.iter_mut().enumerate() {
            let elapsed = now - d.start;
            if d.interval > 0 && d.last > d.start && now - d.last < d.interval {
                continue;
            }
            let mut time = 1.0f32;
            if d.duration != 0 {
                time = elapsed as f32 / d.duration as f32;
                if time > 1.0 {
                    while time > 1.0 {
                        time -= 1.0;
                        d.cycle += 1;
                    }
                    d.start = now - (d.duration as f32 * time) as i64;
                }
            }
            let done = !d.endless && d.count <= d.cycle;
            if done {
                time = 1.0;
            }
            // Stopping: the heights fade to nothing, and it ends when they have.
            let mut fade = 1.0;
            if let Some((began, over)) = d.stop {
                let f = if over != 0 { ((now - began) as f32 / over as f32).min(1.0) } else { 1.0 };
                if f == 1.0 {
                    finished.push(i);
                    d.permanent = false;
                    continue;
                }
                fade = 1.0 - f;
            }
            let progress = (d.cycle as f32 + time) / d.count as f32;
            let (x0, y0, x1, y1) = d.area;
            if x1 >= x0 && y1 >= y0 {
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        let (px, py) = (offset[0] + cx as f32 * 128.0, offset[1] + cy as f32 * 128.0);
                        if d.circular && (px - d.center.0).powi(2) + (py - d.center.1).powi(2) > d.radius * d.radius {
                            continue;
                        }
                        let v = evaluate(d.kind, &d.params, (px, py), time, progress, &mut rng) * fade;
                        d.deltas[((cy - y0) * (x1 - x0 + 1) + (cx - x0)) as usize] = v;
                    }
                }
            }
            d.last = now;
            if done {
                finished.push(i);
            }
        }
        self.deform_rng = rng;
        // A permanent deformation leaves its last heights in the ground, which holds
        // them in quarter units, rounding down (FUN_6f7409e0).
        for &i in finished.iter().rev() {
            let d = self.deformations.remove(i);
            if d.permanent {
                let (x0, y0, x1, y1) = d.area;
                if x1 >= x0 && y1 >= y0 {
                    for cy in y0..=y1 {
                        for cx in x0..=x1 {
                            *self.deform_baked.entry(cy * width + cx).or_insert(0.0) += (d.delta_at(cx, cy) * 4.0).floor() / 4.0;
                        }
                    }
                }
            }
        }
        self.gather_deformations();
    }

    /// Sums the running deformations and the ones left in the ground into each corner's
    /// change in height.
    fn gather_deformations(&mut self) {
        let Some(width) = self.terrain_heights.as_ref().map(|t| t.width) else { return };
        let mut sum: HashMap<u32, f32> = self.deform_baked.clone();
        for d in &self.deformations {
            let (x0, y0, x1, y1) = d.area;
            if x1 < x0 || y1 < y0 {
                continue;
            }
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    let v = d.delta_at(cx, cy);
                    if v != 0.0 {
                        *sum.entry(cy * width + cx).or_insert(0.0) += v;
                    }
                }
            }
        }
        if sum != self.deform_offsets {
            self.deform_offsets = sum;
            self.deform_generation += 1;
        }
    }

    /// The change in a corner's height the deformations make.
    pub fn deform_offset(&self, index: u32) -> f32 {
        self.deform_offsets.get(&index).copied().unwrap_or(0.0)
    }
}
