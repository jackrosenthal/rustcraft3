//! The game camera: a target point viewed from a distance at an angle of attack.

use glam::{Mat4, Quat, Vec3, Vec4};

/// Vertical position of the camera target in normalized device coordinates (fitted to
/// the original at camera distances of 1650 and 2500 and angles of 304 and 320).
pub const TARGET_SCREEN_OFFSET: f32 = 0.18;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    /// Degrees; 270 looks straight down, 304 is the default game view.
    pub angle_of_attack: f32,
    /// Degrees around the z axis; 90 looks toward +y.
    pub rotation: f32,
    pub distance: f32,
    /// Field of view in degrees, measured across the diagonal of a 4:3 screen.  Wider
    /// screens keep the 4:3 vertical extent and see more to the sides.
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub roll: f32,
    /// Where the eye is moved from its place behind the target (camera noise).
    pub eye_offset: Vec3,
}

impl Default for Camera {
    fn default() -> Self {
        // UI\MiscData.txt [Camera], first zoom level.
        Camera {
            target: Vec3::ZERO,
            angle_of_attack: 304.0,
            rotation: 90.0,
            distance: 1650.0,
            fov: 70.0,
            near: 100.0,
            far: 5000.0,
            roll: 0.0,
            eye_offset: Vec3::ZERO,
        }
    }
}

impl Camera {
    pub fn direction(&self) -> Vec3 {
        let (a, r) = (self.angle_of_attack.to_radians(), self.rotation.to_radians());
        Vec3::new(r.cos() * a.cos(), r.sin() * a.cos(), a.sin())
    }

    pub fn eye(&self) -> Vec3 {
        self.target - self.direction() * self.distance + self.eye_offset
    }

    pub fn view(&self) -> Mat4 {
        let dir = (self.target - self.eye()).normalize_or(self.direction());
        let up = Quat::from_axis_angle(dir, self.roll.to_radians()) * Vec3::Z;
        Mat4::look_to_rh(self.eye(), dir, up)
    }

    /// Measured against frames captured from the original 1.27b renderer at 4:3: the
    /// vertical extent matches a 70 degree field of view across the
    /// diagonal of a 4:3 screen, and the camera target projects 0.18 of the half-height
    /// above the screen center.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let half_h = (self.fov.to_radians() / 2.0).tan() * 0.6;
        let fov_y = 2.0 * half_h.atan();
        // Allow zooming out further than the default far plane without clipping the ground.
        let far = self.far.max(self.distance * 3.0);
        Mat4::from_translation(Vec3::new(0.0, TARGET_SCREEN_OFFSET, 0.0)) * Mat4::perspective_rh(fov_y, aspect, self.near, far)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// The view and projection with the far plane at least `far` away (the sky, which
    /// the camera's far clip does not cut).
    pub fn view_proj_beyond(&self, aspect: f32, far: f32) -> Mat4 {
        let half_h = (self.fov.to_radians() / 2.0).tan() * 0.6;
        let far = self.far.max(self.distance * 3.0).max(far);
        Mat4::from_translation(Vec3::new(0.0, TARGET_SCREEN_OFFSET, 0.0)) * Mat4::perspective_rh(2.0 * half_h.atan(), aspect, self.near, far) * self.view()
    }

    /// Rotation that turns model-space +x toward the camera, for billboards.
    pub fn billboard_rotation(&self) -> Quat {
        let inv = self.view().inverse();
        let (_, r, _) = inv.to_scale_rotation_translation();
        // View space looks down -z; billboards in MDX face +x.
        r * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
    }

    /// World-space ray through a point in normalized device coordinates.
    pub fn ray(&self, aspect: f32, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let a = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let b = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (a, (b - a).normalize())
    }
}

/// A view's six clipping planes, each facing inward, for telling what lies outside it.
#[derive(Clone, Copy, Debug)]
pub struct Frustum([Vec4; 6]);

impl Frustum {
    /// The planes of a view and projection (wgpu's clip space: depth from 0 to 1).
    pub fn new(view_proj: Mat4) -> Frustum {
        let (x, y, z, w) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
        Frustum([w + x, w - x, w + y, w - y, z, w - z].map(|p| p / p.truncate().length().max(f32::MIN_POSITIVE)))
    }

    /// Whether any of the sphere at `center` lies within the view.
    pub fn sees(&self, center: Vec3, radius: f32) -> bool {
        self.0.iter().all(|p| p.truncate().dot(center) + p.w >= -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frustum_sees_what_the_camera_looks_at_and_not_far_aside() {
        let cam = Camera { target: Vec3::new(1000.0, 2000.0, 0.0), ..Camera::default() };
        let f = Frustum::new(cam.view_proj(16.0 / 9.0));
        assert!(f.sees(cam.target, 1.0));
        // Well to either side, and behind the eye.
        assert!(!f.sees(cam.target + Vec3::new(20_000.0, 0.0, 0.0), 100.0));
        assert!(!f.sees(cam.target - Vec3::new(20_000.0, 0.0, 0.0), 100.0));
        assert!(!f.sees(cam.eye() + (cam.eye() - cam.target), 10.0));
        // A sphere big enough reaches back in.
        assert!(f.sees(cam.target + Vec3::new(20_000.0, 0.0, 0.0), 30_000.0));
    }
}
