// fut/scene.fut: the whole scene: the wizard (from figure.rs, posed by
// rig.rs), the rune circle and rock he stands on, and the land far below;
// the scene functions the marches use, by layer. (Futhark's `open import`s
// of materials, rig, figure, platform and enemies: the prelude has them.)

use crate::prelude::*;

// The enemies (see enemies.rs), in the layers of `mask`.
#[inline]
pub fn enemies(mask: i32, pose: Pose, p: Vec3) -> Hit {
    if !(has(mask, layers_enemies)) || pose.prm.enemies <= 0 {
        nothing
    } else {
        enemies_dist(pose.prm, pose.foes, pose.secs, pose.fl, p)
    }
}

// His platform, in the layer of the rock (see platform.rs).
#[inline]
pub fn rock(pose: Pose, p: Vec3) -> Hit {
    tag(layers_rock, platform(pose.prm.platform, pose.secs, p))
}

// The terrain far below, sampled through the flight frame. No peak is above
// terrain_max, so well above it the height over terrain_max is a cheap lower
// bound (it stays >= 1.5 here, so it can never register as a hit).
//
// Terrain detail (octaves) is deliberately not reduced with distance: a
// distance-based fade is centred on the wizard, who moves through the world,
// so the ground changed shape as he flew past it (see scene.fut).
#[inline]
pub fn terrain_detail(_p: Vec3) -> f32 {
    5.0
}

#[inline]
pub fn land_of(prm: Params) -> Land {
    Land { sea: prm.sea_level, river: prm.river_width }
}

// The world below, in the layers of `mask`: the ground, the water (sea and
// rivers, a plane at sea level) and the structures.
pub const land_layers: i32 = layers_terrain | layers_water | layers_structures | layers_trees;

// The ground and the water (a real GPU function taking just the flight and
// the land, not the whole pose).
#[inline(never)]
pub fn ground_at(mask: i32, fl: Flight, l: Land, p: Vec3) -> Hit {
    let w = to_world(fl, p);
    let detail = terrain_detail(p);
    let ground = if !(has(mask, layers_terrain)) {
        nothing
    } else if w.y > terrain_max + 1.5 {
        tag(layers_terrain, (w.y - terrain_max, m_terrain))
    } else {
        // the cheap bound clears most points near the wizard, his rock and
        // the clouds; only close to the ground is the full terrain needed
        // (rivers only lower the ground, so it stays an upper bound)
        let (wd, wa) = biome(w.x, w.z);
        // (the bound's noise is the full terrain's first octaves)
        let o = terrain_first_octaves(w.x, w.z);
        let gap = w.y - terrain_upper_from(wd, wa, o);
        tag(
            layers_terrain,
            (if gap > 1.5 { terrain_lipschitz(wa) * gap } else { terrain_dist_from(l, wd, wa, detail, o, w) }, m_terrain),
        )
    };
    let water = if has(mask, layers_water) { tag(layers_water, (w.y - l.sea, m_water)) } else { nothing };
    umin(ground, water)
}

#[inline]
pub fn land_ground(mask: i32, pose: Pose, p: Vec3) -> Hit {
    ground_at(mask, pose.fl, land_of(pose.prm), p)
}

pub const tree_near: f32 = 50.0;
pub const tree_far: f32 = 75.0;

// The structures.
#[inline]
pub fn land_structures(mask: i32, pose: Pose, p: Vec3) -> Hit {
    if !(has(mask, layers_structures)) {
        nothing
    } else {
        let (d, part) = structures(land_of(pose.prm), pose.prm.structure_density, to_world(pose.fl, p));
        tag(layers_structures, (d, m_wall + part))
    }
}

