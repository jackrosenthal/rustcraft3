//! MDX model rendering with GPU skinning.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3, Vec4};
use w3_formats::mdx::{self, Model, filter, shading};

use crate::anim::{self, Skeleton, Time};
use crate::texture::{Texture, TextureCache};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bones: [u16; 4],
    pub bone_count: u32,
}

pub struct GeosetGpu {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub material: usize,
    pub geoset: usize,
}

pub struct ModelGpu {
    pub path: String,
    pub mdx: Model,
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub geosets: Vec<GeosetGpu>,
    /// Loaded textures by texture index; `None` for replaceable textures.
    pub textures: Vec<Option<Arc<Texture>>>,
    /// Geoset id -> geoset animation index.
    pub geoset_anims: HashMap<u32, usize>,
}

impl ModelGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut TextureCache,
        path: &str,
        mdx: Model,
    ) -> ModelGpu {
        use wgpu::util::DeviceExt;
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u16> = Vec::new();
        let mut geosets = Vec::new();
        for (gi, g) in mdx.geosets.iter().enumerate() {
            // Expand matrix groups into per-vertex bone lists.
            let mut groups: Vec<Vec<u16>> = Vec::new();
            let mut k = 0usize;
            for &size in &g.group_sizes {
                let list = g
                    .matrix_indices
                    .get(k..k + size as usize)
                    .unwrap_or(&[])
                    .iter()
                    .map(|&m| m as u16)
                    .collect();
                k += size as usize;
                groups.push(list);
            }
            let base = verts.len() as i32;
            let uvs = g.uvs.first();
            for (vi, p) in g.vertices.iter().enumerate() {
                let grp = g.vertex_groups.get(vi).and_then(|&gidx| groups.get(gidx as usize));
                let mut bones = [0u16; 4];
                let mut count = 0;
                if let Some(grp) = grp {
                    for (j, &b) in grp.iter().take(4).enumerate() {
                        bones[j] = b;
                        count = j as u32 + 1;
                    }
                }
                verts.push(Vertex {
                    pos: *p,
                    normal: g.normals.get(vi).copied().unwrap_or([0.0, 0.0, 1.0]),
                    uv: uvs.and_then(|u| u.get(vi)).copied().unwrap_or([0.0; 2]),
                    bones,
                    bone_count: count,
                });
            }
            let first = idx.len() as u32;
            let tri_len = g.faces.len() / 3 * 3;
            idx.extend_from_slice(&g.faces[..tri_len]);
            if idx.len() % 2 == 1 {
                // Keep each geoset's first index 4-byte aligned is not required, but keep the
                // buffer size a multiple of 4 bytes for the copy.
            }
            geosets.push(GeosetGpu {
                first_index: first,
                index_count: tri_len as u32,
                base_vertex: base,
                material: g.material_id as usize,
                geoset: gi,
            });
        }
        if idx.len() % 2 == 1 {
            idx.push(0);
        }
        if verts.is_empty() {
            verts.push(Vertex::zeroed());
        }
        if idx.is_empty() {
            idx.extend([0, 0]);
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(path),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(path),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let tex = mdx
            .textures
            .iter()
            .map(|t| {
                if t.replaceable_id != 0 {
                    None
                } else {
                    Some(textures.get_or_white(device, queue, &t.path))
                }
            })
            .collect();
        let geoset_anims = mdx
            .geoset_anims
            .iter()
            .enumerate()
            .map(|(i, ga)| (ga.geoset_id, i))
            .collect();
        ModelGpu {
            path: path.to_string(),
            mdx,
            vertices,
            indices,
            geosets,
            textures: tex,
            geoset_anims,
        }
    }
}

/// One placed, animated copy of a model.
pub struct ModelInstance {
    pub model: Arc<ModelGpu>,
    pub transform: Mat4,
    pub sequence: Option<usize>,
    /// Milliseconds into the current sequence.
    pub frame: f32,
    /// Milliseconds since creation, driving global sequences.
    pub global_time: f32,
    pub team: u8,
    pub tint: Vec4,
    pub visible: bool,
    /// A bone turned to face a world point (SetUnitLookAt).
    pub look_at: Option<(String, Vec3)>,
    /// Textures substituted for replaceable texture ids (trees, cliffs, ...).
    pub replaceable: HashMap<u32, Arc<Texture>>,
    pub skeleton: Skeleton,
    /// Set once a non-looping sequence reaches its end.
    pub finished: bool,
    /// Set whenever a looping sequence wraps around.
    pub looped: bool,
    /// Sound event codes (the `xxxx` of `SNDxxxxx` event objects) whose keys were passed
    /// since the caller last drained them.
    pub sound_events: Vec<[u8; 4]>,
    pub particles: crate::particles::Particles,
    /// Milliseconds a change of sequence cross-fades over (the model's BlendTime, or a
    /// unit's blend), the pose left behind, and how far the fade has gone.
    pub blend_ms: f32,
    blend_from: Option<Time>,
    blend_elapsed: f32,
    /// The light grid cell the instance stands in, whose lights light it.
    pub light_cell: u32,
}

