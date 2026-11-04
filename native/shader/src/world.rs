// fut/world.fut: the world the wizard flies over: the flight path, the
// frame that maps the wizard's local space (where he and his rock stay at
// the origin, facing +z) into the world, and the terrain below.

use crate::prelude::*;

// ---------------------------------------------------------------------
// Flight

// The path is closed-form: x wanders as a sum of sines while z advances at
// a steady speed, so heading, turn rate and bank are exact at any time.
pub const forward_speed: f32 = 3.0;
pub const altitude: f32 = 12.0; // world height of the rock's flat top

#[inline]
pub fn path_x(s: f32) -> f32 {
    14.0 * (0.12 * s).sin() + 6.0 * (0.27 * s + 1.0).sin()
}
#[inline]
pub fn path_dx(s: f32) -> f32 {
    14.0 * 0.12 * (0.12 * s).cos() + 6.0 * 0.27 * (0.27 * s + 1.0).cos()
}
#[inline]
pub fn path_ddx(s: f32) -> f32 {
    -14.0 * 0.0144 * (0.12 * s).sin() - 6.0 * 0.0729 * (0.27 * s + 1.0).sin()
}

// heading: radians from world +z towards +x; turn: its rate (rad/s), up to
// about 0.2; bank: roll about the direction of travel, into the turn.
#[derive(Clone, Copy)]
pub struct Flight {
    pub pos: Vec3,
    pub heading: f32,
    pub turn: f32,
    pub bank: f32,
}

pub fn flight_at(bank_gain: f32, s: f32) -> Flight {
    let dx = path_dx(s);
    let heading = dx.atan2(forward_speed);
    let turn = forward_speed * path_ddx(s) / (dx * dx + forward_speed * forward_speed);
    Flight { pos: vec(path_x(s), altitude, forward_speed * s), heading, turn, bank: clamp(-0.35, 0.35, bank_gain * turn) }
}

#[inline]
pub fn yaw(a: f32, v: Vec3) -> Vec3 {
    vec(v.x * a.cos() + v.z * a.sin(), v.y, v.z * a.cos() - v.x * a.sin())
}

#[inline]
pub fn yaw_inv(a: f32, v: Vec3) -> Vec3 {
    yaw(-a, v)
}

#[inline]
pub fn roll(a: f32, v: Vec3) -> Vec3 {
    vec(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos(), v.z)
}

// Local -> world. Banking into a turn towards +x tips the local +x side down.
#[inline]
pub fn to_world_dir(f: Flight, d: Vec3) -> Vec3 {
    yaw(f.heading, roll(-f.bank, d))
}
#[inline]
pub fn to_world(f: Flight, p: Vec3) -> Vec3 {
    f.pos + to_world_dir(f, p)
}
#[inline]
pub fn to_local_dir(f: Flight, d: Vec3) -> Vec3 {
    roll(f.bank, yaw(-f.heading, d))
}

// The moon's direction in the world, from its azimuth (degrees from +z
// towards +x) and elevation.
#[inline]
pub fn moon_dir(azimuth: f32, elevation: f32) -> Vec3 {
    let a = azimuth * PI / 180.0;
    let e = elevation * PI / 180.0;
    vec(e.cos() * a.sin(), e.sin(), e.cos() * a.cos())
}

// ---------------------------------------------------------------------
// Noise

// Value noise in [0, 1], smoothly interpolated.
pub fn vnoise2(x: f32, z: f32) -> f32 {
    let ix = ftoi(x.floor());
    let iz = ftoi(z.floor());
    let fx = fract(x);
    let fz = fract(z);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let a = hash2(ix, iz);
    let b = hash2(ix + 1, iz);
    let c = hash2(ix, iz + 1);
    let d = hash2(ix + 1, iz + 1);
    a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz
}

pub fn vnoise3(p: Vec3) -> f32 {
    let i = vmap(|t| t.floor(), p);
    let f = p - i;
    let u = vmap(|t| t * t * (3.0 - 2.0 * t), f);
    let h = move |dx: i32, dy: i32, dz: i32| hash3(ftoi(i.x) + dx, ftoi(i.y) + dy, ftoi(i.z) + dz);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(
        lerp(lerp(h(0, 0, 0), h(1, 0, 0), u.x), lerp(h(0, 1, 0), h(1, 1, 0), u.x), u.y),
        lerp(lerp(h(0, 0, 1), h(1, 0, 1), u.x), lerp(h(0, 1, 1), h(1, 1, 1), u.x), u.y),
        u.z,
    )
}

// Fractal sum of `octaves` octaves of 2D value noise, rotating each octave
// to hide the lattice; result in [0, 1]. `detail` (fractional) says how
// many octaves to actually evaluate: the rest are faded out smoothly to
// their average, so lowering it removes fine detail without moving the mean.
pub fn fbm2_lod(octaves: i32, detail: f32, x: f32, z: f32) -> f32 {
    let (mut sum, mut norm, mut amp, mut x, mut z) = (0f32, 0f32, 0.5f32, x, z);
    let mut i = 0;
    while i < octaves {
        let k = saturate(detail - i as f32);
        let v = if k > 0.0 { mix1(0.5, vnoise2(x, z), k) } else { 0.5 };
        let sum_n = sum + amp * v;
        let norm_n = norm + amp;
        let amp_n = amp * 0.5;
        let x_n = 1.6 * x - 1.2 * z + 17.1;
        let z_n = 1.2 * x + 1.6 * z - 9.3;
        sum = sum_n;
        norm = norm_n;
        amp = amp_n;
        x = x_n;
        z = z_n;
        i += 1;
    }
    sum / norm
}

