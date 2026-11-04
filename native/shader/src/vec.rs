// fut/vec3.fut: vectors are glam's (Vec3 for vec3, Vec2 for vec2), with
// Futhark's helpers under their own names and argument orders; +^ -^ *^
// are + - *. And the scalar helpers Futhark's prelude has and Rust's does
// not quite: sgn (0 at 0), floor division and modulo for i32.

use crate::prelude::*;

#[inline]
pub fn vec(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}
#[inline]
pub fn splat(s: f32) -> Vec3 {
    Vec3::splat(s)
}
#[inline]
pub fn scale(s: f32, v: Vec3) -> Vec3 {
    v * s
}
#[inline]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
#[inline]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    vec(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}
#[inline]
pub fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}
#[inline]
pub fn normalise(v: Vec3) -> Vec3 {
    scale(1.0 / length(v), v)
}
#[inline]
pub fn vmap(f: impl Fn(f32) -> f32, v: Vec3) -> Vec3 {
    vec(f(v.x), f(v.y), f(v.z))
}
#[inline]
pub fn vabs(v: Vec3) -> Vec3 {
    vec(v.x.abs(), v.y.abs(), v.z.abs())
}
#[inline]
pub fn vmaxs(v: Vec3, s: f32) -> Vec3 {
    vec(s.maximum(v.x), s.maximum(v.y), s.maximum(v.z))
}
#[inline]
pub fn maxc(v: Vec3) -> f32 {
    v.x.maximum(v.y.maximum(v.z))
}
#[inline]
pub fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + scale(t, b - a)
}
#[inline]
pub fn reflect(i: Vec3, n: Vec3) -> Vec3 {
    i - scale(2.0 * dot(n, i), n)
}
#[inline]
pub fn length2(v: Vec2) -> f32 {
    (v.x * v.x + v.y * v.y).sqrt()
}
#[inline]
pub fn rot2(a: f32, p: Vec2) -> Vec2 {
    let c = a.cos();
    let s = a.sin();
    vec2(c * p.x - s * p.y, s * p.x + c * p.y)
}
#[inline]
pub fn clamp(lo: f32, hi: f32, x: f32) -> f32 {
    lo.maximum(hi.minimum(x))
}
#[inline]
pub fn saturate(x: f32) -> f32 {
    clamp(0.0, 1.0, x)
}
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = saturate((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}
#[inline]
pub fn fract(x: f32) -> f32 {
    x - x.floor()
}
#[inline]
pub fn mix1(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
#[inline]
pub fn pow(x: f32, y: f32) -> f32 {
    x.powf(y)
}

// f32.sgn: -1, 0 or 1 (Rust's signum is 1 at +0).
#[inline]
pub fn sgn(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

// i32 `/` and `%`, truncating as Rust's do, but unchecked (as WGSL's):
// Rust checks for zero and overflow even in release builds, and rust-gpu
// inlines every function that may panic, #[inline(never)] or not.
#[inline]
pub fn sdiv(a: i32, b: i32) -> i32 {
    #[cfg(target_arch = "spirv")]
    unsafe {
        let mut r = 0i32;
        core::arch::asm!("%r = OpSDiv typeof*{r} {a} {b}", "OpStore {r} %r", r = in(reg) &mut r, a = in(reg) a, b = in(reg) b);
        r
    }
    #[cfg(not(target_arch = "spirv"))]
    a.wrapping_div(b)
}
#[inline]
pub fn srem(a: i32, b: i32) -> i32 {
    #[cfg(target_arch = "spirv")]
    unsafe {
        let mut r = 0i32;
        core::arch::asm!("%r = OpSRem typeof*{r} {a} {b}", "OpStore {r} %r", r = in(reg) &mut r, a = in(reg) a, b = in(reg) b);
        r
    }
    #[cfg(not(target_arch = "spirv"))]
    a.wrapping_rem(b)
}

// i32.f32: truncating, saturating as WGSL's is (a clamp, then the
// conversion, as naga compiles it); Rust's `as` adds a branch for NaN.
#[inline]
pub fn ftoi(x: f32) -> i32 {
    let c = x.maximum(-2147483648.0).minimum(2147483520.0);
    #[cfg(target_arch = "spirv")]
    unsafe {
        let mut r = 0i32;
        core::arch::asm!("%r = OpConvertFToS typeof*{r} {c}", "OpStore {r} %r", r = in(reg) &mut r, c = in(reg) c);
        r
    }
    #[cfg(not(target_arch = "spirv"))]
    {
        c as i32
    }
}

// Futhark's i32 `/` and `%` round towards minus infinity (`//` and `%%`
// truncate: sdiv and srem).
#[inline]
pub fn idiv(a: i32, b: i32) -> i32 {
    let q = sdiv(a, b);
    let r = srem(a, b);
    if r != 0 && ((r < 0) != (b < 0)) { q - 1 } else { q }
}
#[inline]
pub fn imod(a: i32, b: i32) -> i32 {
    let r = srem(a, b);
    if r == 0 || (a > 0 && b > 0) || (a < 0 && b < 0) { r } else { r + b }
}

pub const PI: f32 = core::f32::consts::PI;
pub const INF: f32 = f32::INFINITY;

#[inline]
pub fn isinf(x: f32) -> bool {
    x.is_infinite()
}

// f32.round: to the nearest, ties to even (Rust's round goes away from 0).
#[inline]
pub fn round_even(x: f32) -> f32 {
    let r = x.floor();
    let d = x - r;
    if d > 0.5 {
        r + 1.0
    } else if d < 0.5 {
        r
    } else if fract(r * 0.5) == 0.0 {
        r
    } else {
        r + 1.0
    }
}
