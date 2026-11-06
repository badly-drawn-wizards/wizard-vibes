// The orbit camera (web/camera.ts), turned by the mouse (taken: see
// look.rs) and zoomed by the wheel; kept between runs.

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
const LOOK: f32 = 0.0035; // radians the mouse turns the view a pixel

impl Orbit {
    // As restored from storage (it counts as touched).
    pub fn restored(cam: Camera) -> Self {
        Orbit { cam, interacted: true }
    }

    // The mouse moved by (dx, dy) (pixels, or the device's units): right
    // turns the view right, up raises it.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.interacted = true;
        self.cam.yaw -= dx * LOOK;
        self.cam.pitch = (self.cam.pitch + dy * LOOK).clamp(PITCH_MIN, PITCH_MAX);
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

    // Where the camera is, in his frame, `shoulder` (0..1) over his right
    // shoulder (as camera_ray in the shader's wizard.rs).
    pub fn eye(&self, shoulder: f32) -> V3 {
        let (ro, _) = self.centre_ray(shoulder);
        [ro.x, ro.y, ro.z]
    }

    // The way the camera looks, in his frame.
    pub fn view_dir(&self) -> V3 {
        let (_, rd) = self.centre_ray(0.0);
        [rd.x, rd.y, rd.z]
    }

    // The ray through the view's centre (the renderer's camera_ray: kept
    // above his platform, as it is outside picture mode).
    fn centre_ray(&self, shoulder: f32) -> (wizard_shader::prelude::Vec3, wizard_shader::prelude::Vec3) {
        use wizard_shader::wizard::{camera_ray, default_target};
        camera_ray(default_target, 2, 2, self.cam.yaw, self.cam.pitch, self.cam.dist, shoulder, 0.1, 1.0, 1.0)
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
        o.look(0.0, 1000.0);
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