// vnoise2 with its gradient: (value, d/dx, d/dz). The smoothstep weight
// u = f^2 (3 - 2f) has derivative 6 f (1 - f).
pub fn vnoise2_grad(x: f32, z: f32) -> (f32, f32, f32) {
    let ix = ftoi(x.floor());
    let iz = ftoi(z.floor());
    let fx = fract(x);
    let fz = fract(z);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let dux = 6.0 * fx * (1.0 - fx);
    let duz = 6.0 * fz * (1.0 - fz);
    let a = hash2(ix, iz);
    let b = hash2(ix + 1, iz);
    let c = hash2(ix, iz + 1);
    let d = hash2(ix + 1, iz + 1);
    let k = a - b - c + d;
    (a + (b - a) * ux + (c - a) * uz + k * ux * uz, dux * (b - a + k * uz), duz * (c - a + k * ux))
}

// fbm2_lod with its gradient with respect to the *input* coordinates. Each
// octave's coordinates are a linear map of the input (the rotation-scale
// [1.6 -1.2; 1.2 1.6] applied i times), so its gradient is mapped back by
// that matrix's transpose; the matrix is tracked as (j00, j01, j10, j11).
pub fn fbm2_lod_grad(octaves: i32, detail: f32, x: f32, z: f32) -> (f32, f32, f32) {
    let (mut sum, mut gx, mut gz, mut norm, mut amp, mut x, mut z, mut j00, mut j01, mut j10, mut j11) =
        (0f32, 0f32, 0f32, 0f32, 0.5f32, x, z, 1f32, 0f32, 0f32, 1f32);
    let mut i = 0;
    while i < octaves {
        let k = saturate(detail - i as f32);
        let (v, dx, dz) = if k > 0.0 { vnoise2_grad(x, z) } else { (0.5, 0.0, 0.0) };
        // faded octave: 0.5 + k (v - 0.5)
        let w = amp * k;
        let sum_n = sum + amp * (0.5 + k * (v - 0.5));
        let gx_n = gx + w * (dx * j00 + dz * j10);
        let gz_n = gz + w * (dx * j01 + dz * j11);
        let norm_n = norm + amp;
        let amp_n = amp * 0.5;
        let x_n = 1.6 * x - 1.2 * z + 17.1;
        let z_n = 1.2 * x + 1.6 * z - 9.3;
        let j00_n = 1.6 * j00 - 1.2 * j10;
        let j01_n = 1.6 * j01 - 1.2 * j11;
        let j10_n = 1.2 * j00 + 1.6 * j10;
        let j11_n = 1.2 * j01 + 1.6 * j11;
        sum = sum_n;
        gx = gx_n;
        gz = gz_n;
        norm = norm_n;
        amp = amp_n;
        x = x_n;
        z = z_n;
        j00 = j00_n;
        j01 = j01_n;
        j10 = j10_n;
        j11 = j11_n;
        i += 1;
    }
    (sum / norm, gx / norm, gz / norm)
}

// Fractal 3D value noise, roughly in [0, 1].
pub fn fbm3(octaves: i32, p: Vec3) -> f32 {
    let (mut sum, mut norm, mut amp, mut q) = (0f32, 0f32, 0.5f32, p);
    let mut i = 0;
    while i < octaves {
        let sum_n = sum + amp * vnoise3(q);
        let norm_n = norm + amp;
        let amp_n = amp * 0.5;
        let q_n = vec(1.6 * q.x - 1.2 * q.z + 7.1, 2.0 * q.y + 3.3, 1.2 * q.x + 1.6 * q.z - 4.9);
        sum = sum_n;
        norm = norm_n;
        amp = amp_n;
        q = q_n;
        i += 1;
    }
    sum / norm
}

// ---------------------------------------------------------------------
// Clouds: sparse wisps in a slab of world heights just below the rock,
// drifting on the wind.

pub const cloud_bottom: f32 = 7.5;
pub const cloud_top: f32 = 10.0;
pub const wind: Vec3 = Vec3::new(0.6, 0.0, -0.9);

// Density (extinction per unit length) at a world point, and a distance
// that is safe to step without missing any cloud (0 inside one).
//
// fbm3 4 = (0.5 o1 + 0.25 o2 + 0.125 o3 + 0.0625 o4) / 0.9375, each octave
// in [0, 1], so the first octave alone bounds it from above. Where even that
// bound is below the coverage threshold the sky is clear, and the other
// octaves are skipped. Octave 1 changes by at most about 1.5 per lattice
// unit, i.e. 0.165 per world unit, so the margin tells how far is clear.
pub fn cloud_sample(cover: f32, density: f32, secs: f32, w: Vec3) -> (f32, f32) {
    let h = (w.y - cloud_bottom) / (cloud_top - cloud_bottom);
    let q = scale(0.11, w - scale(secs, wind));
    let threshold = 1.0 - cover - 0.14 * (4.0 * h * (1.0 - h)); // thinner at top and bottom
    let o1 = vnoise3(q);
    let upper = (0.5 * o1 + 0.4375) / 0.9375;
    if upper < threshold {
        (0.0, (threshold - upper) / (0.165 * 0.5 / 0.9375))
    } else {
        let n = fbm3(4, q);
        (density * 0f32.maximum(n - threshold), 0.0)
    }
}

