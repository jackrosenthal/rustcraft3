//! Day/night lighting from the "DNC" models a map selects with SetDayNightModels.  Each
//! model's first light is sampled at the time of day: its single sequence spans 24 hours.

use glam::{Quat, Vec3};
use w3_formats::mdx::Model;

use crate::anim::{Time, sample};

#[derive(Clone, Copy, Debug)]
pub struct Light {
    /// Direction from the light toward the world.
    pub direction: Vec3,
    /// Diffuse color times intensity.
    pub diffuse: Vec3,
    /// Ambient color times ambient intensity.
    pub ambient: Vec3,
}

fn bgr(c: [f32; 3]) -> Vec3 {
    Vec3::new(c[2], c[1], c[0])
}

/// Samples a DNC model at `hour` (0-24).
pub fn light_at(model: &Model, hour: f32) -> Option<Light> {
    let l = model.lights.first()?;
    let seq = model.sequences.first()?;
    let len = (seq.end - seq.start) as f32;
    let frame = seq.start + ((hour / 24.0).rem_euclid(1.0) * len) as u32;
    let time = Time { frame, seq_start: seq.start, seq_end: seq.end, global: 0 };
    let g = &model.global_sequences;
    let color = sample(l.tracks.vec3(b"KLAC"), &time, g, l.color);
    let intensity = sample(l.tracks.f32(b"KLAI"), &time, g, l.intensity);
    let amb = sample(l.tracks.vec3(b"KLBC"), &time, g, l.ambient_color);
    let amb_intensity = sample(l.tracks.f32(b"KLBI"), &time, g, l.ambient_intensity);
    let r = sample(l.node.tracks.quat(b"KGRT"), &time, g, [0.0, 0.0, 0.0, 1.0]);
    let q = Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize();
    Some(Light {
        direction: q * Vec3::NEG_Z,
        diffuse: bgr(color) * intensity,
        ambient: bgr(amb) * amb_intensity,
    })
}
