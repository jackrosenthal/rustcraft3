//! Particle emitters (MDX PRE2): emission from a node's plane into a cone, gravity,
//! three-segment color, alpha, and scale over life, and texture atlas frames, drawn as
//! camera-facing (or ground-flat) quads with optional tails.

use glam::{Mat4, Quat, Vec3};
use w3_formats::mdx::{Model, ParticleEmitter2};

use crate::anim::{Time, sample};

/// Node flags on particle emitters.
pub mod flags {
    pub const UNSHADED: u32 = 0x8000;
    pub const SORT_FAR_Z: u32 = 0x10000;
    pub const LINE_EMITTER: u32 = 0x20000;
    pub const UNFOGGED: u32 = 0x40000;
    pub const MODEL_SPACE: u32 = 0x80000;
    pub const XY_QUAD: u32 = 0x100000;
}

/// PRE2 filter modes (not the same numbering as material layers).
pub mod filter {
    pub const BLEND: u32 = 0;
    pub const ADDITIVE: u32 = 1;
    pub const MODULATE: u32 = 2;
    pub const MODULATE_2X: u32 = 3;
    pub const ALPHA_KEY: u32 = 4;
    /// Not an MDX mode: a material layer's Additive, the color added by its alpha alone
    /// whatever the texture's (weather's alphaMode 1).
    pub const ADD_COLOR: u32 = 5;
}

#[derive(Clone, Debug)]
struct Particle {
    emitter: usize,
    /// World position, or position in the model's space for model-space emitters.
    pos: Vec3,
    vel: Vec3,
    age: f32,
}

/// One emitter's running state.
#[derive(Clone, Debug, Default)]
struct EmitterState {
    accumulated: f32,
    last_visible: bool,
}

#[derive(Clone, Debug)]
pub struct Particles {
    /// False stops emission; live particles finish their lives.
    pub emitting: bool,
    particles: Vec<Particle>,
    emitters: Vec<EmitterState>,
    rng: u32,
    last_time: Option<f32>,
}

impl Default for Particles {
    fn default() -> Self {
        Particles { emitting: true, particles: Vec::new(), emitters: Vec::new(), rng: 0x9e37_79b9, last_time: None }
    }
}

/// A quad ready to draw.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub corners: [Vec3; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub texture_id: u32,
    pub replaceable_id: u32,
    pub filter: u32,
    pub unshaded: bool,
    pub unfogged: bool,
}

/// Total particles one instance may keep.
const MAX_PARTICLES: usize = 1000;

