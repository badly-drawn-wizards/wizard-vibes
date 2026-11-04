// fut/collide.fut: collision probing for free flight and the spells. The
// rock and the wizard are covered by a few spheres; for each, the probe
// gives the land's distance from its surface and the direction out of the
// land, so the app can push the wizard clear. The app calls `probe` on the
// CPU each frame.

use crate::prelude::*;

// (centre in the wizard's frame, radius): the platform's top, middle and
// tip (the carpet has only a top: its others shrink into the first), the
// wizard's body and head, and the orb.
#[inline]
pub fn collision_spheres(kind: i32) -> [(Vec3, f32); 6] {
    let carpet = kind == platform_carpet;
    [
        (vec(0.0, -0.5, 0.0), rock_radius),
        if carpet { (vec(0.0, -0.5, 0.0), 0.0) } else { (vec(0.0, -1.7, 0.0), 0.95) },
        if carpet { (vec(0.0, -0.5, 0.0), 0.0) } else { (vec(0.0, -2.6, 0.0), 0.45) },
        (vec(0.0, 1.0, 0.0), 0.5),
        (vec(0.0, 2.0, 0.0), 0.4),
        (orb_pos(), 0.2),
    ]
}

// The land at a world point, in the visible layers: (distance, outward unit
// normal). The ground's distance is its height above the terrain scaled by
// the slope (exact for a plane); the water is solid.
pub fn land_probe(prm: Params, w: Vec3) -> (f32, Vec3) {
    let l = land_of(prm);
    let ground = if !has(prm.layers, layers_terrain) {
        (INF, vec(0.0, 1.0, 0.0))
    } else {
        let (h, hx, hz) = terrain_height_grad(l, 5.0, w.x, w.z);
        let n = vec(-hx, 1.0, -hz);
        ((w.y - h) / length(n), normalise(n))
    };
    let water = if has(prm.layers, layers_water) { (w.y - prm.sea_level, vec(0.0, 1.0, 0.0)) } else { (INF, vec(0.0, 1.0, 0.0)) };
    let built = if !has(prm.layers, layers_structures) {
        (INF, vec(0.0, 1.0, 0.0))
    } else {
        // (structures gives only a lower bound away from them, so use the
        // exact distance to this cell's structure: any in another cell is
        // further than the largest sphere's radius)
        let (present, kind, o, yaw) = structure_at(l, prm.structure_density, w);
        let f = move |p: Vec3| if present { sd_structure(kind, yaw_inv(yaw, p - o)).0 } else { INF };
        let d = f(w);
        let e = 0.01;
        let n = vec(
            f(w + vec(e, 0.0, 0.0)) - f(w - vec(e, 0.0, 0.0)),
            f(w + vec(0.0, e, 0.0)) - f(w - vec(0.0, e, 0.0)),
            f(w + vec(0.0, 0.0, e)) - f(w - vec(0.0, 0.0, e)),
        );
        (d, if length(n) > 0.0 { normalise(n) } else { vec(0.0, 1.0, 0.0) })
    };
    let nearer = |a: (f32, Vec3), b: (f32, Vec3)| if a.0 <= b.0 { a } else { b };
    nearer(ground, nearer(water, built))
}

// One probe result: [distance from the sphere's surface to the land
// (negative: inside), normal x, y, z], in world space.
#[inline]
pub fn probe_one(prm: Params, w: Vec3, r: f32) -> [f32; 4] {
    let (d, n) = land_probe(prm, w);
    [(1e6f32).minimum(d) - r, n.x, n.y, n.z]
}

// probe_frame: 4 floats (see probe_one) per sphere of collision_spheres,
// then the same for each world point in pts (x, y, z each; the spells), as
// from a sphere of radius 0. out holds 4 * (6 + pts.len() / 3) floats.
#[cfg(not(target_arch = "spirv"))]
pub fn probe(prm: &Params, dr: &Drive, secs: f32, pts: &[f32], out: &mut [f32]) {
    let fl = flight_of(*prm, *dr, secs);
    let spheres = collision_spheres(prm.platform);
    let k = pts.len() / 3;
    for (s, (c, r)) in spheres.iter().enumerate() {
        out[4 * s..4 * s + 4].copy_from_slice(&probe_one(*prm, to_world(fl, *c), *r));
    }
    for i in 0..k {
        let w = vec(pts[3 * i], pts[3 * i + 1], pts[3 * i + 2]);
        out[4 * (6 + i)..4 * (6 + i) + 4].copy_from_slice(&probe_one(*prm, w, 0.0));
    }
}

// How many floats probe writes for np points (x, y, z each: pts.len() / 3).
pub const fn probe_len(np: usize) -> usize {
    4 * (6 + np)
}
