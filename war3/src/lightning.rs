//! Lightning (Splats\LightningData.slk), drawn as the original does: points spaced
//! along the bolt, (length / AvgSegLen + 2) segments, the inner ones offset at random
//! up to NoiseScale times the length on each axis and eased a quarter of the way toward
//! new offsets every frame; a strip Width to each side across the screen, narrowing to
//! the end points; the texture running once along it and scrolling by TexCoordScale
//! a second, added to what is behind, over everything (no depth test).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use w3_formats::slk::{Table, Value};
use w3_render::particles::{Quad, filter};
use w3_render::texture::Texture;

use crate::scene::Assets;

struct Info {
    texture: Option<Arc<Texture>>,
    seg_len: f32,
    width: f32,
    noise: f32,
    tex_scale: f32,
}

struct Bolt {
    from: Vec3,
    to: Vec3,
    points: Vec<Vec3>,
}

#[derive(Default)]
pub struct LightningView {
    slk: Option<Table>,
    infos: HashMap<String, Option<Info>>,
    bolts: HashMap<u32, Bolt>,
    rng: u32,
    time: f32,
}

fn num(t: &Table, row: &str, col: &str) -> f32 {
    match t.get(row, col) {
        Some(Value::Num(n)) => *n as f32,
        Some(v) => v.as_str().trim().parse().unwrap_or(0.0),
        None => 0.0,
    }
}

impl LightningView {
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// Evenly spaced points from `from` to `to`, the inner ones offset at random.
    fn targets(&mut self, from: Vec3, to: Vec3, info: &Info) -> Vec<Vec3> {
        let len = from.distance(to);
        let segs = ((len / info.seg_len.max(1.0)) + 2.0) as usize;
        let amp = info.noise * len;
        (0..=segs)
            .map(|i| {
                let p = from.lerp(to, i as f32 / segs as f32);
                if i == 0 || i == segs { p } else { p + Vec3::new(self.random(), self.random(), self.random()) * amp }
            })
            .collect()
    }

    /// Moves the bolts and queues their strips; `view` is the camera's view matrix.
    /// `now` is the scene's clock in seconds.
    pub fn draw(&mut self, assets: &mut Assets, device: &wgpu::Device, queue: &wgpu::Queue, world: &w3_game::world::World, view: Mat4, now: f32, renderer: &mut w3_render::model::ModelRenderer) {
        if self.rng == 0 {
            self.rng = 0x2545_f491;
        }
        self.time = now;
        let table = self.slk.take().unwrap_or_else(|| assets.textures.chain.read_string("Splats\\LightningData.slk").map(|s| Table::parse(&s)).unwrap_or_default());
        let slk = &table;
        let mut live = Vec::new();
        for (&h, o) in world.handles.iter() {
            let w3_game::types::Obj::Lightning(l) = o else { continue };
            live.push((h, l.code.clone(), Vec3::from_array(l.from), Vec3::from_array(l.to), l.color));
        }
        self.bolts.retain(|h, _| live.iter().any(|l| l.0 == *h));
        let inv = view.inverse();
        for (h, code, from, to, tint) in live {
            if !self.infos.contains_key(&code) {
                let info = slk.rows.contains_key(&code).then(|| {
                    let s = |c: &str| slk.get(&code, c).map(|v| v.as_str()).unwrap_or_default();
                    Info {
                        texture: Some(assets.textures.get_or_white(device, queue, &format!("{}\\{}", s("Dir"), s("file")))),
                        seg_len: num(slk, &code, "AvgSegLen"),
                        width: num(slk, &code, "Width"),
                        noise: num(slk, &code, "NoiseScale"),
                        tex_scale: num(slk, &code, "TexCoordScale"),
                    }
                });
                self.infos.insert(code.clone(), info);
            }
            let Some(Some(info)) = self.infos.remove(&code) else { continue };
            // New ends start over; otherwise ease toward new offsets.
            let fresh = self.targets(from, to, &info);
            let bolt = self.bolts.entry(h).or_insert_with(|| Bolt { from, to, points: Vec::new() });
            if bolt.from != from || bolt.to != to || bolt.points.len() != fresh.len() {
                *bolt = Bolt { from, to, points: fresh };
            } else {
                for (p, n) in bolt.points.iter_mut().zip(&fresh) {
                    *p = *p * 0.75 + *n * 0.25;
                }
            }
            let pts: Vec<Vec3> = bolt.points.iter().map(|p| view.transform_point3(*p)).collect();
            let n = pts.len();
            // Across the screen from each segment, averaged at the joints.
            let normal = |a: Vec3, b: Vec3| {
                let d = b - a;
                let l = (d.x * d.x + d.y * d.y).sqrt();
                if l <= 0.001 { Vec3::ZERO } else { Vec3::new(-d.y / l, d.x / l, 0.0) }
            };
            let mut sides: Vec<(Vec3, Vec3)> = (0..n)
                .map(|i| {
                    let a = if i > 0 { normal(pts[i - 1], pts[i]) } else { normal(pts[0], pts[1]) };
                    let b = if i + 1 < n { normal(pts[i], pts[i + 1]) } else { a };
                    let off = (a + b) * 0.5 * info.width;
                    (inv.transform_point3(pts[i] + off), inv.transform_point3(pts[i] - off))
                })
                .collect();
            // The ends narrow to the points.
            sides[0] = (bolt.points[0], bolt.points[0]);
            sides[n - 1] = (bolt.points[n - 1], bolt.points[n - 1]);
            let scroll = if info.tex_scale == 0.0 { 0.0 } else { -(self.time % info.tex_scale) / info.tex_scale };
            // The type's color until SetLightningColor replaces it.
            let color = tint;
            let v = |i: usize| if i == 0 || i == n - 1 { (0.5, 0.5) } else { (0.0, 1.0) };
            let quads: Vec<Quad> = (0..n - 1)
                .map(|i| {
                    let (u0, u1) = (i as f32 / (n - 1) as f32 + scroll, (i + 1) as f32 / (n - 1) as f32 + scroll);
                    let (a, b) = (v(i), v(i + 1));
                    Quad {
                        corners: [sides[i].0, sides[i].1, sides[i + 1].0, sides[i + 1].1],
                        uv: [[u0, a.0], [u0, a.1], [u1, b.0], [u1, b.1]],
                        color,
                        texture_id: 0,
                        replaceable_id: 0,
                        filter: filter::ADDITIVE,
                        unshaded: true,
                        unfogged: true,
                    }
                })
                .collect();
            if let Some(t) = &info.texture {
                renderer.submit_quads(t, &quads, true, false);
            }
            self.infos.insert(code, Some(info));
        }
        self.slk = Some(table);
    }
}
