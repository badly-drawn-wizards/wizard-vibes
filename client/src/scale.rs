// The render scale (the fraction of the view's pixels the scene is
// rendered at): set by hand with a slider, or dynamic, kept as high as
// holds the target frame rate (a setting: 60 frames a second at first).
//
// Dynamic: the frame time, smoothed, is looked at every few tenths of a
// second. Too slow, and the scale drops towards what would make the
// target (the cost goes with the pixels, the square of the scale); at the
// target, it creeps up a step at a time (a display's rate caps the frame
// rate, hiding any room there is); well under it, it goes most of the way
// there at once. Each change is judged by its own frames: the average starts again. A scale that proved too slow is a ceiling for a while, so that a
// display's vsync (which halves the rate the moment a frame misses) does
// not set it sawing up and down; scales are in steps of 1/32, so the
// render targets are not remade every frame.

pub const MIN: f32 = 0.25;
pub const MAX: f32 = 1.0;
const STEP: f32 = 1.0 / 32.0;
pub const TARGET_FPS: f32 = 60.0;
pub const TARGET_MIN: f32 = 30.0;
pub const TARGET_MAX: f32 = 144.0;
const SLOW: f32 = 1.12; // of the target: too slow
const FAST: f32 = 1.04; // of the target: at it (or faster)
const SMOOTH: f32 = 0.25; // the frame time's smoothing, in seconds
const EVERY: f32 = 0.3; // seconds between looks
const CEILING: f32 = 10.0; // seconds a too-slow scale is not gone back to
const HITCH: f32 = 0.1; // a frame this long is a hitch (loading, a tab switch), not counted

#[derive(Clone, Debug)]
pub struct RenderScale {
    pub dynamic: bool,
    // the frame rate dynamic scaling keeps
    pub target_fps: f32,
    // the slider's value, when set by hand
    pub manual: f32,
    current: f32,
    frame_time: f32,
    until_look: f32,
    // the lowest scale found too slow lately, and how long it stays so
    ceiling: f32,
    ceiling_for: f32,
}

fn snap(s: f32) -> f32 {
    ((s / STEP).round() * STEP).clamp(MIN, MAX)
}

impl RenderScale {
    pub fn new(dynamic: bool, manual: f32) -> RenderScale {
        let manual = snap(manual);
        RenderScale {
            dynamic,
            target_fps: TARGET_FPS,
            manual,
            current: if dynamic { 0.5 } else { manual },
            frame_time: 1.0 / TARGET_FPS,
            until_look: EVERY,
            ceiling: MAX + STEP,
            ceiling_for: 0.0,
        }
    }

    // The scale in force.
    pub fn get(&self) -> f32 {
        if self.dynamic { self.current } else { self.manual }
    }

    // After a frame that took `dt` seconds; `counting` false while the
    // frame rate says nothing (paused, hidden, a shader loading).
    pub fn frame(&mut self, dt: f32, counting: bool) {
        if !self.dynamic || !counting || dt <= 0.0 || dt >= HITCH {
            return;
        }
        let target = 1.0 / self.target_fps.clamp(TARGET_MIN, TARGET_MAX);
        self.frame_time += (dt - self.frame_time) * (1.0 - (-dt / SMOOTH).exp());
        self.ceiling_for -= dt;
        if self.ceiling_for <= 0.0 {
            self.ceiling = MAX + STEP;
        }
        self.until_look -= dt;
        if self.until_look > 0.0 {
            return;
        }
        self.until_look = EVERY;
        let ft = self.frame_time;
        let was = self.current;
        if ft > target * SLOW {
            self.ceiling = self.ceiling.min(self.current);
            self.ceiling_for = CEILING;
            // (towards the scale that would make the target, a fifth of the
            // way down at most per look)
            let want = self.current * (target / ft).sqrt();
            self.current = (((want.max(0.8 * self.current)) / STEP).floor() * STEP).clamp(MIN, MAX);
        } else if ft < target * FAST && self.current + STEP < self.ceiling {
            // (well under the target, towards the scale that would just make
            // it, with a margin; at it, where a display's rate hides any room
            // there is, a step)
            let want = self.current * (0.85 * target / ft).sqrt();
            let below = self.ceiling - STEP;
            self.current = snap((((want / STEP).floor() * STEP).max(self.current + STEP)).min(below));
        }
        // (changed: judge the new scale by its own frames, not the old's)
        if self.current != was {
            self.frame_time = target;
            self.until_look = 2.0 * EVERY;
        }
    }