impl Particles {
    fn random(&mut self) -> f32 {
        // xorshift32
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Emits and moves particles.  `nodes` are the instance's node world matrices (in
    /// model space) and `transform` places the model in the world; `now` is the
    /// instance's clock in milliseconds.
    pub fn update(&mut self, model: &Model, nodes: &[Mat4], transform: Mat4, time: &Time, now: f32) {
        let emitters = &model.particle_emitters2;
        if emitters.is_empty() {
            return;
        }
        if self.emitters.len() != emitters.len() {
            self.emitters = vec![EmitterState::default(); emitters.len()];
        }
        let dt = match self.last_time {
            Some(t) => ((now - t) / 1000.0).clamp(0.0, 0.1),
            None => 0.0,
        };
        self.last_time = Some(now);
        let g = &model.global_sequences;
        // Move and age.
        for p in &mut self.particles {
            let e = &emitters[p.emitter];
            let gravity = sample(e.tracks.f32(b"KP2G"), time, g, e.gravity);
            p.vel.z -= gravity * dt;
            p.pos += p.vel * dt;
            p.age += dt;
        }
        self.particles.retain(|p| p.age < emitters[p.emitter].lifespan);
        // Emit.
        for (ei, e) in emitters.iter().enumerate() {
            let visible = self.emitting && sample(e.tracks.f32(b"KP2V"), time, g, 1.0) > 0.0;
            let rate = sample(e.tracks.f32(b"KP2E"), time, g, e.emission_rate);
            let count = if e.squirt {
                // A squirt emits its whole rate at once when it becomes visible.
                let n = if visible && !self.emitters[ei].last_visible { rate } else { 0.0 };
                n as usize
            } else if visible {
                let st = &mut self.emitters[ei];
                st.accumulated += rate * dt;
                let n = st.accumulated.floor();
                st.accumulated -= n;
                n as usize
            } else {
                0
            };
            self.emitters[ei].last_visible = visible;
            if count == 0 {
                continue;
            }
            let node = nodes.get(e.node.object_id as usize).copied().unwrap_or(Mat4::IDENTITY);
            let pivot = model.pivots.get(e.node.object_id as usize).map_or(Vec3::ZERO, |p| Vec3::from_array(*p));
            let model_space = e.node.flags & flags::MODEL_SPACE != 0;
            let world = if model_space { node } else { transform * node };
            let (scale, rot, _) = world.to_scale_rotation_translation();
            let speed = sample(e.tracks.f32(b"KP2S"), time, g, e.speed);
            let variation = sample(e.tracks.f32(b"KP2R"), time, g, e.variation);
            let latitude = sample(e.tracks.f32(b"KP2L"), time, g, e.latitude).to_radians();
            let width = sample(e.tracks.f32(b"KP2W"), time, g, e.width);
            let length = sample(e.tracks.f32(b"KP2N"), time, g, e.length);
            for _ in 0..count {
                if self.particles.len() >= MAX_PARTICLES {
                    break;
                }
                // Somewhere on the emitter's plane, around its pivot.
                let local = pivot + Vec3::new((self.random() - 0.5) * width, (self.random() - 0.5) * length, 0.0);
                let pos = world.transform_point3(local);
                // Into a cone of half-angle `latitude` around the node's +Z (a line
                // emitter spreads in its YZ plane only).
                let dir = if e.node.flags & flags::LINE_EMITTER != 0 {
                    let a = (self.random() * 2.0 - 1.0) * latitude;
                    Vec3::new(0.0, a.sin(), a.cos())
                } else {
                    let polar = self.random() * latitude;
                    let az = self.random() * std::f32::consts::TAU;
                    Vec3::new(polar.sin() * az.cos(), polar.sin() * az.sin(), polar.cos())
                };
                let s = speed * (1.0 + (self.random() * 2.0 - 1.0) * variation);
                let vel = (rot * dir) * s * scale.max_element();
                self.particles.push(Particle { emitter: ei, pos, vel, age: 0.0 });
            }
        }
    }

    /// Builds quads facing the camera.  `camera_rot` is the billboard rotation: it turns
    /// +x toward the camera, +y to the screen's right, and +z to its top.
    pub fn quads(&self, model: &Model, transform: Mat4, camera_rot: Quat, out: &mut Vec<Quad>) {
        let right = camera_rot * Vec3::Y;
        let up = camera_rot * Vec3::Z;
        let toward = camera_rot * Vec3::X;
        let scale = transform.to_scale_rotation_translation().0.max_element();
        for p in &self.particles {
            let e = &model.particle_emitters2[p.emitter];
            let t = (p.age / e.lifespan.max(1e-4)).clamp(0.0, 1.0);
            let (color, alpha, size) = segment(e, t);
            let pos = if e.node.flags & flags::MODEL_SPACE != 0 { transform.transform_point3(p.pos) } else { p.pos };
            // Segment scaling is the quad's half-extent (measured against the original).
            let half = size * if e.node.flags & flags::MODEL_SPACE != 0 { scale } else { 1.0 };
            let uv = atlas(e, t);
            let base = Quad {
                corners: [Vec3::ZERO; 4],
                uv,
                color: [color.x, color.y, color.z, alpha],
                texture_id: e.texture_id,
                replaceable_id: e.replaceable_id,
                filter: e.filter_mode,
                unshaded: e.node.flags & flags::UNSHADED != 0,
                unfogged: e.node.flags & flags::UNFOGGED != 0,
            };
            // Head: 0, tail: 1, both: 2.
            if e.head_or_tail != 1 {
                let (r, u) = if e.node.flags & flags::XY_QUAD != 0 { (Vec3::X, Vec3::Y) } else { (right, up) };
                let mut q = base;
                q.corners = [pos - r * half + u * half, pos + r * half + u * half, pos - r * half - u * half, pos + r * half - u * half];
                out.push(q);
            }
            if e.head_or_tail != 0 {
                // The tail runs back along the velocity for TailLength of a second.
                let tail = pos - p.vel * e.tail_length;
                let along = (pos - tail).normalize_or_zero();
                let side = along.cross(toward).normalize_or_zero() * half;
                let mut q = base;
                q.corners = [pos - side, pos + side, tail - side, tail + side];
                out.push(q);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }
}

/// Color, alpha, and size at fraction `t` of life: start to middle over the first `time`
/// of it, then middle to end.
fn segment(e: &ParticleEmitter2, t: f32) -> (Vec3, f32, f32) {
    let c = e.segment_color.map(Vec3::from_array);
    let a = e.segment_alpha.map(|v| v as f32 / 255.0);
    let s = e.segment_scaling;
    let mid = e.time.clamp(1e-4, 1.0 - 1e-4);
    if t < mid {
        let f = t / mid;
        (c[0].lerp(c[1], f), a[0] + (a[1] - a[0]) * f, s[0] + (s[1] - s[0]) * f)
    } else {
        let f = (t - mid) / (1.0 - mid);
        (c[1].lerp(c[2], f), a[1] + (a[2] - a[1]) * f, s[1] + (s[2] - s[1]) * f)
    }
}

/// Texture coordinates of the atlas cell for fraction `t` of life: the head's life
/// interval before the middle time, its decay interval after; each runs from its start
/// to end cell, `repeat` times.
fn atlas(e: &ParticleEmitter2, t: f32) -> [[f32; 2]; 4] {
    let (cols, rows) = (e.columns.max(1), e.rows.max(1));
    let mid = e.time.clamp(1e-4, 1.0 - 1e-4);
    let (iv, f) = if t < mid { (e.head_intervals[0], t / mid) } else { (e.head_intervals[1], (t - mid) / (1.0 - mid)) };
    let (start, end, repeat) = (iv[0], iv[1], iv[2].max(1));
    let span = end.saturating_sub(start) + 1;
    let k = ((f * repeat as f32).fract() * span as f32) as u32;
    let cell = (start + k.min(span - 1)).min(cols * rows - 1);
    let (cx, cy) = ((cell % cols) as f32, (cell / cols) as f32);
    let (w, h) = (1.0 / cols as f32, 1.0 / rows as f32);
    let (u0, v0, u1, v1) = (cx * w, cy * h, cx * w + w, cy * h + h);
    [[u0, v0], [u1, v0], [u0, v1], [u1, v1]]
}