// Vertical distance from a world height to the slab (0 inside).
#[inline]
pub fn cloud_gap(y: f32) -> f32 {
    0f32.maximum((cloud_bottom - y).maximum(y - cloud_top))
}

// ---------------------------------------------------------------------
// Terrain: rolling hills with ridged mountains, in world coordinates.

pub const terrain_max: f32 = 11.0; // 9 + 6 - 4, and the alpine cap: no height exceeds this

// ---------------------------------------------------------------------
// Biomes. One broad field b, a few hundred units across, sorts the land:
// desert (low dunes) where it is low, alpine (higher, steeper mountains)
// where it is high, and the hills and meadows between. Each biome's height
// is a function of the same main noise, so mountains run on from meadow
// into alpine and sink into dunes across the borders.

// smoothstep e0 e1 x with its derivative in x
#[inline]
pub fn smoothstep_d(e0: f32, e1: f32, x: f32) -> (f32, f32) {
    let t = saturate((x - e0) / (e1 - e0));
    (t * t * (3.0 - 2.0 * t), 6.0 * t * (1.0 - t) / (e1 - e0))
}

// The biome weights at (x, z): (desert, alpine), each in [0, 1] and never
// both above 0 (the meadow's is the rest), with their gradients:
// (wd, dwd/dx, dwd/dz, wa, dwa/dx, dwa/dz).
//
// b is two crossing waves, each bent by a third and fourth: evaluated at
// every step of the march, it has to be cheap.
// (shifted along the flight so it opens over meadow, with mountains ahead
// after a minute and desert after seven)
pub fn biome_field(x: f32, z0: f32) -> (f32, f32, f32) {
    // (b, db/dx, db/dz)
    let z = z0 + 2850.0;
    let w1 = 0.0047 * z - 0.0013 * x + 0.4;
    let w2 = 0.0051 * x + 0.0017 * z + 2.1;
    let (c1, c2) = (1.3 * w1.cos(), 1.1 * w2.cos());
    let u = 0.0062 * x + 0.0021 * z + 1.3 * w1.sin();
    let v = 0.0058 * z - 0.0024 * x + 1.1 * w2.sin();
    let (cu, cv) = (0.2 * u.cos(), 0.2 * v.cos());
    (
        0.5 + 0.2 * (u.sin() + v.sin()),
        cu * (0.0062 - 0.0013 * c1) + cv * (-0.0024 + 0.0051 * c2),
        cu * (0.0021 + 0.0047 * c1) + cv * (0.0058 + 0.0017 * c2),
    )
}

pub fn biome_grad(x: f32, z: f32) -> (f32, f32, f32, f32, f32, f32) {
    let (b, bx, bz) = biome_field(x, z);
    let (d, dd) = smoothstep_d(0.42, 0.32, b);
    let (a, da) = smoothstep_d(0.58, 0.68, b);
    (d, dd * bx, dd * bz, a, da * bx, da * bz)
}

pub fn biome(x: f32, z: f32) -> (f32, f32) {
    let b = biome_field(x, z).0;
    (smoothstep(0.42, 0.32, b), smoothstep(0.58, 0.68, b))
}

// The desert's height from the main noise n (in [0, 1]): low ground with
// dunes, long ridges bent by the noise. With its gradient from n's world-
// space gradient (nx, nz).
pub fn dune_height(x: f32, z: f32, n: f32, nx: f32, nz: f32) -> (f32, f32, f32) {
    let a = 0.3 * (0.8 * x + 0.6 * z) + 6.0 * n;
    let s = 0.45 * a.cos();
    (-0.5 + 3.0 * n + 0.45 * (1.0 + a.sin()), 3.0 * nx + s * (0.24 + 6.0 * nx), 3.0 * nz + s * (0.18 + 6.0 * nz))
}

// The alpine height from the meadow's: steeper, and higher, but eased
// under terrain_max (the flight path's rock hangs not far above it):
// above alpine_knee, h becomes knee + r tanh ((h - knee) / r).
pub const alpine_scale: f32 = 1.25;
pub const alpine_lift: f32 = 1.0;
pub const alpine_knee: f32 = 7.0;
pub fn alpine_height(hm: f32) -> (f32, f32) {
    // (height, d height / d hm)
    let h = alpine_scale * hm + alpine_lift;
    let r = terrain_max - alpine_knee;
    if h <= alpine_knee {
        (h, alpine_scale)
    } else {
        let t = ((h - alpine_knee) / r).tanh();
        (alpine_knee + r * t, alpine_scale * (1.0 - t * t))
    }
}

