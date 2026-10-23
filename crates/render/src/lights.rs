//! Lights that models give off (an MDX Light: torches, fires, glowing things) as
//! game.dll uses them.  Each becomes a Direct3D light (FUN_6f191040): an omni light a
//! point light, any other kind directional; its diffuse color times its intensity, its
//! ambient color times its ambient intensity.  Every light shares one attenuation, set
//! at startup (FUN_6f136cb0 with 0 and 0.0005: 1 / (1 + 0.0005 d²)) out to a range of
//! 10000; the models' own attenuation keys go unused.  Which lights light an object is
//! chosen per 1024-unit cell (FUN_6f120360): at the cell's point, each light scores its
//! intensity under that attenuation plus its ambient intensity (a directional light its
//! intensity alone, plus its ambient intensity), and the best few are set, up to 1, 4,
//! or 8 by the Lights option (FUN_6f1206b0).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::anim::sample;
use crate::model::ModelInstance;

/// An empty slot in a cell's list.
pub const NO_LIGHT: u32 = u32::MAX;
/// The day/night light in a cell's list (each shader uses its own: terrain or units).
pub const SUN: u32 = u32::MAX - 1;
/// Lights a cell's list holds.
pub const PER_CELL: usize = 8;
pub const MAX_LIGHTS: usize = 2048;
pub const MAX_CELLS: usize = 64 * 64;
/// The cells' size and the offset to their points (0x6fa8de38, 0x6fce3cc8).
pub const CELL: f32 = 1024.0;
pub const CELL_OFFSET: f32 = 512.0;
/// The attenuation's linear and quadratic terms, and the lights' range.
pub const ATTENUATION: (f32, f32) = (0.0, 0.0005);
pub const RANGE: f32 = 10000.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct GpuLight {
    /// A point light's position (w 1), or a directional one's direction toward the
    /// world (w 0).
    pub pos: [f32; 4],
    pub diffuse: [f32; 4],
    pub ambient: [f32; 4],
}

/// A light a model gives off now.
#[derive(Clone, Copy, Debug)]
pub struct ModelLight {
    pub point: bool,
    /// The position of a point light, the direction of a directional one.
    pub pos: Vec3,
    pub diffuse: Vec3,
    pub ambient: Vec3,
    pub intensity: f32,
    pub ambient_intensity: f32,
}

impl ModelLight {
    pub fn gpu(&self) -> GpuLight {
        GpuLight { pos: [self.pos.x, self.pos.y, self.pos.z, self.point as u32 as f32], diffuse: self.diffuse.extend(1.0).to_array(), ambient: self.ambient.extend(1.0).to_array() }
    }

    /// FUN_6f120360's score at a point.
    fn score(&self, at: Vec3) -> f32 {
        let s = if self.point {
            let d = self.pos.distance(at);
            self.intensity / (ATTENUATION.1 * d * d + ATTENUATION.0 * d + 1.0)
        } else {
            self.intensity
        };
        s + self.ambient_intensity
    }
}

fn bgr(c: [f32; 3]) -> Vec3 {
    Vec3::new(c[2], c[1], c[0])
}

/// The lights an instance gives off at its current frame (after it was submitted, so
/// its skeleton is posed), those whose visibility track shows them.
pub fn instance_lights(inst: &ModelInstance, out: &mut Vec<ModelLight>) {
    let mdx = &inst.model.mdx;
    if mdx.lights.is_empty() || !inst.visible {
        return;
    }
    let time = inst.time();
    let g = &mdx.global_sequences;
    for l in &mdx.lights {
        if sample(l.tracks.f32(b"KLAV"), &time, g, 1.0) <= 0.0 {
            continue;
        }
        let color = bgr(sample(l.tracks.vec3(b"KLAC"), &time, g, l.color));
        let intensity = sample(l.tracks.f32(b"KLAI"), &time, g, l.intensity);
        let amb = bgr(sample(l.tracks.vec3(b"KLBC"), &time, g, l.ambient_color));
        let ambient_intensity = sample(l.tracks.f32(b"KLBI"), &time, g, l.ambient_intensity);
        let id = l.node.object_id as usize;
        let node = inst.transform * inst.skeleton.world.get(id).copied().unwrap_or(Mat4::IDENTITY);
        let point = l.kind == 0;
        let pos = if point {
            let pivot = mdx.pivots.get(id).map_or(Vec3::ZERO, |p| Vec3::from_array(*p));
            node.transform_point3(pivot)
        } else {
            let (_, rot, _) = node.to_scale_rotation_translation();
            rot * Vec3::NEG_Z
        };
        out.push(ModelLight { point, pos, diffuse: color * intensity, ambient: amb * ambient_intensity, intensity, ambient_intensity });
    }
}

/// The cell holding a world coordinate.
pub fn cell_of(v: f32) -> i32 {
    ((v + CELL_OFFSET) / CELL).floor() as i32
}

/// A cell's lights: the best `n` at its point (the day/night light, scoring
/// `sun_score`, among them).
pub fn choose(at: Vec3, sun_score: f32, lights: &[ModelLight], n: usize) -> [u32; PER_CELL] {
    let mut scored: Vec<(f32, u32)> = lights.iter().enumerate().filter(|(_, l)| !l.point || l.pos.distance(at) < RANGE).map(|(i, l)| (l.score(at), i as u32)).collect();
    scored.push((sun_score, SUN));
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out = [NO_LIGHT; PER_CELL];
    for (k, (_, i)) in scored.into_iter().take(n.min(PER_CELL)).enumerate() {
        out[k] = i;
    }
    out
}

/// The lights and cell lists a frame's shaders read.
pub struct LightBuffers {
    pub lights: wgpu::Buffer,
    pub cells: wgpu::Buffer,
}

impl LightBuffers {
    pub fn new(device: &wgpu::Device) -> LightBuffers {
        let buf = |label, size| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        LightBuffers {
            lights: buf("model lights", (MAX_LIGHTS * std::mem::size_of::<GpuLight>()) as u64),
            cells: buf("light cells", (MAX_CELLS * PER_CELL * 4) as u64),
        }
    }

    pub fn write(&self, queue: &wgpu::Queue, lights: &[GpuLight], cells: &[[u32; PER_CELL]]) {
        let lights = &lights[..lights.len().min(MAX_LIGHTS)];
        let cells = &cells[..cells.len().min(MAX_CELLS)];
        if !lights.is_empty() {
            queue.write_buffer(&self.lights, 0, bytemuck::cast_slice(lights));
        }
        if !cells.is_empty() {
            queue.write_buffer(&self.cells, 0, bytemuck::cast_slice(cells));
        }
    }

    pub fn layout_entries(first: u32, visibility: wgpu::ShaderStages) -> [wgpu::BindGroupLayoutEntry; 2] {
        let e = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        [e(first), e(first + 1)]
    }
}
