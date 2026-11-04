// fut/hash.fut: integer hashes -> [0, 1).

#[inline]
pub fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let h = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791);
    let h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    let h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    let h = h ^ (h >> 16);
    (h >> 8) as f32 / 16777216.0
}

#[inline]
pub fn hash2(x: i32, y: i32) -> f32 {
    hash3(x, y, 1337)
}
