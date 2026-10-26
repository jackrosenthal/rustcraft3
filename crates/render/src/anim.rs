//! Keyframe sampling and node hierarchy evaluation for MDX models.

use glam::{Mat4, Quat, Vec3};
use w3_formats::mdx::{self, Interp, Key, Model, Track, node_flags};

pub trait Sample: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
    fn hermite(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self;
    fn bezier(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self;
}

fn hermite_f(a: f32, ao: f32, bi: f32, b: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    a * (2.0 * t3 - 3.0 * t2 + 1.0) + ao * (t3 - 2.0 * t2 + t) + bi * (t3 - t2) + b * (3.0 * t2 - 2.0 * t3)
}

fn bezier_f(a: f32, ao: f32, bi: f32, b: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    a * u * u * u + ao * 3.0 * t * u * u + bi * 3.0 * t * t * u + b * t * t * t
}

impl Sample for f32 {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
    fn hermite(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        hermite_f(a.value, a.out_tan, b.in_tan, b.value, t)
    }
    fn bezier(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        bezier_f(a.value, a.out_tan, b.in_tan, b.value, t)
    }
}

impl Sample for u32 {
    fn lerp(a: Self, _b: Self, _t: f32) -> Self {
        a
    }
    fn hermite(a: &Key<Self>, _b: &Key<Self>, _t: f32) -> Self {
        a.value
    }
    fn bezier(a: &Key<Self>, _b: &Key<Self>, _t: f32) -> Self {
        a.value
    }
}

impl Sample for [f32; 3] {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
    }
    fn hermite(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        [0, 1, 2].map(|i| hermite_f(a.value[i], a.out_tan[i], b.in_tan[i], b.value[i], t))
    }
    fn bezier(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        [0, 1, 2].map(|i| bezier_f(a.value[i], a.out_tan[i], b.in_tan[i], b.value[i], t))
    }
}

fn q(v: [f32; 4]) -> Quat {
    Quat::from_xyzw(v[0], v[1], v[2], v[3])
}

impl Sample for [f32; 4] {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        q(a).slerp(q(b), t).to_array()
    }
    fn hermite(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        squad(a, b, t)
    }
    fn bezier(a: &Key<Self>, b: &Key<Self>, t: f32) -> Self {
        squad(a, b, t)
    }
}

fn squad(a: &Key<[f32; 4]>, b: &Key<[f32; 4]>, t: f32) -> [f32; 4] {
    let p = q(a.value).slerp(q(b.value), t);
    let s = q(a.out_tan).slerp(q(b.in_tan), t);
    p.slerp(s, 2.0 * t * (1.0 - t)).to_array()
}

/// The time a model is being evaluated at.
#[derive(Clone, Copy, Debug, Default)]
pub struct Time {
    /// Frame within the model's timeline (already offset into the current sequence).
    pub frame: u32,
    pub seq_start: u32,
    pub seq_end: u32,
    /// Milliseconds since the instance was created, for global sequences.
    pub global: u32,
}

pub fn sample<T: Sample>(track: Option<&Track<T>>, time: &Time, globals: &[u32], default: T) -> T {
    let Some(track) = track else { return default };
    if track.keys.is_empty() {
        return default;
    }
    let (frame, start, end) = match track.global_seq {
        Some(g) => match globals.get(g as usize) {
            Some(&d) if d > 0 => (time.global % d, 0, d),
            // A global sequence of no length holds its track at the first key (the
            // Archer's portrait sets its root 21.8 lower so, keyed at 155633 on one of
            // length 0; without it the camera saw her shoulder, not her face).
            _ => return track.keys[0].value,
        },
        None => (time.frame, time.seq_start, time.seq_end),
    };
    let (frame, start, end) = (frame as i32, start as i32, end as i32);
    let keys = &track.keys;
    // Keys belonging to the active interval.
    let lo = keys.partition_point(|k| k.frame < start);
    let hi = keys.partition_point(|k| k.frame <= end);
    if lo >= hi {
        return default;
    }
    let ks = &keys[lo..hi];
    if frame <= ks[0].frame {
        return ks[0].value;
    }
    let i = ks.partition_point(|k| k.frame <= frame);
    if i >= ks.len() {
        return ks[ks.len() - 1].value;
    }
    let (a, b) = (&ks[i - 1], &ks[i]);
    let span = (b.frame - a.frame) as f32;
    let t = if span > 0.0 { (frame - a.frame) as f32 / span } else { 0.0 };
    match track.interp {
        Interp::None => a.value,
        Interp::Linear => T::lerp(a.value, b.value, t),
        Interp::Hermite => T::hermite(a, b, t),
        Interp::Bezier => T::bezier(a, b, t),
    }
}