// The trees. They are drawn near the wizard only, where the camera always
// is: they shrink away between tree_near and tree_far from him; farther
// off, forests are a shade of the ground (see shading.rs).
// (at the High quality preset only: draw_trees)
#[inline]
pub fn land_trees(mask: i32, pose: Pose, p: Vec3) -> Hit {
    if !draw_trees || !(has(mask, layers_trees)) {
        nothing
    } else {
        let r = length(p);
        let (d, part) = trees(
            land_of(pose.prm),
            pose.prm.tree_density,
            smoothstep(tree_far, tree_near, r),
            r - tree_far,
            to_world(pose.fl, p),
        );
        tag(layers_trees, (d, m_trunk + part))
    }
}

// The same along a ray (see structures_in in world.rs), given the structure
// of the cell holding p.
#[inline]
pub fn land_structures_in(mask: i32, pose: Pose, exit: f32, c: StructureCell, p: Vec3) -> Hit {
    if !(has(mask, layers_structures)) {
        nothing
    } else {
        let (d, part) = structures_in(land_of(pose.prm), exit, c, to_world(pose.fl, p));
        tag(layers_structures, (d, m_wall + part))
    }
}

pub const no_tree_cell: TreeCell = TreeCell { present: false, kind: 0, px: 0.0, pz: 0.0, size: 0.0, seed: 0.0, known: false, ground: 0.0 };

// The same along a ray (exit: see structures in world.rs), given the tree
// cell holding p.
#[inline]
pub fn land_trees_in(mask: i32, pose: Pose, exit: f32, c: TreeCell, p: Vec3) -> Hit {
    if !draw_trees || !(has(mask, layers_trees)) {
        nothing
    } else {
        let r = length(p);
        let (d, part) = trees_in(
            land_of(pose.prm),
            pose.prm.tree_density,
            smoothstep(tree_far, tree_near, r),
            r - tree_far,
            exit,
            c,
            to_world(pose.fl, p),
        );
        tag(layers_trees, (d, m_trunk + part))
    }
}

#[inline]
pub fn land_built(mask: i32, pose: Pose, p: Vec3) -> Hit {
    umin(land_structures(mask, pose, p), land_trees(mask, pose, p))
}

#[inline]
pub fn terrain(mask: i32, pose: Pose, p: Vec3) -> Hit {
    umin(land_ground(mask, pose, p), land_built(mask, pose, p))
}

// #[noinline] (here #[inline(never)]): the scene is used by the march,
// normals, AO and both shadow loops, so the figure, the scene, the
// occluders and the media are compiled once as real GPU functions (see
// scene.fut for the WebGPU backend's limits that shaped the names).
// The scene near the wizard, in the layers of `mask`: the figure, its staff
// and gear, and the rock. (Primary rays march this and the terrain in
// separate passes, see follow in shading.rs.)
#[inline(never)]
pub fn local_scene(mask: i32, pose: Pose, tol: f32, p: Vec3) -> Hit {
    umin(bounded_figure(mask, pose, tol, p), if has(mask, layers_rock) { rock(pose, p) } else { nothing })
}

pub const local_layers: i32 = figure_layers | layers_rock;

// What casts the shadows near the wizard: a simpler stand-in for the
// figure (soft shadows need no fingers, beard locks, potions or tome),
// each part inside the real one, so no surface starts its shadow ray inside
// it: the robe and arms, the head as a sphere, the hat, the beard as a thin
// capsule along its axis, and the staff as a plain rod; and the rock.
#[inline(never)]
pub fn shadow_scene(mask: i32, pose: Pose, tol: f32, p: Vec3) -> f32 {
    let figure = if !(has(mask, layers_figure)) {
        1e9
    } else {
        let b = wizard_bound(p);
        if b > tol {
            b
        } else {
            let hq = head_space(pose, p);
            let x = smin(0.06, sd_robe(pose, body_space(pose, p)), sd_arms_shadow(pose, p));
            let x = sd_sphere(hq - vec(0.0, 1.6, 0.02), 0.17).minimum(x);
            let x = sd_hat(pose, p).minimum(x);
            sd_capsule(hq, beard_top, beard_tip, 0.05).minimum(x)
        }
    };
    let staff = if !(has(mask, layers_staff)) {
        1e9
    } else {
        sd_capsule(staff_rest(pose, p), staff_foot, staff_top, staff_radius)
    };
    let rock = if !(has(mask, layers_rock)) { 1e9 } else { rock(pose, p).0 };
    figure.minimum(staff.minimum(rock))
}