// Blends the meadow's height hm (with gradient) into the desert's hd and
// the alpine's by the weights (with gradients).
pub fn biome_blend(hm: f32, hmx: f32, hmz: f32, hd: f32, hdx: f32, hdz: f32, wd: f32, wdx: f32, wdz: f32, wa: f32, wax: f32, waz: f32) -> (f32, f32, f32) {
    let (ha, dha) = alpine_height(hm);
    let (ed, ea) = (hd - hm, ha - hm);
    let (ax, az) = ((dha - 1.0) * hmx, (dha - 1.0) * hmz);
    (
        hm + wd * ed + wa * ea,
        hmx + wd * (hdx - hmx) + wa * ax + ed * wdx + ea * wax,
        hmz + wd * (hdz - hmz) + wa * az + ed * wdz + ea * waz,
    )
}

// The land's parameters: sea level, and the rivers' half-width in river-
// noise units (0: no rivers).
#[derive(Clone, Copy)]
pub struct Land {
    pub sea: f32,
    pub river: f32,
}

// Rivers run where a broad, low-frequency noise crosses its midpoint, which
// traces long meandering lines; there the ground is carved down to a bed
// below sea level. Carving only lowers the ground, so terrain_upper stays an
// upper bound.
pub const river_noise_scale: f32 = 0.012;
#[inline]
pub fn river_bed(l: Land) -> f32 {
    l.sea - 0.6
}

// (weight in [0, 1], 0 in the channel; d weight / d f)
pub fn river_weight(l: Land, f: f32) -> (f32, f32) {
    if l.river <= 0.0 {
        (1.0, 0.0)
    } else {
        let e0 = 0.5 * l.river;
        let e1 = 1.5 * l.river;
        let a = (f - 0.5).abs();
        let t = saturate((a - e0) / (e1 - e0));
        let ds = if t <= 0.0 || t >= 1.0 { 0.0 } else { 6.0 * t * (1.0 - t) / (e1 - e0) };
        (t * t * (3.0 - 2.0 * t), ds * sgn(f - 0.5))
    }
}

// Carves height h (with gradient hx, hz) by weight m (with gradient mx, mz):
// h - (1 - m) max(0, h - bed).
pub fn carve(l: Land, h: f32, hx: f32, hz: f32, m: f32, mx: f32, mz: f32) -> (f32, f32, f32) {
    let above = h > river_bed(l);
    let g = if above { h - river_bed(l) } else { 0.0 };
    let (gx, gz) = if above { (hx, hz) } else { (0.0, 0.0) };
    (h - (1.0 - m) * g, hx - (1.0 - m) * gx + g * mx, hz - (1.0 - m) * gz + g * mz)
}

// The terrain's octaves (its main fbm's; 5 gave a little more fine
// relief for 0.66 ms a frame at 1080p).
pub const terrain_octaves: i32 = 4;

// `detail` in octaves (terrain_octaves = full); far-away terrain needs fewer.
// (given the biome weights there: see biome)
pub fn terrain_height_in(l: Land, wd: f32, wa: f32, detail: f32, x: f32, z: f32) -> f32 {
    let n = fbm2_lod(terrain_octaves, detail, x * 0.035, z * 0.035);
    // (the desert has no ridges)
    let ridge = if wd >= 1.0 { 0.0 } else { 1.0 - (2.0 * fbm2_lod(3, detail - 2.0, x * 0.02 + 5.2, z * 0.02 - 1.7) - 1.0).abs() };
    let hm = 9.0 * n * n + 6.0 * ridge * ridge * ridge - 4.0;
    let hd = if wd > 0.0 { dune_height(x, z, n, 0.0, 0.0).0 } else { 0.0 };
    let ha = if wa > 0.0 { alpine_height(hm).0 } else { 0.0 };
    let h = hm + wd * (hd - hm) + wa * (ha - hm);
    // (rivers only carve ground above their bed, so lower ground needs no
    // river noise)
    if l.river <= 0.0 || h <= river_bed(l) {
        h
    } else {
        let f = fbm2_lod(2, 2.0, x * river_noise_scale + 31.7, z * river_noise_scale - 12.3);
        carve(l, h, 0.0, 0.0, river_weight(l, f).0, 0.0, 0.0).0
    }
}

pub fn terrain_height_lod(l: Land, detail: f32, x: f32, z: f32) -> f32 {
    let (wd, wa) = biome(x, z);
    terrain_height_in(l, wd, wa, detail, x, z)
}

#[inline]
pub fn terrain_height(l: Land, x: f32, z: f32) -> f32 {
    terrain_height_lod(l, 5.0, x, z)
}

// terrain_height_lod with its gradient: (height, dh/dx, dh/dz).
// h = 9 n^2 + 6 r^3 - 4 with r = 1 - |2f - 1|, then carved by the rivers.
pub fn terrain_height_grad(l: Land, detail: f32, x: f32, z: f32) -> (f32, f32, f32) {
    let (n, nx, nz) = fbm2_lod_grad(terrain_octaves, detail, x * 0.035, z * 0.035);
    let (f, fx, fz) = fbm2_lod_grad(3, detail - 2.0, x * 0.02 + 5.2, z * 0.02 - 1.7);
    let r = 1.0 - (2.0 * f - 1.0).abs();
    let dr = -2.0 * sgn(2.0 * f - 1.0); // dr/df
    let hn = 18.0 * n; // dh/dn
    let hr = 18.0 * r * r; // dh/dr
    let hm = 9.0 * n * n + 6.0 * r * r * r - 4.0;
    let hmx = 0.035 * hn * nx + 0.02 * hr * dr * fx;
    let hmz = 0.035 * hn * nz + 0.02 * hr * dr * fz;
    let (wd, wdx, wdz, wa, wax, waz) = biome_grad(x, z);
    let (hd, hdx, hdz) = if wd > 0.0 { dune_height(x, z, n, 0.035 * nx, 0.035 * nz) } else { (0.0, 0.0, 0.0) };
    let (h, hx, hz) = biome_blend(hm, hmx, hmz, hd, hdx, hdz, wd, wdx, wdz, wa, wax, waz);
    if l.river <= 0.0 || h <= river_bed(l) {
        (h, hx, hz)
    } else {
        let (rf, rfx, rfz) = fbm2_lod_grad(2, 2.0, x * river_noise_scale + 31.7, z * river_noise_scale - 12.3);
        let (m, dm) = river_weight(l, rf);
        carve(l, h, hx, hz, m, dm * rfx * river_noise_scale, dm * rfz * river_noise_scale)
    }
}