    // The slider moved: by hand from now on.
    pub fn set_manual(&mut self, s: f32) {
        self.manual = snap(s);
        self.dynamic = false;
    }

    pub fn set_dynamic(&mut self, on: bool) {
        if on && !self.dynamic {
            // (from where the slider was)
            self.current = self.manual;
            self.ceiling = MAX + STEP;
            self.frame_time = 1.0 / self.target_fps;
        }
        self.dynamic = on;
    }

    // A new target frame rate (judged afresh).
    pub fn set_target(&mut self, fps: f32) {
        self.target_fps = fps.clamp(TARGET_MIN, TARGET_MAX).round();
        self.ceiling = MAX + STEP;
        self.frame_time = 1.0 / self.target_fps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A GPU taking `full` seconds a frame at scale 1 (the cost going with
    // the pixels), under a display of `hz` (frames wait for its next
    // refresh), for `secs`.
    fn run(r: &mut RenderScale, full: f32, hz: f32, secs: f32) {
        let mut t = 0.0;
        while t < secs {
            let s = r.get();
            let gpu = full * s * s;
            let dt = (gpu * hz).ceil().max(1.0) / hz;
            r.frame(dt, true);
            t += dt;
        }
    }

    #[test]
    fn drops_to_hold_the_target() {
        // three times too slow at full size: about 1/sqrt(3) holds 60 fps
        let mut r = RenderScale::new(true, 1.0);
        run(&mut r, 3.0 / 60.0, 60.0, 30.0);
        let s = r.get();
        assert!((0.45..=0.58).contains(&s), "{s}");
        assert!(3.0 / 60.0 * s * s <= 1.0 / 60.0);
    }

    #[test]
    fn rises_to_full_when_it_can() {
        let mut r = RenderScale::new(true, 0.25);
        run(&mut r, 0.5 / 60.0, 60.0, 20.0);
        assert_eq!(r.get(), 1.0);
    }

    #[test]
    fn rises_fast_with_room_to_see() {
        // uncapped (a display far faster than the GPU needs): full size in
        // a couple of seconds, not a step at a time
        let mut r = RenderScale::new(true, 0.25);
        run(&mut r, 0.4 / 60.0, 1000.0, 2.0);
        assert_eq!(r.get(), 1.0);
    }

    #[test]
    fn does_not_saw_at_a_vsync_edge() {
        // full size takes 1.6 frames: the most that holds is ~0.79; once
        // settled, few changes
        let mut r = RenderScale::new(true, 1.0);
        run(&mut r, 1.6 / 60.0, 60.0, 10.0);
        let mut seen = vec![];
        let mut t = 0.0;
        while t < 9.0 {
            let s = r.get();
            let dt = ((1.6 * s * s * 60.0f32).ceil().max(1.0)) / 60.0;
            r.frame(dt, true);
            t += dt;
            if seen.last() != Some(&r.get()) {
                seen.push(r.get());
            }
        }
        assert!(seen.len() <= 3, "{seen:?}");
        assert!(seen.iter().all(|s| 1.6 * s * s <= 1.0 + 1e-3 || seen.len() <= 2), "{seen:?}");
    }

    #[test]
    fn by_hand() {
        let mut r = RenderScale::new(true, 1.0);
        r.set_manual(0.4);
        assert!(!r.dynamic);
        assert_eq!(r.get(), 0.40625); // (snapped to 1/32)
        run(&mut r, 3.0 / 60.0, 60.0, 5.0);
        assert_eq!(r.get(), 0.40625);
    }
}