// Everything in local_scene lies in this sphere (radius 3.3), and so do the
// glowing media (the orb's cloud and tendrils, the rune circle; 3.6).
pub const occluder_centre: Vec3 = Vec3::new(0.0, -0.3, 0.0);
pub const occluder_radius: f32 = 3.3;
pub const local_radius: f32 = 3.6;

#[inline(never)]
pub fn scene(mask: i32, pose: Pose, tol: f32, p: Vec3) -> Hit {
    let h = umin(
        local_scene(mask, pose, tol, p),
        if has(mask, land_layers) { terrain(mask, pose, p) } else { nothing },
    );
    umin(h, enemies(mask, pose, p))
}

#[inline]
pub fn dist(mask: i32, pose: Pose, tol: f32, p: Vec3) -> f32 {
    scene(mask, pose, tol, p).0
}

// The terrain pass: sphere-traces the land alone (the layers of `mask` among
// land_layers) along ro + t rd, for t in [0, tmax], returning the hit's
// distance and tagged material, or infinity. A ray heading upwards
// from above the highest peak cannot hit it at all, and one heading down
// jumps straight to that height before stepping.
//
// The structures are rarely the nearest thing, so their distance is not
// evaluated at every step: a bound taken at t still bounds them at t' > t
// (less the way travelled, t' - t), so it is only evaluated again once that
// could cut the step short. Their bounds are taken along the ray: where it
// meets their tops' heights, and a cell with nothing near holds the ray up
// only where it leaves the cell (see cell_exit), not wherever the cell's edge
// is near; and what stands in a cell is worked out once per cell.
//
// The march is over-relaxed (Keinert et al. 2014, "Enhanced Sphere
// Tracing"): each step goes terrain_relax times the safe distance, and
// whenever the new point's bound shows that went too far (the two bounding
// spheres do not overlap), it steps back and carries on unrelaxed.
pub const terrain_relax: f32 = 1.4;

// Along a ray from height y going up at dy (a unit direction's), how far it
// is to the height `top` (0 if not above it, inf if never reaching it).
#[inline]
pub fn ray_to_plane(y: f32, dy: f32, top: f32) -> f32 {
    if y <= top {
        0.0
    } else if dy >= 0.0 {
        INF
    } else {
        (y - top) / -dy
    }
}

// Along a ray from p along the unit d, how far it is into the sphere of
// radius r round the origin (0 from inside, inf if it misses).
#[inline]
pub fn ray_sphere_entry(r: f32, p: Vec3, d: Vec3) -> f32 {
    let b = dot(p, d);
    let c = dot(p, p) - r * r;
    if c <= 0.0 {
        0.0
    } else {
        let disc = b * b - c;
        if b >= 0.0 || disc < 0.0 { INF } else { -b - disc.sqrt() }
    }
}