// The terrain's upward unit normal in world space.
pub fn terrain_normal(l: Land, detail: f32, x: f32, z: f32) -> Vec3 {
    let (_, hx, hz) = terrain_height_grad(l, detail, x, z);
    normalise(vec(-hx, 1.0, -hz))
}

// A heightfield is not a distance: a point at height y over ground of slope
// at most s is at least (y - h) / sqrt (1 + s^2) from it. Away from the
// rivers' banks the slopes stay below about 1.55, and 1.85 in the
// mountains (measured over 36 km^2), so scaling the height difference by
// this (for the alpine weight wa), and the march's own 0.85, keeps sphere
// tracing safe.
#[inline]
pub fn terrain_lipschitz(wa: f32) -> f32 {
    0.6 - 0.1 * wa
}

// An upper bound on terrain_height from the first octave of each fbm alone
// (2 noise lookups instead of 8): the other octaves can add at most their
// amplitudes. The height grows with n and with the ridge, so bounding those
// bounds it.
// (given the biome weights there)
pub fn terrain_upper(wd: f32, wa: f32, x: f32, z: f32) -> f32 {
    // (the first octave has amplitude 0.5 of the total 1 - 0.5^octaves; the
    // others together 0.5 - 0.5^octaves)
    let total = 1.0 - 0.5f32.powf(terrain_octaves as f32);
    let n_hi = (0.5 * vnoise2(x * 0.035, z * 0.035) + (total - 0.5)) / total;
    let f_lo = 0.5 * vnoise2(x * 0.02 + 5.2, z * 0.02 - 1.7) / 0.875;
    let f_hi = f_lo + 0.375 / 0.875;
    let ridge_hi = if f_lo <= 0.5 && f_hi >= 0.5 { 1.0 } else { 1.0 - (2.0 * f_lo - 1.0).abs().minimum((2.0 * f_hi - 1.0).abs()) };
    let hm = 9.0 * n_hi * n_hi + 6.0 * ridge_hi * ridge_hi * ridge_hi - 4.0;
    // (each biome's height grows with n and the meadow's, so bounding those
    // bounds each; the weights are the same, never negative and sum to at
    // most 1, so the blend of the bounds bounds the blend)
    let hd = -0.5 + 3.0 * n_hi + 0.9;
    hm + wd * (hd - hm) + wa * (alpine_height(hm).0 - hm)
}

// (the march evaluates the biome once per step for the bound and this)
#[inline]
pub fn terrain_dist(l: Land, wd: f32, wa: f32, detail: f32, w: Vec3) -> f32 {
    terrain_lipschitz(wa) * (w.y - terrain_height_in(l, wd, wa, detail, w.x, w.z))
}

// ---------------------------------------------------------------------
// Structures: one cell of a 30-unit grid in every few holds a cottage, a
// wizard's tower or a pyramid, at a random spot kept well inside its cell,
// standing on the ground there with a random rotation. Only dry ground
// below structure_ground_max is built on, so no structure rises above
// land_max (the land's early-out bound).

pub const struct_cell: f32 = 30.0;
pub const struct_reach: f32 = 3.2; // every structure fits within this of its centre (in xz)
pub const structure_ground_max: f32 = 2.8; // so structure_top = terrain_max
pub const structure_top: f32 = structure_ground_max + 8.2; // the tallest: a tower's roof tip
pub const land_max: f32 = terrain_max.maximum(structure_top); // nothing on land is higher

pub const kind_cottage: i32 = 0;
pub const kind_tower: i32 = 1;
pub const kind_pyramid: i32 = 2;

// The candidate structure of the cell holding (x, z): (kind, centre x, z, yaw).
pub fn structure_candidate(x: f32, z: f32) -> (i32, f32, f32, f32) {
    let cx = ftoi((x / struct_cell).floor());
    let cz = ftoi((z / struct_cell).floor());
    let r = hash2(cx * 7 + 3, cz * 13 + 5);
    let px = (cx as f32 + 0.25 + 0.5 * hash2(cx + 41, cz - 17)) * struct_cell;
    let pz = (cz as f32 + 0.25 + 0.5 * hash2(cx - 23, cz + 59)) * struct_cell;
    // pyramids in the desert, towers in the mountains, cottages between
    let (wd, wa) = biome(px, pz);
    let kind = if wd > 0.5 {
        if r < 0.8 { kind_pyramid } else { kind_tower }
    } else if wa > 0.5 {
        if r < 0.7 { kind_tower } else { kind_cottage }
    } else if r < 0.7 {
        kind_cottage
    } else {
        kind_tower
    };
    (kind, px, pz, 2.0 * PI * hash2(cx + 5, cz + 77))
}

