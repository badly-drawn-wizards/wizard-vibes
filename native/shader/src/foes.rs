// fut/foes.fut: the enemies' states, from the harness, and the arena.

use crate::prelude::*;

#[derive(Clone, Copy)]
pub struct Foes {
    pub states: (f32, f32, f32, f32, f32, f32),
    pub arena: Vec3,
}

#[inline]
pub fn foe_state(fs: Foes, i: i32) -> f32 {
    let s = fs.states;
    if i == 0 {
        s.0
    } else if i == 1 {
        s.1
    } else if i == 2 {
        s.2
    } else if i == 3 {
        s.3
    } else if i == 4 {
        s.4
    } else {
        s.5
    }
}