pub fn march_terrain(mask: i32, pose: Pose, ro: Vec3, rd: Vec3, tmax: f32) -> (f32, i32) {
    let wo = to_world(pose.fl, ro);
    let wd = to_world_dir(pose.fl, rd);
    let top = land_max + 1.5;
    if wd.y >= 0.0 && wo.y > land_max {
        return (INF, 0);
    }
    let t0 = if wo.y > top { (wo.y - top) / -wd.y } else { 0.0 };
    // (prev: the safe step from the last point; step: the step taken from it)
    let (mut t, mut i, mut hit, mut m) = ((0.02f32).maximum(t0), 0i32, false, 0i32);
    let (mut sd, mut st, mut sm) = (-1f32, 0f32, 0i32);
    let (mut td, mut tt, mut tm) = (-1f32, 0f32, 0i32);
    let (mut omega, mut prev, mut step) = (terrain_relax, 0f32, 0f32);
    // (the structure and tree cells last seen, as (x, z, what is there),
    // each worked out once)
    let mut sc: (i32, i32, StructureCell) = (i32::MIN, 0, (false, 0, 0.0, 0.0, 0.0));
    let mut tc: (i32, i32, TreeCell) = (i32::MIN, 0, no_tree_cell);
    while !hit && i < terrain_steps && t < tmax {
        let p = ro + scale(t, rd);
        let ground = land_ground(mask, pose, p);
        // the structures and the trees, each still bounded by what was found
        // for it at st (tt), or again
        let w = to_world(pose.fl, p);
        let (sd_n, st_n, sm_n) = if sd - (t - st) >= omega * ground.0 {
            (sd, st, sm)
        } else {
            // (along the ray, how far it is to the roofs' height)
            let over = ray_to_plane(w.y, wd.y, structure_top);
            if over > 0.3 {
                (over, t, sm)
            } else {
                let key = (ftoi((w.x / struct_cell).floor()), ftoi((w.z / struct_cell).floor()));
                if !((key.0 == sc.0 && key.1 == sc.1) || !(has(mask, layers_structures))) {
                    sc = (key.0, key.1, structure_cell_at(pose.prm.structure_density, w.x, w.z));
                }
                let (d, part) = land_structures_in(mask, pose, cell_exit(struct_cell, w, wd), sc.2, p);
                (d, t, part)
            }
        };
        let (td_n, tt_n, tm_n) = if td - (t - tt) >= omega * ground.0 {
            (td, tt, tm)
        } else {
            // (along the ray, how far it is to the trees' height and to where
            // they are drawn round him)
            let over = ray_to_plane(w.y, wd.y, tree_top).maximum(ray_sphere_entry(tree_far, p, rd));
            if over > 0.3 {
                (over, t, tm)
            } else {
                // (the cell's candidate and its ground, once per cell)
                let key = (ftoi((w.x / tree_cell).floor()), ftoi((w.z / tree_cell).floor()));
                if !((key.0 == tc.0 && key.1 == tc.1) || !draw_trees || !(has(mask, layers_trees))) {
                    tc = (key.0, key.1, tree_cell_grounded(land_of(pose.prm), pose.prm.tree_density, w.x, w.z));
                }
                let (d, part) = land_trees_in(mask, pose, cell_exit(tree_cell, w, wd), tc.2, p);
                (d, t, part)
            }
        };
        let (built, bm) = umin((sd_n - (t - st_n), sm_n), (td_n - (t - tt_n), tm_n));
        let (d, mat) = umin(ground, (built, bm));
        // (the relaxation follows the ground alone; the structures and trees,
        // whose bounds jump from cell to cell, would keep cutting it short,
        // so they only cap each step at their own safe distance)
        let safe = 0.85 * ground.0;
        let next = (omega * safe).minimum(0.85 * built);
        let (t_n, hit_n, m_n, omega_n, prev_n, step_n) = if omega > 1.0 && safe + prev < step {
            // overshot: back to where the last safe step would have ended
            (t - step + prev, false, m, 1.0, 0.0, 0.0)
        } else if d < 0.0015 * t {
            (t, true, mat, omega, safe, 0.0)
        } else {
            (t + next, false, m, omega, safe, next)
        };
        // (after stepping back, a bound taken further on says nothing of the
        // way back: forget it)
        let back = omega > 1.0 && safe + prev < step;
        t = t_n;
        i += 1;
        hit = hit_n;
        m = m_n;
        sd = if back && st_n > t_n { -1.0 } else { sd_n };
        st = st_n;
        sm = sm_n;
        td = if back && tt_n > t_n { -1.0 } else { td_n };
        tt = tt_n;
        tm = tm_n;
        omega = omega_n;
        prev = prev_n;
        step = step_n;
    }
    if hit { (t, m) } else { (INF, 0) }
}