// Whether the cell holding (x, z) has a structure at all, for density in [0, 1].
pub fn structure_present(density: f32, x: f32, z: f32) -> bool {
    let cx = ftoi((x / struct_cell).floor());
    let cz = ftoi((z / struct_cell).floor());
    hash2(cx * 31 + 11, cz * 17 + 3) < density
}

// Distance from (x, z) to the edge of its cell.
#[inline]
pub fn cell_border(x: f32, z: f32) -> f32 {
    let fx = x - struct_cell * (x / struct_cell).floor();
    let fz = z - struct_cell * (z / struct_cell).floor();
    fx.minimum(struct_cell - fx).minimum(fz.minimum(struct_cell - fz))
}

// Square pyramid with base 1 x 1 at y = 0 and apex at height h (after Quilez).
pub fn sd_pyramid(p: Vec3, h: f32) -> f32 {
    let m2 = h * h + 0.25;
    let (px, pz) = (p.x.abs(), p.z.abs());
    let (px, pz) = if pz > px { (pz, px) } else { (px, pz) };
    let (px, pz) = (px - 0.5, pz - 0.5);
    let qx = pz;
    let qy = h * p.y - 0.5 * px;
    let qz = h * px + 0.5 * p.y;
    let s = (-qx).maximum(0.0);
    let t = saturate((qy - 0.5 * pz) / (m2 + 0.25));
    let a = m2 * (qx + s) * (qx + s) + qy * qy;
    let b = m2 * (qx + 0.5 * t) * (qx + 0.5 * t) + (qy - m2 * t) * (qy - m2 * t);
    let d2 = if qy.minimum(-qx * m2 - qy * 0.5) > 0.0 { 0.0 } else { a.minimum(b) };
    ((d2 + qz * qz) / m2).sqrt() * sgn(qz.maximum(-p.y))
}

// Structure parts (for materials).
pub const part_wall: i32 = 0;
pub const part_roof: i32 = 1;
pub const part_stone: i32 = 2;
pub const part_slate: i32 = 3;
pub const part_sandstone: i32 = 4;

// A structure in its own frame (ground at y = 0): (distance, part). Each is
// sunk into the ground on a foundation, so slopes leave no gap beneath it.
#[inline(never)]
pub fn sd_structure(kind: i32, q: Vec3) -> (f32, i32) {
    if kind == kind_cottage {
        let walls = sd_box(q - vec(0.0, 0.1, 0.0), vec(1.8, 2.1, 1.3));
        // a gable roof along x: from the eaves at y 2.2 (z = +-1.6) to the ridge at 3.4
        let gable = (q.z.abs() * 1.2 + (q.y - 3.4) * 1.6) / 2.0;
        let roof = gable.maximum(2.15 - q.y).maximum(q.x.abs() - 2.0);
        let chimney = sd_box(q - vec(1.1, 3.2, 0.5), vec(0.22, 0.6, 0.22));
        if walls < roof.minimum(chimney) { (walls, part_wall) } else { (roof.minimum(chimney), part_roof) }
    } else if kind == kind_tower {
        let body = sd_cylinder(q - vec(0.0, 1.75, 0.0), 1.0, 3.75);
        let ring = sd_cylinder(q - vec(0.0, 5.45, 0.0), 1.15, 0.15);
        let roof = sd_capped_cone(q - vec(0.0, 5.6, 0.0), 2.6, 1.35, 0.03);
        let stone = body.minimum(ring);
        if stone < roof { (stone, part_stone) } else { (roof, part_slate) }
    } else {
        // 5.2 across, 3.6 high, its base sunk 0.6 into the ground
        let s = 5.2;
        (sd_pyramid(scale(1.0 / s, q + vec(0.0, 0.6, 0.0)), 4.2 / s) * s, part_sandstone)
    }
}

// The structure of the cell holding a world point, if there is one: (present,
// kind, its frame's origin and yaw). Evaluates the ground there.
#[inline(never)]
pub fn structure_at(l: Land, density: f32, w: Vec3) -> (bool, i32, Vec3, f32) {
    let (kind, px, pz, yaw) = structure_candidate(w.x, w.z);
    let ground = terrain_height(l, px, pz);
    let dry = ground > l.sea + 0.3 && ground < structure_ground_max;
    (structure_present(density, w.x, w.z) && dry, kind, vec(px, ground, pz), yaw)
}

