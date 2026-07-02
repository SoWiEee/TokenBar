//! Orbit camera for the 3D graph: azimuth/elevation around a target, with zoom.
//! Driven by GTK drag + scroll gestures; the renderer only consumes the final
//! view-projection matrix it produces.

use glam::{vec3, Mat4, Vec3};

pub struct Orbit {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: Vec3,
}

/// Keep the camera from flipping over the poles.
const ELEVATION_MIN: f32 = 0.12;
const ELEVATION_MAX: f32 = 1.50;
const DISTANCE_MIN: f32 = 3.0;
const DISTANCE_MAX: f32 = 400.0;

impl Orbit {
    /// A 3/4 top-down framing scaled to a grid of half-width `extent`.
    pub fn framing(extent: f32) -> Self {
        Self {
            azimuth: -0.55,
            elevation: 0.82,
            distance: (extent * 1.7).max(DISTANCE_MIN),
            target: vec3(0.0, 0.4, 0.0),
        }
    }

    fn eye(&self) -> Vec3 {
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        self.target + self.distance * vec3(ce * sa, se, ce * ca)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let far = self.distance * 4.0 + 50.0;
        let proj = Mat4::perspective_rh_gl(45f32.to_radians(), aspect.max(0.01), 0.1, far);
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        proj * view
    }

    /// Set absolute orientation (used by drag: start value + gesture offset),
    /// clamping elevation.
    pub fn set_orbit(&mut self, azimuth: f32, elevation: f32) {
        self.azimuth = azimuth;
        self.elevation = elevation.clamp(ELEVATION_MIN, ELEVATION_MAX);
    }

    /// Multiply the distance (scroll zoom), clamped.
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(DISTANCE_MIN, DISTANCE_MAX);
    }
}
