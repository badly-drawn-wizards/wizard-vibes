// fut/anim.fut: animation timing. The animation loops every loop_seconds;
// inside the renderer `tm` is the loop angle in [0, 2pi).

use crate::prelude::*;

pub const loop_seconds: f32 = 12.0;

#[inline]
pub fn loop_angle(seconds: f32) -> f32 {
    2.0 * PI * fract(seconds / loop_seconds)
}

// The rune circle pulses gently, three times per loop, by amp.
#[inline]
pub fn rune_pulse(amp: f32, tm: f32) -> f32 {
    1.0 - amp + amp * (3.0 * tm).sin()
}