// The structures' distance at a world point: (distance, part). Every bound
// returned is a true lower bound, so it is safe to step by:
// - above every roof, the height above structure_top;
// - structures in other cells lie at least (cell_border + margin) away, as
//   each stays struct_reach of its centre and its centre stays a quarter
//   cell inside its cell;
// - the own cell's structure lies at least (xz distance - struct_reach) away.
// The ground (8 noise lookups) is only evaluated close to a structure.
#[inline(never)]
pub fn structures(l: Land, density: f32, w: Vec3) -> (f32, i32) {
    let above = w.y - structure_top;
    if above > 0.3 {
        return (above, part_wall);
    }
    let far = cell_border(w.x, w.z) + (0.25 * struct_cell - struct_reach);
    // (cells without a structure stop here, after one hash)
    if density <= 0.0 || !(structure_present(density, w.x, w.z)) {
        return (above.maximum(far), part_wall);
    }
    let (kind, px, pz, yaw) = structure_candidate(w.x, w.z);
    let own = length2(vec2(w.x - px, w.z - pz)) - struct_reach;
    let bound = above.maximum(far.minimum(own));
    if bound > 0.3 {
        return (bound, part_wall);
    }
    let ground = terrain_height(l, px, pz);
    let dry = ground > l.sea + 0.3 && ground < structure_ground_max;
    if !dry {
        return (above.maximum(far), part_wall);
    }
    let (d, part) = sd_structure(kind, yaw_inv(yaw, w - vec(px, ground, pz)));
    (d.minimum(far), part)
}

// ---------------------------------------------------------------------
// Trees: one cell of a tree_cell grid in every few holds a tree, in groves
// with clearings between; broadleaf trees in the meadows and pines in the
// mountains, none in the desert, and none above the tree line or on the
// shore. Bounded like the structures: each tree stays within tree_reach of
// its centre, which stays tree_inset cells inside its cell.
//
// A broadleaf tree is an L-system (each branch ends in two shorter,
// thinner branches) evaluated as a distance in time linear in its depth,
// not in its 2^depth branches: at each level space is mirrored (|x|),
// turned and moved to the branch's end, so one capsule per level stands for
// every branch of that level.

pub const tree_cell: f32 = 12.0;
pub const tree_reach: f32 = 2.2; // every tree fits within this of its centre (in xz)
pub const tree_inset: f32 = 0.38; // (so trees in other cells are 2.36 further than the border)
pub const tree_height_max: f32 = 4.6;
pub const tree_size_max: f32 = 1.1;
pub const tree_tall: f32 = 3.82 * tree_size_max; // (measured: the tallest point of either kind, at size 1)

pub const tree_line: f32 = 4.5; // no trees on ground higher than this (so they pass under the rock)
pub const tree_top: f32 = tree_line + tree_height_max; // under terrain_max

pub const kind_broadleaf: i32 = 0;
pub const kind_pine: i32 = 1;

// Groves and clearings: 0 to 1.
#[inline]
pub fn grove(x: f32, z: f32) -> f32 {
    smoothstep(0.3, 0.55, 0.5 + 0.25 * ((0.05 * x + 1.3 * (0.031 * z).sin()).sin() + (0.043 * z - 0.9 * (0.037 * x).sin() + 2.0).sin()))
}

// The candidate tree of the cell holding (x, z): (present, kind, centre x,
// z, size, seed in [0, 1)).
pub fn tree_candidate(density: f32, x: f32, z: f32) -> (bool, i32, f32, f32, f32, f32) {
    let cx = ftoi((x / tree_cell).floor());
    let cz = ftoi((z / tree_cell).floor());
    let seed = hash2(cx * 19 + 7, cz * 23 + 1);
    let px = (cx as f32 + tree_inset + (1.0 - 2.0 * tree_inset) * hash2(cx + 13, cz - 29)) * tree_cell;
    let pz = (cz as f32 + tree_inset + (1.0 - 2.0 * tree_inset) * hash2(cx - 31, cz + 37)) * tree_cell;
    // (most cells fail the hash, without the biome and grove fields)
    let h = hash2(cx * 3 + 101, cz * 5 - 7);
    let (present, kind) = if h >= density {
        (false, kind_broadleaf)
    } else {
        let (wd, wa) = biome(px, pz);
        (h < density * (1.0 - wd) * grove(px, pz), if wa > 0.25 + 0.5 * seed { kind_pine } else { kind_broadleaf })
    };
    (present, kind, px, pz, 0.8 + (tree_size_max - 0.8) * hash2(cx + 3, cz + 9), seed)
}

// Tree parts.
pub const part_trunk: i32 = 0;
pub const part_leaves: i32 = 1;
pub const part_needles: i32 = 2;

// A broadleaf tree, ground at y = 0, at most tree_reach / 1.2 across and
// tree_height_max / 1.2 tall: (distance, part).
pub fn sd_broadleaf(seed: f32, q: Vec3) -> (f32, i32) {
    let trunk = sd_round_cone_ab(q, vec(0.0, -0.3, 0.0), vec(0.0, 1.3, 0.0), 0.15, 0.09);
    let (mut d, mut p, mut len, mut rad) = (trunk, q - vec(0.0, 1.3, 0.0), 0.85f32, 0.085f32);
    let mut i = 0;
    while i < 3 {
        // turn the branching plane (about 100 degrees a level, varied per tree),
        // mirror, then lean the branch out from the mirror plane
        let p1 = yaw(1.75 + 0.6 * seed + i as f32, p);
        let p1 = vec(p1.x.abs(), p1.y, p1.z);
        let lean = 0.42 + 0.1 * i as f32;
        let r = rot2(lean, vec2(p1.x, p1.y));
        let p1 = vec(r.x, r.y, p1.z);
        let d_n = d.minimum(sd_capsule(p1, vec(0.0, 0.0, 0.0), vec(0.0, len, 0.0), rad));
        let p_n = p1 - vec(0.0, len, 0.0);
        let len_n = 0.78 * len;
        let rad_n = 0.62 * rad;
        d = d_n;
        p = p_n;
        len = len_n;
        rad = rad_n;
        i += 1;
    }
    let (wood, tip) = (d, p);
    // a clump of leaves at each tip (lumpy: the bumps have slope under 0.45,
    // so the 0.7 keeps it a bound), and one over the crown's middle
    let bumps = 0.04 * (6.0 * q.x + 7.0 * seed).sin() * (6.0 * q.y).sin() * (6.0 * q.z + 3.0 * seed).sin();
    let clump = sd_sphere(tip - vec(0.0, 0.1, 0.0), 0.55);
    let middle = sd_ellipsoid(q - vec(0.0, 2.55, 0.0), vec(0.95, 0.75, 0.95));
    let leaves = 0.7 * (smin(0.3, clump, middle) + bumps);
    if wood < leaves { (wood, part_trunk) } else { (leaves, part_leaves) }
}