impl ModelInstance {
    pub fn new(model: Arc<ModelGpu>) -> Self {
        let skeleton = Skeleton::new(&model.mdx);
        let blend_ms = model.mdx.blend_time as f32;
        ModelInstance {
            blend_ms,
            blend_from: None,
            blend_elapsed: 0.0,
            model,
            transform: Mat4::IDENTITY,
            sequence: None,
            frame: 0.0,
            global_time: 0.0,
            team: 0,
            tint: Vec4::ONE,
            visible: true,
            look_at: None,
            replaceable: HashMap::new(),
            skeleton,
            finished: false,
            sound_events: Vec::new(),
            particles: Default::default(),
            looped: false,
            light_cell: crate::lights::NO_LIGHT,
        }
    }

    pub fn set_sequence(&mut self, seq: Option<usize>) {
        if self.blend_ms > 0.0 && self.sequence.is_some() && seq != self.sequence {
            self.blend_from = Some(self.time());
            self.blend_elapsed = 0.0;
        }
        self.sequence = seq;
        self.frame = 0.0;
        self.finished = false;
    }

    pub fn play(&mut self, name: &str) {
        let s = anim::find_sequence(&self.model.mdx, name);
        self.set_sequence(s);
    }

    pub fn advance(&mut self, dt_ms: f32) {
        self.global_time += dt_ms;
        if self.blend_from.is_some() {
            self.blend_elapsed += dt_ms;
            if self.blend_elapsed >= self.blend_ms {
                self.blend_from = None;
            }
        }
        if let Some(s) = self.sequence.and_then(|s| self.model.mdx.sequences.get(s)) {
            let len = (s.end - s.start) as f32;
            // Event keys in (before, after], in sequence time.
            // Events nobody drains (no audio) are dropped.
            if self.sound_events.len() > 32 {
                self.sound_events.clear();
            }
            // A sequence that just started includes its first frame.
            let before = if self.frame <= 0.0 { -1.0 } else { self.frame };
            let after = self.frame + dt_ms;
            for e in &self.model.mdx.event_objects {
                let name = e.node.name.as_bytes();
                if e.global_seq.is_some() || name.len() < 8 || !name.starts_with(b"SND") {
                    continue;
                }
                let hit = e.frames.iter().any(|&f| {
                    if f < s.start || f > s.end {
                        return false;
                    }
                    let t = (f - s.start) as f32;
                    (t > before && t <= after) || (!s.non_looping && len > 0.0 && after > len && t <= after - len)
                });
                if hit {
                    self.sound_events.push([name[4], name[5], name[6], name[7]]);
                }
            }
            self.frame += dt_ms;
            if self.frame > len {
                if s.non_looping {
                    self.frame = len;
                    self.finished = true;
                } else if len > 0.0 {
                    self.frame %= len;
                    self.looped = true;
                } else {
                    self.frame = 0.0;
                }
            }
        }
    }

