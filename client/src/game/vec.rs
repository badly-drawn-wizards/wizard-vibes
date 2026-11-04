// Small 3-vector helpers (world space: y up), as web/vec.ts.

pub type V3 = [f32; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(s: f32, a: V3) -> V3 {
    [s * a[0], s * a[1], s * a[2]]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn dist(a: V3, b: V3) -> f32 {
    len(sub(a, b))
}
// (a zero vector stays zero)
pub fn normalise(a: V3) -> V3 {
    let l = len(a);
    scale(1.0 / if l == 0.0 { 1.0 } else { l }, a)
}
// A direction turned by `a` about the vertical (yaw in fut/world.fut).
pub fn turn(a: f32, [x, y, z]: V3) -> V3 {
    let (s, c) = a.sin_cos();
    [x * c + z * s, y, z * c - x * s]
}