// A pine: a trunk and three tiers of needles, narrowing upwards.
pub fn sd_pine(seed: f32, q: Vec3) -> (f32, i32) {
    let trunk = sd_capsule(q, vec(0.0, -0.3, 0.0), vec(0.0, 3.4, 0.0), 0.09);
    let tier = move |y: f32, r: f32, h: f32| sd_capped_cone(q - vec(0.0, y, 0.0), h, r, 0.02);
    let s = 0.1 * seed;
    let needles = tier(0.7, 1.25 + s, 1.6).minimum(tier(1.6, 0.95 + s, 1.4).minimum(tier(2.4, 0.65, 1.4)));
    // (a ragged edge: needles in drooping bunches round each tier)
    let a = q.z.atan2(q.x);
    let needles = needles + 0.05 * (11.0 * a + 5.0 * q.y + 6.0 * seed).sin() * saturate(length2(vec2(q.x, q.z)) * 2.0);
    if trunk < 0.8 * needles { (trunk, part_trunk) } else { (0.8 * needles, part_needles) }
}

#[inline(never)]
pub fn sd_tree(kind: i32, seed: f32, q: Vec3) -> (f32, i32) {
    if kind == kind_pine { sd_pine(seed, q) } else { sd_broadleaf(seed, q) }
}

// The tree of the cell holding a world point: (present, kind, origin on the
// ground, size, seed). Evaluates the ground there.
pub fn tree_at(l: Land, density: f32, w: Vec3) -> (bool, i32, Vec3, f32, f32) {
    let (present, kind, px, pz, size, seed) = tree_candidate(density, w.x, w.z);
    let ground = if present { terrain_height(l, px, pz) } else { 0.0 };
    let ok = present && ground > l.sea + 0.4 && ground < tree_line;
    (ok, kind, vec(px, ground, pz), size, seed)
}

// Distance from (x, z) to the edge of its tree cell.
#[inline]
pub fn tree_cell_border(x: f32, z: f32) -> f32 {
    let fx = x - tree_cell * (x / tree_cell).floor();
    let fz = z - tree_cell * (z / tree_cell).floor();
    fx.minimum(tree_cell - fx).minimum(fz.minimum(tree_cell - fz))
}

// The trees' distance at a world point: (distance, part), with the same
// kinds of bounds as the structures', and one more: a tree is no taller
// than its ground's cheap upper bound (capped by the tree line) plus its
// height, so the ground under it (8 noise lookups) is only needed close by.
// `grown` in [0, 1] scales every tree (towards the edge of where they are
// drawn at all: see land_built); shrinking only shortens distances. Where
// none are drawn, `outside` is the distance to where they are.
#[inline(never)]
pub fn trees(l: Land, density: f32, grown: f32, outside: f32, w: Vec3) -> (f32, i32) {
    let above = (w.y - tree_top).maximum(outside);
    if above > 0.3 {
        return (above, part_trunk);
    }
    let far = tree_cell_border(w.x, w.z) + (tree_inset * tree_cell - tree_reach);
    if density <= 0.0 || grown <= 0.0 {
        return (above.maximum(far), part_trunk);
    }
    let (present, kind, px, pz, size, seed) = tree_candidate(density, w.x, w.z);
    if !present {
        return (above.maximum(far), part_trunk);
    }
    let size = size * grown;
    let own = length2(vec2(w.x - px, w.z - pz)) - tree_reach * size / tree_size_max;
    let b = above.maximum(far.minimum(own));
    if b > 0.3 {
        return (b, part_trunk);
    }
    let (wd, wa) = biome(px, pz);
    let tall = w.y - (terrain_upper(wd, wa, px, pz).minimum(tree_line) + tree_tall * size / tree_size_max);
    if tall > 0.3 {
        return (tall.maximum(far.minimum(own)), part_trunk);
    }
    let ground = terrain_height(l, px, pz);
    if ground <= l.sea + 0.4 || ground >= tree_line {
        return (above.maximum(far), part_trunk);
    }
    let (d, part) = sd_tree(kind, seed, scale(1.0 / size, yaw(6.28 * seed, w - vec(px, ground, pz))));
    ((d * size).minimum(far), part)
}