    pub fn time(&self) -> Time {
        match self.sequence.and_then(|s| self.model.mdx.sequences.get(s)) {
            Some(s) => Time {
                frame: s.start + self.frame as u32,
                seq_start: s.start,
                seq_end: s.end,
                global: self.global_time as u32,
            },
            None => Time { global: self.global_time as u32, ..Default::default() },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniform {
    color: [f32; 4],
    uv_offset: [f32; 2],
    bone_offset: u32,
    flags: u32,
    /// The instance's cell in the light grid (`lights::NO_LIGHT` outside it).
    light_cell: u32,
    _pad0: [u32; 3],
    /// The texture animation's rotation and scaling of the coordinates (a 2x2 matrix,
    /// columns first), before uv_offset.
    uv_mat: [f32; 4],
    /// Where the instance stands, for its fog of war.
    origin: [f32; 4],
    _pad: [u32; 44],
}

const DRAW_ALIGN: u64 = 256;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    flags: u32,
}

const FLAG_UNSHADED: u32 = 1;
const FLAG_ALPHA_TEST: u32 = 2;
/// Fog takes additive layers toward black and modulating layers toward white, so they
/// fade out rather than brighten or darken the scene.
const FLAG_FOG_BLACK: u32 = 4;
const FLAG_FOG_WHITE: u32 = 8;
/// Additive layers add their color, faded by the layer's alpha (not the texture's).
const FLAG_ADDITIVE: u32 = 16;
/// A layer the distance fog leaves alone (shading Unfogged, 0x20).
const FLAG_UNFOGGED: u32 = 32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 4],
    pub light_color: [f32; 4],
    pub ambient: [f32; 4],
    pub fog_color: [f32; 4],
    /// x: start, y: end, z: enabled
    pub fog: [f32; 4],
    pub camera_pos: [f32; 4],
    /// Terrain uses its own day/night light.
    pub terrain_light_color: [f32; 4],
    pub terrain_ambient: [f32; 4],
    /// Fog of war grid origin and size; a zero size disables it.
    pub fow_extent: [f32; 4],
    /// UI\MiscData.txt [FogOfWar] FoggedTerrain and FoggedObject as rgb and alpha.
    pub fow_terrain: [f32; 4],
    pub fow_object: [f32; 4],
    /// Masked areas: BlackMaskedTerrain/Object, or the Dark ones where masked areas
    /// are partially visible.
    pub fow_masked_terrain: [f32; 4],
    pub fow_masked_object: [f32; 4],
    /// SetWaterBaseColor, multiplying the water's colors.
    pub water_tint: [f32; 4],
    /// The model lights' cell grid (see `lights`): its first cell's indices and its
    /// width and height in cells; a zero width leaves only the day/night light.
    pub light_grid: [f32; 4],
    /// The world's boundary: [FogOfWar] BoundaryTerrain, FoggedBoundaryTerrain,
    /// BoundaryObject, and FoggedBoundaryObject.
    pub fow_boundary_terrain: [f32; 4],
    pub fow_fogged_boundary_terrain: [f32; 4],
    pub fow_boundary_object: [f32; 4],
    pub fow_fogged_boundary_object: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct PipelineKey {
    filter: u32,
    two_sided: bool,
    depth_test: bool,
    depth_write: bool,
}

struct Draw {
    model: Arc<ModelGpu>,
    geoset: usize,
    pipeline: PipelineKey,
    texture: Arc<Texture>,
    wrap: (bool, bool),
    uniform: DrawUniform,
    depth: f32,
    priority: i32,
    order: usize,
}

pub struct ModelRenderer {
    /// Leaves the particles for the caller to draw with `render_particles` (the world
    /// draws them over its water).
    pub defer_particles: bool,
    shader: wgpu::ShaderModule,
    globals_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    color_format: wgpu::TextureFormat,
    pub globals_buf: wgpu::Buffer,
    bones_buf: wgpu::Buffer,
    bones_cap: usize,
    draws_buf: wgpu::Buffer,
    draws_cap: usize,
    globals_bg: wgpu::BindGroup,
    draws_bg: wgpu::BindGroup,
    samplers: [wgpu::Sampler; 4],
    texture_bgs: HashMap<(usize, bool, bool), wgpu::BindGroup>,
    team_colors: Vec<Arc<Texture>>,
    team_glows: Vec<Arc<Texture>>,
    fog: Arc<crate::fog::FogTexture>,
    /// The model lights and the cells' choices of them, shared with the terrain.
    pub lights: Arc<crate::lights::LightBuffers>,
    particle_shader: wgpu::ShaderModule,
    particle_layout: wgpu::PipelineLayout,
    /// By filter and whether depth is tested.
    particle_pipelines: HashMap<(u32, bool), wgpu::RenderPipeline>,
    particle_buf: Option<wgpu::Buffer>,
    /// This frame's particle quads: vertices and runs sharing a texture and filter.
    particle_verts: Vec<ParticleVertex>,
    /// Texture, filter, u wrap, depth test, and vertices.
    particle_runs: Vec<(Arc<Texture>, u32, bool, bool, std::ops::Range<u32>)>,
    // Per-frame state.
    bones: Vec<[[f32; 4]; 4]>,
    draws: Vec<Draw>,
}

impl ModelRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut TextureCache,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("model.wgsl"));
        let [fow0, fow1] = crate::fog::layout_entries(2);
        let [light0, light1] = crate::lights::LightBuffers::layout_entries(4, wgpu::ShaderStages::FRAGMENT);
        let lights = Arc::new(crate::lights::LightBuffers::new(device));
        let fog = Arc::new(crate::fog::FogTexture::new(device, queue, 1, 1));
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                fow0,
                fow1,
                light0,
                light1,
            ],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model draw"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniform>() as u64),
                },
                count: None,
            }],
        });
        let texture_layout = texture_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&globals_layout), Some(&draw_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bones_cap = 1024;
        let bones_buf = bones_buffer(device, bones_cap);
        let draws_cap = 256;
        let draws_buf = draws_buffer(device, draws_cap);
        let globals_bg = globals_bind_group(device, &globals_layout, &globals_buf, &bones_buf, &fog, &lights);
        let draws_bg = draws_bind_group(device, &draw_layout, &draws_buf);
        let samplers = [
            (false, false),
            (true, false),
            (false, true),
            (true, true),
        ]
        .map(|(u, v)| make_sampler(device, u, v));
        let team = |kind: &str, textures: &mut TextureCache| {
            (0..16)
                .map(|i| {
                    textures.get_or_white(
                        device,
                        queue,
                        &format!("ReplaceableTextures\\TeamColor\\{kind}{i:02}.blp"),
                    )
                })
                .collect::<Vec<_>>()
        };
        let team_colors = team("TeamColor", textures);
        let team_glows = {
            (0..16)
                .map(|i| {
                    textures.get_or_white(
                        device,
                        queue,
                        &format!("ReplaceableTextures\\TeamGlow\\TeamGlow{i:02}.blp"),
                    )
                })
                .collect()
        };
        let particle_shader = device.create_shader_module(wgpu::include_wgsl!("particle.wgsl"));
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        ModelRenderer {
            fog,
            lights,
            particle_shader,
            particle_layout,
            particle_pipelines: HashMap::new(),
            particle_buf: None,
            particle_verts: Vec::new(),
            particle_runs: Vec::new(),
            defer_particles: false,
            shader,
            globals_layout,
            draw_layout,
            texture_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            color_format,
            globals_buf,
            bones_buf,
            bones_cap,
            draws_buf,
            draws_cap,
            globals_bg,
            draws_bg,
            samplers,
            texture_bgs: HashMap::new(),
            team_colors,
            team_glows,
            bones: Vec::new(),
            draws: Vec::new(),
        }
    }

    fn push_quad(&mut self, texture: &Arc<Texture>, q: &crate::particles::Quad, wrap_u: bool, depth: bool) {
        let mut flags = 0;
        if q.unfogged {
            flags |= 1;
        }
        if q.filter == crate::particles::filter::ALPHA_KEY {
            flags |= 2;
        }
        if q.filter == crate::particles::filter::ADDITIVE {
            flags |= 4;
        }
        if q.filter == crate::particles::filter::ADD_COLOR {
            flags |= 4 | 8;
        }
        let first = self.particle_verts.len() as u32;
        for k in [0usize, 2, 1, 1, 2, 3] {
            self.particle_verts.push(ParticleVertex { pos: q.corners[k].to_array(), uv: q.uv[k], color: q.color, flags });
        }
        let end = first + 6;
        match self.particle_runs.last_mut() {
            Some((t, f, w, d, r)) if Arc::ptr_eq(t, texture) && *f == q.filter && *w == wrap_u && *d == depth && r.end == first => r.end = end,
            _ => self.particle_runs.push((texture.clone(), q.filter, wrap_u, depth, first..end)),
        }
    }

    /// Queues textured quads drawn with the particles (lightning); `wrap_u` repeats the
    /// texture along u, and `depth` hides them behind nearer things.
    pub fn submit_quads(&mut self, texture: &Arc<Texture>, quads: &[crate::particles::Quad], wrap_u: bool, depth: bool) {
        for q in quads {
            self.push_quad(texture, q, wrap_u, depth);
        }
    }

    /// Animates an instance and queues its geosets for drawing.
    pub fn submit(&mut self, inst: &mut ModelInstance, camera_pos: Vec3, camera_rot: Quat) {
        if !inst.visible {
            return;
        }
        let model = inst.model.clone();
        let mdx = &model.mdx;
        let time = inst.time();
        let inv_rot = inst.transform.to_scale_rotation_translation().1.inverse() * camera_rot;
        inst.skeleton.look = inst.look_at.as_ref().and_then(|(bone, target)| {
            let i = mdx.nodes().iter().position(|n| n.is_some_and(|n| n.name.eq_ignore_ascii_case(bone)))?;
            Some((i, inst.transform.inverse().transform_point3(*target)))
        });
        let blend = inst.blend_from.as_ref().map(|f| (f, (inst.blend_elapsed / inst.blend_ms.max(1.0)).clamp(0.0, 1.0)));
        inst.skeleton.update(mdx, &time, inv_rot, blend);
        let bone_offset = self.bones.len() as u32;
        if inst.skeleton.world.is_empty() {
            self.bones.push(inst.transform.to_cols_array_2d());
        }
        for m in &inst.skeleton.world {
            self.bones.push((inst.transform * *m).to_cols_array_2d());
        }
        // Particles: emit and move, then queue quads.
        if !mdx.particle_emitters2.is_empty() {
            let now = inst.global_time;
            inst.particles.update(mdx, &inst.skeleton.world, inst.transform, &time, now);
            let mut quads = Vec::new();
            inst.particles.quads(mdx, inst.transform, camera_rot, &mut quads);
            for q in quads {
                let texture = match q.replaceable_id {
                    0 => model.textures.get(q.texture_id as usize).cloned().flatten(),
                    1 => Some(self.team_colors[inst.team as usize % 16].clone()),
                    2 => Some(self.team_glows[inst.team as usize % 16].clone()),
                    id => inst.replaceable.get(&id).cloned(),
                };
                let Some(texture) = texture else { continue };
                self.push_quad(&texture, &q, false, true);
            }
        }
        let center = inst.transform.transform_point3(Vec3::ZERO);
        let depth = center.distance(camera_pos);
        let globals = &mdx.global_sequences;
        for g in &model.geosets {
            let (mut ga_alpha, mut ga_color) = (1.0, [1.0f32; 3]);
            if let Some(&ga) = model.geoset_anims.get(&(g.geoset as u32)) {
                let (a, c) = anim::geoset_anim_state(&mdx.geoset_anims[ga], &time, globals);
                // At most 1 (measured: Malfurion's ground glow, keyed at 68.53, shows as a
                // soft round glow in the original, where ours, so multiplied, lit its
                // texture's faint edges into a square).
                ga_alpha = a.min(1.0);
                if mdx.geoset_anims[ga].flags & 2 != 0 {
                    // A static color is red, green, blue (the main menu's tower glow, a
                    // Blue_Glow2 quad under 0, 0.07, 1, shows blue in the original); the
                    // KGAC track's keys are blue, green, red (the score screen's bars then
                    // show the player colors in order).
                    let track = mdx.geoset_anims[ga].tracks.vec3(b"KGAC").is_some_and(|t| !t.keys.is_empty());
                    ga_color = if track { [c[2], c[1], c[0]] } else { c };
                }
            }
            if ga_alpha <= 0.0 {
                continue;
            }
            let Some(mat) = mdx.materials.get(g.material) else { continue };
            for (li, layer) in mat.layers.iter().enumerate() {
                let (alpha, tex_id) = anim::layer_state(layer, &time, globals);
                let alpha = alpha * ga_alpha * inst.tint.w;
                if alpha <= 0.0 {
                    continue;
                }
                let Some(t) = mdx.textures.get(tex_id as usize) else { continue };
                let texture = match t.replaceable_id {
                    0 => model.textures[tex_id as usize].clone().unwrap_or_else(|| self.team_colors[0].clone()),
                    1 => self.team_colors[inst.team as usize % 16].clone(),
                    2 => self.team_glows[inst.team as usize % 16].clone(),
                    id => match inst.replaceable.get(&id) {
                        Some(t) => t.clone(),
                        None => continue,
                    },
                };
                let mut flags = 0;
                if layer.shading & shading::UNSHADED != 0 {
                    flags |= FLAG_UNSHADED;
                }
                if layer.shading & shading::UNFOGGED != 0 {
                    flags |= FLAG_UNFOGGED;
                }
                if layer.filter_mode == filter::TRANSPARENT {
                    flags |= FLAG_ALPHA_TEST;
                }
                if layer.filter_mode == filter::ADDITIVE || layer.filter_mode == filter::ADD_ALPHA {
                    flags |= FLAG_FOG_BLACK;
                }
                if layer.filter_mode == filter::ADDITIVE {
                    flags |= FLAG_ADDITIVE;
                }
                if layer.filter_mode == filter::MODULATE || layer.filter_mode == filter::MODULATE_2X {
                    flags |= FLAG_FOG_WHITE;
                }
                // An opaque or alpha-tested layer that is fading draws blended.
                let fading = alpha < 1.0 && layer.filter_mode <= filter::TRANSPARENT;
                let blended = layer.filter_mode >= filter::BLEND || fading;
                let key = PipelineKey {
                    filter: if fading { filter::BLEND } else { layer.filter_mode.min(6) },
                    two_sided: layer.shading & shading::TWO_SIDED != 0,
                    depth_test: layer.shading & shading::NO_DEPTH_TEST == 0,
                    depth_write: layer.shading & shading::NO_DEPTH_SET == 0 && !blended,
                };
                let tint = inst.tint;
                // Texture animation: the coordinates are scaled (KTAS) and turned (KTAR)
                // about their origin, then moved (KTAT), as the cooldown indicator's
                // sweep turns its texture.
                let ta = layer.texture_anim_id.and_then(|i| mdx.texture_anims.get(i as usize));
                let uv_offset = ta.map(|ta| anim::sample(ta.vec3(b"KTAT"), &time, globals, [0.0; 3])).map_or([0.0, 0.0], |t| [t[0], t[1]]);
                let uv_mat = ta.map_or([1.0, 0.0, 0.0, 1.0], |ta| {
                    let q = anim::sample(ta.quat(b"KTAR"), &time, globals, [0.0, 0.0, 0.0, 1.0]);
                    let s = anim::sample(ta.vec3(b"KTAS"), &time, globals, [1.0; 3]);
                    let m = Mat4::from_quat(glam::Quat::from_array(q).normalize()) * Mat4::from_scale(Vec3::from_array(s));
                    // The coordinates turn the other way from the model's own rotations:
                    // the transpose (measured: the cooldown indicator 28% through shows
                    // its first quarter clear, top right, as the original's does).
                    [m.x_axis.x, m.y_axis.x, m.x_axis.y, m.y_axis.y]
                });
                self.draws.push(Draw {
                    model: model.clone(),
                    geoset: g.geoset,
                    pipeline: key,
                    texture,
                    wrap: (t.flags & 1 != 0, t.flags & 2 != 0),
                    uniform: DrawUniform {
                        color: [ga_color[0] * tint.x, ga_color[1] * tint.y, ga_color[2] * tint.z, alpha],
                        uv_offset,
                        bone_offset,
                        flags,
                        light_cell: inst.light_cell,
                        _pad0: [0; 3],
                        uv_mat,
                        origin: inst.transform.w_axis.to_array(),
                        _pad: [0; 44],
                    },
                    depth: if blended { depth } else { 0.0 },
                    priority: mat.priority_plane * 16 + li as i32,
                    order: self.draws.len(),
                });
            }
        }
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: PipelineKey) -> &wgpu::RenderPipeline {
        let (shader, layout, format) = (&self.shader, &self.pipeline_layout, self.color_format);
        self.pipelines.entry(key).or_insert_with(|| {
            use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
            let comp = |src, dst| C { src_factor: src, dst_factor: dst, operation: O::Add };
            let blend = match key.filter {
                filter::BLEND => Some(wgpu::BlendState {
                    color: comp(F::SrcAlpha, F::OneMinusSrcAlpha),
                    alpha: comp(F::One, F::OneMinusSrcAlpha),
                }),
                // Additive adds the color (the shader fades it by the layer's alpha), as
                // measured from the original's hero glow; AddAlpha adds it by its alpha.
                filter::ADDITIVE => Some(wgpu::BlendState {
                    color: comp(F::One, F::One),
                    alpha: comp(F::Zero, F::One),
                }),
                filter::ADD_ALPHA => Some(wgpu::BlendState {
                    color: comp(F::SrcAlpha, F::One),
                    alpha: comp(F::SrcAlpha, F::One),
                }),
                filter::MODULATE => Some(wgpu::BlendState {
                    color: comp(F::Zero, F::Src),
                    alpha: comp(F::Zero, F::One),
                }),
                filter::MODULATE_2X => Some(wgpu::BlendState {
                    color: comp(F::Dst, F::Src),
                    alpha: comp(F::Zero, F::One),
                }),
                _ => None,
            };
            let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Uint16x4, 4 => Uint32];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("model"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: if key.two_sided { None } else { Some(wgpu::Face::Back) },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::gpu::DEPTH_FORMAT,
                    depth_write_enabled: Some(key.depth_write),
                    depth_compare: Some(if key.depth_test {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    /// Uses a fog-of-war texture (see [`Globals::fow_extent`]).
    pub fn set_fog(&mut self, device: &wgpu::Device, fog: Arc<crate::fog::FogTexture>) {
        self.fog = fog;
        self.globals_bg = globals_bind_group(device, &self.globals_layout, &self.globals_buf, &self.bones_buf, &self.fog, &self.lights);
    }

    /// Uploads this frame's data and records draws into `pass`.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass, globals: &Globals) {
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(globals));
        if self.draws.is_empty() {
            self.bones.clear();
            if !self.defer_particles {
                self.render_particles(device, queue, pass);
            }
            return;
        }
        let mut rebind = false;
        if self.bones.len() > self.bones_cap {
            self.bones_cap = self.bones.len().next_power_of_two();
            self.bones_buf = bones_buffer(device, self.bones_cap);
            rebind = true;
        }
        if rebind {
            self.globals_bg = globals_bind_group(device, &self.globals_layout, &self.globals_buf, &self.bones_buf, &self.fog, &self.lights);
        }
        queue.write_buffer(&self.bones_buf, 0, bytemuck::cast_slice(&self.bones));
        // Opaque first, then blended back to front.
        let mut draws = std::mem::take(&mut self.draws);
        draws.sort_by(|a, b| {
            let ab = a.pipeline.filter >= filter::BLEND;
            let bb = b.pipeline.filter >= filter::BLEND;
            ab.cmp(&bb)
                .then(b.depth.total_cmp(&a.depth))
                .then(a.priority.cmp(&b.priority))
                .then(a.order.cmp(&b.order))
        });
        if draws.len() > self.draws_cap {
            self.draws_cap = draws.len().next_power_of_two();
            self.draws_buf = draws_buffer(device, self.draws_cap);
            self.draws_bg = draws_bind_group(device, &self.draw_layout, &self.draws_buf);
        }
        let uniforms: Vec<DrawUniform> = draws.iter().map(|d| d.uniform).collect();
        queue.write_buffer(&self.draws_buf, 0, bytemuck::cast_slice(&uniforms));
        for d in &draws {
            self.pipeline(device, d.pipeline);
            let key = (Arc::as_ptr(&d.texture) as usize, d.wrap.0, d.wrap.1);
            if !self.texture_bgs.contains_key(&key) {
                let s = &self.samplers[d.wrap.0 as usize | (d.wrap.1 as usize) << 1];
                let bg = texture_bind_group(device, &self.texture_layout, &d.texture.view, s);
                self.texture_bgs.insert(key, bg);
            }
        }
        pass.set_bind_group(0, &self.globals_bg, &[]);
        let mut last_model: Option<*const ModelGpu> = None;
        for (i, d) in draws.iter().enumerate() {
            let pipeline = &self.pipelines[&d.pipeline];
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, &self.draws_bg, &[(i as u64 * DRAW_ALIGN) as u32]);
            let key = (Arc::as_ptr(&d.texture) as usize, d.wrap.0, d.wrap.1);
            pass.set_bind_group(2, &self.texture_bgs[&key], &[]);
            if last_model != Some(Arc::as_ptr(&d.model)) {
                pass.set_vertex_buffer(0, d.model.vertices.slice(..));
                pass.set_index_buffer(d.model.indices.slice(..), wgpu::IndexFormat::Uint16);
                last_model = Some(Arc::as_ptr(&d.model));
            }
            let g = &d.model.geosets[d.geoset];
            pass.draw_indexed(g.first_index..g.first_index + g.index_count, g.base_vertex, 0..1);
        }
        self.bones.clear();
        // Keep textures alive until the next frame's draws replace them.
        self.draws = Vec::with_capacity(draws.len());
        if !self.defer_particles {
            self.render_particles(device, queue, pass);
        }
    }

    /// Draws this frame's particle quads after the models: depth tested, not written.
    pub fn render_particles(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass) {
        if self.particle_verts.is_empty() {
            self.particle_runs.clear();
            return;
        }
        let bytes = (self.particle_verts.len() * std::mem::size_of::<ParticleVertex>()) as u64;
        if self.particle_buf.as_ref().is_none_or(|b| b.size() < bytes) {
            self.particle_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buf = self.particle_buf.as_ref().unwrap();
        queue.write_buffer(buf, 0, bytemuck::cast_slice(&self.particle_verts));
        let runs = std::mem::take(&mut self.particle_runs);
        for (t, f, w, d, _) in &runs {
            self.particle_pipeline(device, *f, *d);
            let key = (Arc::as_ptr(t) as usize, *w, false);
            if !self.texture_bgs.contains_key(&key) {
                let bg = texture_bind_group(device, &self.texture_layout, &t.view, &self.samplers[*w as usize]);
                self.texture_bgs.insert(key, bg);
            }
        }
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_vertex_buffer(0, self.particle_buf.as_ref().unwrap().slice(..));
        for (t, f, w, d, r) in &runs {
            pass.set_pipeline(&self.particle_pipelines[&(*f, *d)]);
            pass.set_bind_group(1, &self.texture_bgs[&(Arc::as_ptr(t) as usize, *w, false)], &[]);
            pass.draw(r.clone(), 0..1);
        }
        self.particle_verts.clear();
    }

    fn particle_pipeline(&mut self, device: &wgpu::Device, filter: u32, depth: bool) {
        if self.particle_pipelines.contains_key(&(filter, depth)) {
            return;
        }
        use crate::particles::filter as pf;
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
        let comp = |src, dst| C { src_factor: src, dst_factor: dst, operation: O::Add };
        let blend = match filter {
            pf::ADDITIVE => wgpu::BlendState { color: comp(F::SrcAlpha, F::One), alpha: comp(F::Zero, F::One) },
            pf::ADD_COLOR => wgpu::BlendState { color: comp(F::One, F::One), alpha: comp(F::Zero, F::One) },
            pf::MODULATE => wgpu::BlendState { color: comp(F::Zero, F::Src), alpha: comp(F::Zero, F::One) },
            pf::MODULATE_2X => wgpu::BlendState { color: comp(F::Dst, F::Src), alpha: comp(F::Zero, F::One) },
            _ => wgpu::BlendState { color: comp(F::SrcAlpha, F::OneMinusSrcAlpha), alpha: comp(F::One, F::OneMinusSrcAlpha) },
        };
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Uint32];
        let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&self.particle_layout),
            vertex: wgpu::VertexState {
                module: &self.particle_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ParticleVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.particle_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: self.color_format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::gpu::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(if depth { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.particle_pipelines.insert((filter, depth), p);
    }
}

fn bones_buffer(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bones"),
        size: (cap * 64) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn draws_buffer(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model draws"),
        size: cap as u64 * DRAW_ALIGN,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn globals_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    bones: &wgpu::Buffer,
    fog: &crate::fog::FogTexture,
    lights: &crate::lights::LightBuffers,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model globals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: bones.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&fog.view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&fog.sampler) },
            wgpu::BindGroupEntry { binding: 4, resource: lights.lights.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: lights.cells.as_entire_binding() },
        ],
    })
}

