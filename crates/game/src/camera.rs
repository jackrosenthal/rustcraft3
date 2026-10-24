//! The local player's camera: a target point and the seven camera fields, each of which
//! may be moving toward a value over a duration (SetCameraField, PanCameraToTimed,
//! CameraSetupApply).  Angles are kept in degrees; the JASS getters return radians.

use crate::world::World;

pub const FIELD_TARGET_DISTANCE: usize = 0;
pub const FIELD_FAR_Z: usize = 1;
pub const FIELD_ANGLE_OF_ATTACK: usize = 2;
pub const FIELD_FIELD_OF_VIEW: usize = 3;
pub const FIELD_ROLL: usize = 4;
pub const FIELD_ROTATION: usize = 5;
pub const FIELD_ZOFFSET: usize = 6;
pub const FIELD_COUNT: usize = 7;

/// Whether a field is an angle (stored in degrees, read in radians).
pub fn is_angle(field: usize) -> bool {
    matches!(field, FIELD_ANGLE_OF_ATTACK | FIELD_FIELD_OF_VIEW | FIELD_ROLL | FIELD_ROTATION)
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Lerp {
    from: f32,
    to: f32,
    elapsed: f32,
    duration: f32,
}

impl Lerp {
    fn value(&self) -> f32 {
        let t = if self.duration > 0.0 { (self.elapsed / self.duration).clamp(0.0, 1.0) } else { 1.0 };
        self.from + (self.to - self.from) * t
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GameCamera {
    pub x: f32,
    pub y: f32,
    pub fields: [f32; FIELD_COUNT],
    /// The game camera's fields (UI\MiscData.txt [Camera], first zoom level), which
    /// ResetToGameCamera returns to.
    pub defaults: [f32; FIELD_COUNT],
    pan: Option<(Lerp, Lerp)>,
    lerps: [Option<Lerp>; FIELD_COUNT],
    /// The unit the camera follows and the offset from it (SetCameraTargetController).
    #[serde(default)]
    pub follow: Option<(u32, f32, f32)>,
    /// The followed unit's facing turns the offset and the view (the controller's
    /// orientation flags: SetCameraOrientController, or SetCameraTargetController's
    /// inheritOrientation).
    #[serde(default)]
    pub follow_orient: bool,
    /// Shaking of the eye and of the target: magnitude, velocity, and whether only up
    /// and down (CameraSetSourceNoise, CameraSetTargetNoise, and their Ex forms).
    #[serde(default)]
    pub source_noise: (f32, f32, bool),
    #[serde(default)]
    pub target_noise: (f32, f32, bool),
    /// A cinematic camera model and when it started (SetCinematicCamera).
    #[serde(default)]
    pub cinematic: Option<(String, f32)>,
    /// CameraSetSmoothingFactor, 0 to 10 (the camera's +0x5bc).
    #[serde(default)]
    pub smoothing: f32,
    /// SetCameraRotateMode (FUN_6f3633f0): the point turned about, the target's offset
    /// from it, the rotation it started at, the sweep in degrees, and the time.
    #[serde(default)]
    rotate: Option<RotateMode>,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct RotateMode {
    pivot: (f32, f32),
    offset: (f32, f32),
    rotation: f32,
    sweep: f32,
    elapsed: f32,
    duration: f32,
}

/// How far the camera's target moves toward where it should be this frame
/// (FUN_6f35f750): moves over 1000 at once; otherwise the share is the move (in
/// thousands) to the smoothing factor's power times 0.8, plus 0.15, plus (10 / fps) to
/// the 1.5 (1 under 10 frames a second), at most all of it.
pub fn smoothing_share(factor: f32, distance: f32, fps: f32) -> f32 {
    if distance > 1000.0 {
        return 1.0;
    }
    let pace = if fps >= 10.0 { (10.0 / fps).powf(1.5) } else { 1.0 };
    ((distance / 1000.0).powf(factor) * 0.8 + 0.15 + pace).min(1.0)
}

/// A camera noise's offset at time `t`: two sine waves per axis at the velocity's
/// rate, scaled to the magnitude.
pub fn noise_offset((mag, vel, vertical): (f32, f32, bool), t: f32) -> [f32; 3] {
    if mag == 0.0 {
        return [0.0; 3];
    }
    let w = vel.max(0.0) * 0.01;
    let wave = |k: f32, p: f32| ((t * w * k + p).sin() * 0.6 + (t * w * k * 2.3 + p * 1.7).sin() * 0.4) * mag;
    if vertical { [0.0, 0.0, wave(1.0, 0.3)] } else { [wave(1.0, 0.0), wave(1.13, 1.1), wave(0.91, 2.2)] }
}

impl GameCamera {
    pub fn new(misc_data: &w3_formats::profile::Profile) -> GameCamera {
        let first = |key: &str, d: f32| misc_data.get_list("Camera", key).first().and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        let mut defaults = [0.0; FIELD_COUNT];
        defaults[FIELD_TARGET_DISTANCE] = first("Distance", 1650.0);
        defaults[FIELD_FAR_Z] = first("FarZ", 5000.0);
        defaults[FIELD_ANGLE_OF_ATTACK] = first("AOA", 304.0);
        defaults[FIELD_FIELD_OF_VIEW] = first("FOV", 70.0);
        defaults[FIELD_ROLL] = 0.0;
        defaults[FIELD_ROTATION] = first("Rotation", 90.0);
        defaults[FIELD_ZOFFSET] = first("Height", 0.0);
        GameCamera { x: 0.0, y: 0.0, fields: defaults, defaults, pan: None, lerps: [None; FIELD_COUNT], follow: None, follow_orient: false, source_noise: (0.0, 0.0, false), target_noise: (0.0, 0.0, false), cinematic: None, smoothing: 0.0, rotate: None }
    }

    /// Moves a field to `value` over `duration` seconds (at once when zero).
    pub fn set_field(&mut self, field: usize, value: f32, duration: f32) {
        if field >= FIELD_COUNT {
            return;
        }
        if duration <= 0.0 {
            self.fields[field] = value;
            self.lerps[field] = None;
        } else {
            self.lerps[field] = Some(Lerp { from: self.fields[field], to: value, elapsed: 0.0, duration });
        }
    }

    /// Pans the target to a point over `duration` seconds (at once when zero).
    pub fn pan_to(&mut self, x: f32, y: f32, duration: f32) {
        self.rotate = None;
        if duration <= 0.0 {
            self.x = x;
            self.y = y;
            self.pan = None;
        } else {
            self.pan = Some((Lerp { from: self.x, to: x, elapsed: 0.0, duration }, Lerp { from: self.y, to: y, elapsed: 0.0, duration }));
        }
    }

    /// Stops moving (StopCamera).
    pub fn stop(&mut self) {
        self.pan = None;
        self.lerps = [None; FIELD_COUNT];
        self.rotate = None;
    }

    /// SetCameraRotateMode: the camera turns about a point by `sweep` radians over
    /// `duration` seconds, its target and eye keeping their places relative to it.
    pub fn rotate_about(&mut self, x: f32, y: f32, sweep: f32, duration: f32) {
        self.pan = None;
        self.rotate = Some(RotateMode { pivot: (x, y), offset: (self.x - x, self.y - y), rotation: self.fields[FIELD_ROTATION], sweep: sweep.to_degrees(), elapsed: 0.0, duration });
        self.update(0.0);
    }

    pub fn panning(&self) -> bool {
        self.pan.is_some()
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(r) = self.rotate.as_mut() {
            r.elapsed += dt;
            let t = if r.duration > 0.0 { (r.elapsed / r.duration).min(1.0) } else { 1.0 };
            let turn = r.sweep * t;
            let (s, c) = turn.to_radians().sin_cos();
            self.x = r.pivot.0 + r.offset.0 * c - r.offset.1 * s;
            self.y = r.pivot.1 + r.offset.0 * s + r.offset.1 * c;
            self.fields[FIELD_ROTATION] = r.rotation + turn;
        }
        if let Some((px, py)) = self.pan.as_mut() {
            px.elapsed += dt;
            py.elapsed += dt;
            self.x = px.value();
            self.y = py.value();
            if px.elapsed >= px.duration {
                self.pan = None;
            }
        }
        for f in 0..FIELD_COUNT {
            if let Some(l) = self.lerps[f].as_mut() {
                l.elapsed += dt;
                self.fields[f] = l.value();
                if l.elapsed >= l.duration {
                    self.lerps[f] = None;
                }
            }
        }
    }
}

impl World {
    /// A camera target kept inside the camera bounds (SetCameraBounds: their corners'
    /// box), for the player's scrolling.
    pub fn clamp_to_camera_bounds(&self, x: f32, y: f32) -> (f32, f32) {
        let b = &self.camera_bounds;
        let (xs, ys) = ([b[0], b[2], b[4], b[6]], [b[1], b[3], b[5], b[7]]);
        let (x0, x1) = (xs.iter().copied().fold(f32::INFINITY, f32::min), xs.iter().copied().fold(f32::NEG_INFINITY, f32::max));
        let (y0, y1) = (ys.iter().copied().fold(f32::INFINITY, f32::min), ys.iter().copied().fold(f32::NEG_INFINITY, f32::max));
        if x1 <= x0 || y1 <= y0 {
            return (x, y);
        }
        (x.clamp(x0, x1), y.clamp(y0, y1))
    }

    /// Applies a camera setup: its fields (each over its own duration, or `force`), and
    /// its destination when `pan` is set (over the longest field duration, or `force`;
    /// at once unless `timed`).
    pub fn apply_camera_setup(&mut self, setup: u32, pan: bool, timed: bool, force: Option<f32>) {
        let Some(crate::types::Obj::CameraSetup(s)) = self.handles.get(setup) else { return };
        let s = (**s).clone();
        let mut longest = 0.0f32;
        for (&f, &v) in &s.fields {
            let d = force.unwrap_or_else(|| s.durations.get(&f).copied().unwrap_or(0.0));
            longest = longest.max(d);
            self.camera.set_field(f as usize, v, d);
        }
        if pan {
            let d = if timed { force.unwrap_or(longest) } else { 0.0 };
            self.camera.pan_to(s.x, s.y, d);
        }
    }
}