/// Evaluates every node's world transform for one instance.
pub struct Skeleton {
    /// Indexed by object id.
    pub world: Vec<Mat4>,
    order: Vec<usize>,
    parents: Vec<Option<usize>>,
    flags: Vec<u32>,
    pivots: Vec<Vec3>,
    /// A node turned to face a point in model space instead of animating its rotation
    /// (SetUnitLookAt).
    pub look: Option<(usize, Vec3)>,
}

impl Skeleton {
    pub fn new(model: &Model) -> Self {
        let nodes = model.nodes();
        let n = nodes.len();
        let parents: Vec<Option<usize>> = nodes
            .iter()
            .map(|nd| nd.and_then(|nd| nd.parent).map(|p| p as usize).filter(|&p| p < n))
            .collect();
        let flags = nodes.iter().map(|nd| nd.map_or(0, |nd| nd.flags)).collect();
        // Parents before children.
        let mut order = Vec::with_capacity(n);
        let mut state = vec![0u8; n];
        fn visit(i: usize, parents: &[Option<usize>], state: &mut [u8], order: &mut Vec<usize>) {
            if state[i] != 0 {
                return;
            }
            state[i] = 1;
            if let Some(p) = parents[i] {
                visit(p, parents, state, order);
            }
            state[i] = 2;
            order.push(i);
        }
        for i in 0..n {
            visit(i, &parents, &mut state, &mut order);
        }
        let pivots = (0..n)
            .map(|i| model.pivots.get(i).map_or(Vec3::ZERO, |p| Vec3::from_array(*p)))
            .collect();
        Skeleton { world: vec![Mat4::IDENTITY; n], order, parents, flags, pivots, look: None }
    }