fn draws_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, buf: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model draws"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: buf,
                offset: 0,
                size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniform>() as u64),
            }),
        }],
    })
}

pub fn texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

pub fn make_sampler(device: &wgpu::Device, wrap_u: bool, wrap_v: bool) -> wgpu::Sampler {
    let mode = |w| if w { wgpu::AddressMode::Repeat } else { wgpu::AddressMode::ClampToEdge };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: mode(wrap_u),
        address_mode_v: mode(wrap_v),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 1,
        ..Default::default()
    })
}

pub fn default_globals(view_proj: Mat4, camera: Vec3) -> Globals {
    Globals {
        view_proj: view_proj.to_cols_array_2d(),
        light_dir: [0.3, 0.3, -0.25, 0.0],
        light_color: [1.0, 1.0, 1.0, 1.0],
        ambient: [0.35, 0.35, 0.35, 1.0],
        fog_color: [0.0; 4],
        fog: [0.0; 4],
        camera_pos: [camera.x, camera.y, camera.z, 1.0],
        terrain_light_color: [1.0, 1.0, 1.0, 1.0],
        terrain_ambient: [0.35, 0.35, 0.35, 1.0],
        fow_extent: [0.0; 4],
        fow_terrain: [0.0; 4],
        fow_object: [0.0; 4],
        fow_masked_terrain: [0.0, 0.0, 0.0, 1.0],
        fow_masked_object: [0.0, 0.0, 0.0, 1.0],
        water_tint: [1.0; 4],
        light_grid: [0.0; 4],
        fow_boundary_terrain: [0.0; 4],
        fow_fogged_boundary_terrain: [0.0; 4],
        fow_boundary_object: [1.0; 4],
        fow_fogged_boundary_object: [1.0; 4],
    }
}

pub use mdx::Model as Mdx;
