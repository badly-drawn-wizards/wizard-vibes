// The orbit camera (web/camera.ts), turned by dragging the view and zoomed
// by the wheel; kept between runs.

use super::vec::V3;
use crate::types::{Camera, PAGE_CAMERA};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    pub cam: Camera,
    // touched by the user: no more idle orbiting
    pub interacted: bool,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { cam: PAGE_CAMERA, interacted: false }
    }
}

const PITCH_MIN: f32 = -0.9; // (looking up: see camera_ray in wizard.rs)
const PITCH_MAX: f32 = 1.2;

impl Orbit {
    // As restored from storage (it counts as touched).
    pub fn restored(cam: Camera) -> Self {
        Orbit { cam, interacted: true }
    }

    // A drag over the view by (dx, dy) points.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.interacted = true;
        self.cam.yaw -= dx * 0.006;
        self.cam.pitch = (self.cam.pitch + dy * 0.006).clamp(PITCH_MIN, PITCH_MAX);
    }

    // The wheel, `delta_y` as a browser's deltaY (pixels, positive away
    // from the user: zooms out).
    pub fn wheel(&mut self, delta_y: f32) {
        self.interacted = true;
        self.cam.dist = (self.cam.dist * (delta_y * 0.001).exp()).clamp(2.5, 12.0);
    }

    // Zooms by `amount` (positive out: the distance grows by e^amount), e.g.
    // from a controller's D-pad held.
    pub fn zoom(&mut self, amount: f32) {
        if amount == 0.0 {
            return;
        }
        self.interacted = true;
        self.cam.dist = (self.cam.dist * amount.exp()).clamp(2.5, 12.0);
    }

    // Turns the camera by (dyaw, dpitch), e.g. from a controller's stick.
    pub fn nudge(&mut self, dyaw: f32, dpitch: f32) {
        if dyaw == 0.0 && dpitch == 0.0 {
            return;
        }
        self.interacted = true;
        self.cam.yaw += dyaw;
        self.cam.pitch = (self.cam.pitch + dpitch).clamp(PITCH_MIN, PITCH_MAX);
    }

    // Slow idle orbit until the user touches the camera: one revolution per
    // three 12 s animation loops, so the idle view loops too.
    pub fn idle(&mut self, dt: f32) {
        if !self.interacted {
            self.cam.yaw += dt * 2.0 * PI / 36.0;
        }
    }

    // The way the camera looks, in his frame (as camera_ray in fut/wizard.fut).
    pub fn view_dir(&self) -> V3 {
        let (sp, cp) = self.cam.pitch.sin_cos();
        let (sy, cy) = self.cam.yaw.sin_cos();
        [-cp * sy, -sp, -cp * cy]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits() {
        let mut o = Orbit::default();
        o.idle(1.0);
        assert!((o.cam.yaw - (0.5 + PI / 18.0)).abs() < 1e-6);
        o.drag(0.0, 1000.0);
        assert_eq!(o.cam.pitch, PITCH_MAX);
        assert!(o.interacted);
        o.wheel(1e5);
        assert_eq!(o.cam.dist, 12.0);
        let y = o.cam.yaw;
        o.idle(1.0);
        assert_eq!(o.cam.yaw, y);
        let d = o.view_dir();
        assert!((super::super::vec::len(d) - 1.0).abs() < 1e-5);
    }
}
