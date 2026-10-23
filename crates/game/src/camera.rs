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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Debug)]
pub struct GameCamera {
    pub x: f32,
    pub y: f32,
    pub fields: [f32; FIELD_COUNT],
    /// The game camera's fields (UI\MiscData.txt [Camera], first zoom level), which
    /// ResetToGameCamera returns to.
    pub defaults: [f32; FIELD_COUNT],
    pan: Option<(Lerp, Lerp)>,
    lerps: [Option<Lerp>; FIELD_COUNT],
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
        GameCamera { x: 0.0, y: 0.0, fields: defaults, defaults, pan: None, lerps: [None; FIELD_COUNT] }
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
    }

    pub fn panning(&self) -> bool {
        self.pan.is_some()
    }

    pub fn update(&mut self, dt: f32) {
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
