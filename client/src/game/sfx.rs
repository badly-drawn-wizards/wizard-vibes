// Where sound effects are heard from (web/sfx.ts, combat.ts's placed()):
// panned by which side of the view they are on, quieter further from him.
// The kinds as fut/sfx.fut numbers them (web/sfx.ts's SFX). Playing them is
// the audio's business.

use super::vec::{dot, len, sub, V3};
use crate::types::SfxEvent;

pub const CAST_FIRE: usize = 0;
pub const CAST_STRIKE: usize = 1;
pub const CAST_WIND: usize = 2;
pub const BURST: usize = 3;
pub const THUNDER: usize = 4;
pub const GUST: usize = 5;
pub const HURT: usize = 6;
pub const DEFEAT: usize = 7;
pub const SHIELD_UP: usize = 8;
pub const BLOCK: usize = 9;
pub const SHIELD_DOWN: usize = 10;
pub const FIZZLE: usize = 11;
pub const MARK: usize = 12;
pub const APPEAR: usize = 13;

// An effect heard as if beside him: in the middle, at full volume.
pub fn here(kind: usize) -> SfxEvent {
    SfxEvent { kind, pan: 0.0, gain: 1.0 }
}

// An effect at world point `pos`, heard by him at `him` looking along `dir`
// (pan clamped to -1..1, as the stereo panner does).
pub fn placed(kind: usize, pos: V3, him: V3, dir: V3) -> SfxEvent {
    let v = sub(pos, him);
    let d = nonzero(len(v));
    let right: V3 = [-dir[2], 0.0, dir[0]];
    let pan = 0.8 * dot(v, right) / (nonzero(len(right)) * d);
    SfxEvent { kind, pan: pan.clamp(-1.0, 1.0), gain: 1.0 / (1.0 + d / 15.0) }
}

fn nonzero(x: f32) -> f32 {
    if x == 0.0 { 1.0 } else { x }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement() {
        // looking along +z, his right is -x (the camera's right in this
        // world, as flight.ts has it: [-fwd.z, 0, fwd.x])
        let e = placed(BURST, [-15.0, 0.0, 0.0], [0.0; 3], [0.0, 0.0, 1.0]);
        assert!((e.pan - 0.8).abs() < 1e-5);
        assert!((e.gain - 0.5).abs() < 1e-5);
        let e = placed(BURST, [0.0, 0.0, 30.0], [0.0; 3], [0.0, 0.0, 1.0]);
        assert!(e.pan.abs() < 1e-5);
        assert!((e.gain - 1.0 / 3.0).abs() < 1e-5);
    }
}
