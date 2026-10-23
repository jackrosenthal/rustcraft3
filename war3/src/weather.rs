//! Weather effects (TerrainArt\Weather.slk): each effect's rect is split into 512-unit
//! cells, each with a particle emitter `height` above the cell's ground, tilted by
//! `angx` and `angy`, emitting over the whole cell.  The emitter is the same kind as a
//! model's particle emitter, so a one-emitter model is made from the effect's row.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use w3_formats::mdx::{Model, Node, ParticleEmitter2, Texture, Tracks};
use w3_formats::slk::{Table, Value};
use w3_render::model::{ModelGpu, ModelInstance};

use crate::scene::Assets;

/// Cell size of a weather area.
const CELL: f32 = 512.0;

/// Cells farther than this from the camera target are not drawn.
const DRAW_RANGE: f32 = 4096.0;

#[derive(Default)]
pub struct WeatherView {
    slk: Option<Table>,
    models: HashMap<[u8; 4], Option<Arc<ModelGpu>>>,
    /// Emitters by weather handle and cell.
    cells: HashMap<(u32, i32, i32), ModelInstance>,
}

fn num(t: &Table, row: &str, col: &str) -> f32 {
    match t.get(row, col) {
        Some(Value::Num(n)) => *n as f32,
        Some(v) => v.as_str().trim().parse().unwrap_or(0.0),
        None => 0.0,
    }
}

/// The emitter for one weather row.
fn emitter_model(t: &Table, row: &str) -> Option<(Model, f32, f32, f32)> {
    let s = |c: &str| t.get(row, c).map(|v| v.as_str()).unwrap_or_default();
    if s("texFile").is_empty() {
        return None;
    }
    let n = |c: &str| num(t, row, c);
    let color = |seg: &str| [n(&format!("red{seg}")) / 255.0, n(&format!("green{seg}")) / 255.0, n(&format!("blue{seg}")) / 255.0];
    // alphaMode: 0 blend, 1 additive, 2 additive alpha, 3 modulate, 4 alpha key.
    let filter_mode = match n("alphaMode") as i32 {
        0 => 0,
        1 | 2 => 1,
        3 => 2,
        _ => 4,
    };
    let (head, tail) = (n("head") != 0.0, n("tail") != 0.0);
    // useFog off: the particles ignore fog of war.
    let flags = if n("useFog") == 0.0 { 0x40000 } else { 0 };
    let e = ParticleEmitter2 {
        node: Node { name: row.to_string(), object_id: 0, parent: None, flags, tracks: Tracks::default() },
        speed: n("veloc"),
        variation: n("var"),
        latitude: n("lati"),
        gravity: n("accel"),
        lifespan: n("lifespan"),
        emission_rate: n("emrate"),
        width: CELL,
        length: CELL,
        filter_mode,
        rows: n("texr").max(1.0) as u32,
        columns: n("texc").max(1.0) as u32,
        head_or_tail: match (head, tail) {
            (true, true) => 2,
            (false, true) => 1,
            _ => 0,
        },
        tail_length: n("taillen"),
        time: n("midTime"),
        segment_color: [color("Start"), color("Mid"), color("End")],
        segment_alpha: [n("alphaStart") as u8, n("alphaMid") as u8, n("alphaEnd") as u8],
        segment_scaling: [n("scaleStart"), n("scaleMid"), n("scaleEnd")],
        head_intervals: [[n("hUVStart") as u32, n("hUVMid") as u32, 1], [n("hUVMid") as u32, n("hUVEnd") as u32, 1]],
        tail_intervals: [[n("tUVStart") as u32, n("tUVMid") as u32, 1], [n("tUVMid") as u32, n("tUVEnd") as u32, 1]],
        texture_id: 0,
        squirt: false,
        priority_plane: 0,
        replaceable_id: 0,
        tracks: Tracks::default(),
    };
    let model = Model {
        name: row.to_string(),
        textures: vec![Texture { replaceable_id: 0, path: format!("{}\\{}.blp", s("texDir"), s("texFile")), flags: 0 }],
        pivots: vec![[0.0; 3]],
        particle_emitters2: vec![e],
        ..Default::default()
    };
    Some((model, n("height"), n("angx"), n("angy")))
}

impl WeatherView {
    /// Keeps an emitter for each cell of each enabled weather effect near `target`.
    pub fn sync(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets, world: &w3_game::world::World, ground: impl Fn(f32, f32) -> f32, target: Vec3) {
        let slk = self.slk.get_or_insert_with(|| assets.textures.chain.read_string("TerrainArt\\Weather.slk").map(|s| Table::parse(&s)).unwrap_or_default());
        let mut keep = std::collections::HashSet::new();
        for (&h, o) in world.handles.iter() {
            let w3_game::types::Obj::Weather(w) = o else { continue };
            let row = String::from_utf8_lossy(&w.id).to_string();
            let entry = self.models.entry(w.id).or_insert_with(|| {
                emitter_model(slk, &row).map(|(m, ..)| Arc::new(ModelGpu::new(device, queue, &mut assets.textures, &row, m)))
            });
            let Some(model) = entry.clone() else { continue };
            let (height, angx, angy) = (num(slk, &row, "height"), num(slk, &row, "angx"), num(slk, &row, "angy"));
            let r = w.rect;
            let (nx, ny) = (((r.max_x - r.min_x) / CELL) as i32 + 1, ((r.max_y - r.min_y) / CELL) as i32 + 1);
            for cy in 0..ny {
                for cx in 0..nx {
                    let (x0, y0) = (r.min_x + cx as f32 * CELL, r.min_y + cy as f32 * CELL);
                    let (x, y) = (x0 + CELL * 0.5, y0 + CELL * 0.5);
                    let key = (h, cx, cy);
                    let near = (x - target.x).abs() < DRAW_RANGE && (y - target.y).abs() < DRAW_RANGE;
                    if !near && !self.cells.contains_key(&key) {
                        continue;
                    }
                    let inst = self.cells.entry(key).or_insert_with(|| {
                        let mut i = ModelInstance::new(model.clone());
                        let rot = Quat::from_rotation_x(angx.to_radians()) * Quat::from_rotation_y(angy.to_radians());
                        i.transform = Mat4::from_rotation_translation(rot, Vec3::new(x, y, ground(x, y) + height));
                        i
                    });
                    // Out of range or disabled cells stop emitting and go once empty.
                    inst.particles.emitting = w.enabled && near;
                    if inst.particles.emitting || !inst.particles.is_empty() {
                        keep.insert(key);
                    }
                }
            }
        }
        self.cells.retain(|k, _| keep.contains(k));
    }

    /// The effect's ambient sound label, if it has one.
    pub fn ambient(&self, id: [u8; 4]) -> Option<String> {
        let row = String::from_utf8_lossy(&id).to_string();
        let v = self.slk.as_ref()?.get(&row, "AmbientSound")?.as_str();
        (!v.is_empty() && v != "-").then_some(v)
    }

    pub fn advance(&mut self, ms: f32) {
        for i in self.cells.values_mut() {
            i.advance(ms);
        }
    }

    pub fn instances(&mut self) -> impl Iterator<Item = &mut ModelInstance> {
        self.cells.values_mut()
    }
}