    /// `camera_rotation` is the inverse view rotation, used for billboarded nodes.
    /// With `blend`, each node's translation, rotation, and scale run from their values
    /// at that earlier time (the sequence left) toward the current ones by the weight.
    pub fn update(&mut self, model: &Model, time: &Time, camera_rotation: Quat, blend: Option<(&Time, f32)>) {
        let nodes = model.nodes();
        let g = &model.global_sequences;
        let mut local_rot = vec![Quat::IDENTITY; self.world.len()];
        let mut world_rot = vec![Quat::IDENTITY; self.world.len()];
        let mut world_scale = vec![Vec3::ONE; self.world.len()];
        for &i in &self.order {
            let Some(node) = nodes[i] else { continue };
            let mut t = Vec3::from_array(sample(node.tracks.vec3(b"KGTR"), time, g, [0.0; 3]));
            let mut r = q(sample(node.tracks.quat(b"KGRT"), time, g, [0.0, 0.0, 0.0, 1.0]));
            let mut s = Vec3::from_array(sample(node.tracks.vec3(b"KGSC"), time, g, [1.0; 3]));
            if let Some((from, w)) = blend {
                let t0 = Vec3::from_array(sample(node.tracks.vec3(b"KGTR"), from, g, [0.0; 3]));
                let r0 = q(sample(node.tracks.quat(b"KGRT"), from, g, [0.0, 0.0, 0.0, 1.0]));
                let s0 = Vec3::from_array(sample(node.tracks.vec3(b"KGSC"), from, g, [1.0; 3]));
                t = t0.lerp(t, w);
                r = r0.slerp(r, w);
                s = s0.lerp(s, w);
            }
            let pivot = self.pivots[i];
            let flags = self.flags[i];
            let parent = self.parents[i];
            let (pw, prot, pscale) = match parent {
                Some(p) => (self.world[p], world_rot[p], world_scale[p]),
                None => (Mat4::IDENTITY, Quat::IDENTITY, Vec3::ONE),
            };
            let mut r = r;
            if flags & node_flags::BILLBOARDED != 0 {
                // Face the camera regardless of the parent's rotation.
                r = prot.inverse() * camera_rotation;
            }
            // A looking node's +x points at its target.
            if let Some((node, target)) = self.look.filter(|l| l.0 == i) {
                let at = pw.transform_point3(pivot + t);
                let want = (target - at).normalize_or_zero();
                if want != Vec3::ZERO {
                    r = prot.inverse() * Quat::from_rotation_arc(Vec3::X, want);
                }
                let _ = node;
            }
            local_rot[i] = r;
            let mut parent_m = pw;
            if parent.is_some()
                && flags
                    & (node_flags::DONT_INHERIT_TRANSLATION
                        | node_flags::DONT_INHERIT_ROTATION
                        | node_flags::DONT_INHERIT_SCALING)
                    != 0
            {
                let (ps, pr, pt) = pw.to_scale_rotation_translation();
                let ps = if flags & node_flags::DONT_INHERIT_SCALING != 0 { Vec3::ONE } else { ps };
                let pr = if flags & node_flags::DONT_INHERIT_ROTATION != 0 { Quat::IDENTITY } else { pr };
                let pt = if flags & node_flags::DONT_INHERIT_TRANSLATION != 0 { Vec3::ZERO } else { pt };
                parent_m = Mat4::from_scale_rotation_translation(ps, pr, pt);
            }
            let local = Mat4::from_translation(pivot + t)
                * Mat4::from_quat(r)
                * Mat4::from_scale(s)
                * Mat4::from_translation(-pivot);
            self.world[i] = parent_m * local;
            world_rot[i] = prot * r;
            world_scale[i] = pscale * s;
        }
    }
}

/// Picks the sequence a name refers to, the way animation names are matched: by the
/// first word of the sequence name, case-insensitively.
pub fn find_sequence(model: &Model, name: &str) -> Option<usize> {
    let want = name.to_ascii_lowercase();
    model
        .sequences
        .iter()
        .position(|s| s.name.to_ascii_lowercase() == want)
        .or_else(|| {
            model.sequences.iter().position(|s| {
                s.name
                    .split_whitespace()
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case(&want))
            })
        })
}

/// Every sequence whose words are `name`'s, in any order (for example "Attack - 1" and
/// "Attack - 2" for "attack", "Stand Upgrade First" for "stand first upgrade"),
/// excluding those that carry extra qualifiers such as "Attack Slam" unless asked for.
pub fn sequences_named(model: &Model, name: &str) -> Vec<usize> {
    let mut want: Vec<String> = name.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
    want.sort();
    model
        .sequences
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let mut words: Vec<String> = s
                .name
                .split(|c: char| c.is_whitespace() || c == '-')
                .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
                .map(|w| w.to_ascii_lowercase())
                .collect();
            words.sort();
            words == want
        })
        .map(|(i, _)| i)
        .collect()
}

/// Layer alpha and texture id at a given time.
pub fn layer_state(layer: &mdx::Layer, time: &Time, globals: &[u32]) -> (f32, u32) {
    (
        sample(layer.tracks.f32(b"KMTA"), time, globals, layer.alpha),
        sample(layer.tracks.u32(b"KMTF"), time, globals, layer.texture_id),
    )
}

/// Geoset animation alpha and color at a given time.
pub fn geoset_anim_state(ga: &mdx::GeosetAnim, time: &Time, globals: &[u32]) -> (f32, [f32; 3]) {
    (
        sample(ga.tracks.f32(b"KGAO"), time, globals, ga.alpha),
        sample(ga.tracks.vec3(b"KGAC"), time, globals, ga.color),
    )
}
