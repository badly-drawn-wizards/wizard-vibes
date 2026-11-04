// fut/shading.fut: materials, sky and lighting for the wizard scene.

use crate::prelude::*;

// ---------------------------------------------------------------------
// Surface detail

// What the sky depends on: projected out of the pose, so that sky_colour,
// a real GPU function (#[noinline]), takes a few scalars rather than the
// whole pose's.
#[derive(Clone, Copy)]
pub struct SkyIn {
    pub horizon: Vec3,
    pub zenith: Vec3,
    pub moon: Vec3,
    pub moon_size: f32,
    pub moon_halo: f32,
    pub moon_brightness: f32,
    pub star_density: f32,
    pub star_brightness: f32,
    pub twinkle: f32,
    pub secs: f32,
}

pub fn sky_in_of(pose: Pose) -> SkyIn {
    SkyIn {
        horizon: pose.prm.sky_horizon,
        zenith: pose.prm.sky_zenith,
        moon: pose.moon_world,
        moon_size: pose.prm.moon_size,
        moon_halo: pose.prm.moon_halo,
        moon_brightness: pose.prm.moon_brightness,
        star_density: pose.prm.star_density,
        star_brightness: pose.prm.star_brightness,
        twinkle: pose.prm.twinkle,
        secs: pose.secs,
    }
}

// The night sky in a world direction: a gradient, the moon, and stars.
#[inline(never)]
pub fn sky_colour(sk: SkyIn, rd: Vec3) -> Vec3 {
    let up = saturate(rd.y * 0.5 + 0.5);
    let base = mix(sk.horizon, sk.zenith, up.sqrt());
    // The moon: a disc of angular radius moon_size, with darker maria and
    // limb darkening, in a soft halo.
    let m = sk.moon;
    let r = sk.moon_size;
    let md = dot(rd, m);
    let ang = clamp(-1.0, 1.0, md).acos(); // angle from the moon's centre
    let rr = ang / r; // 0 at the centre, 1 at the limb
    let moon = if rr > 1.05 {
        0.0
    } else {
        let e1 = normalise(cross(m, vec(0.0, 1.0, 0.0)));
        let e2 = cross(e1, m);
        let u = dot(rd, e1) / r;
        let v = dot(rd, e2) / r;
        let maria = smoothstep(0.5, 0.65, 0.6 * vnoise2(u * 1.8 + 3.1, v * 1.8 - 1.7) + 0.4 * vnoise2(u * 4.3, v * 4.3 + 5.0));
        let craters = smoothstep(0.72, 0.8, vnoise2(u * 9.0 + 11.0, v * 9.0 - 7.0));
        let limb = 0f32.maximum(1.0 - rr * rr).sqrt().powf(0.4);
        smoothstep(1.0, 0.96, rr) * (0.55 + 0.45 * limb) * (1.0 - 0.45 * maria) * (1.0 - 0.2 * craters)
    };
    let halo = 0.15 * (-(1.0 - md) * 60.0).exp() + sk.moon_halo * (-0f32.maximum(ang - r) / 0.06).exp();
    // Stars: a few cells of a direction grid each hold a small round star at a
    // random point (kept off the cell's edges), twinkling at its own rate.
    let q = scale(220.0, rd);
    let c = vmap(|x| x.floor(), q);
    let (ix, iy, iz) = (ftoi(c.x), ftoi(c.y), ftoi(c.z));
    let s = hash3(ix, iy, iz);
    let star = {
        let thr = 1.0 - sk.star_density;
        if s <= thr || rd.y <= 0.02 {
            0.0
        } else {
            let at = c + vec(0.2 + 0.6 * hash3(ix, iy, iz + 71), 0.2 + 0.6 * hash3(ix + 13, iy, iz), 0.2 + 0.6 * hash3(ix, iy + 29, iz));
            let d = length(q - at) / 0.22; // in units of the star's size (~1.3 pixels at 1080p)
            let rate = 2.0 + 5.0 * hash3(ix + 5, iy + 3, iz);
            let twinkle = 1.0 - sk.twinkle + sk.twinkle * (sk.secs * rate + 6.28 * hash3(ix, iy + 7, iz + 3)).sin();
            (s - thr) / 0.0025 * sk.star_brightness * 0.0025 * (-(d * d)).exp() * twinkle * saturate(rd.y * 4.0) * smoothstep(1.2, 2.0, rr)
        }
    };
    base + scale(moon * sk.moon_brightness + halo, vec(0.85, 0.9, 1.0)) + splat(star)
}

pub fn sky(pose: Pose, rd: Vec3) -> Vec3 {
    sky_colour(sky_in_of(pose), rd)
}

// ---------------------------------------------------------------------
// Materials

// Metals tint their highlights with their albedo and have little diffuse.
// sheen: a cloth-like glow at grazing angles, tinted by the albedo.
#[derive(Clone, Copy)]
pub struct Material {
    pub albedo: Vec3,
    pub emission: Vec3,
    pub ks: f32,
    pub shininess: f32,
    pub rim: bool,
    pub metal: bool,
    pub sheen: f32,
}

#[inline]
pub fn plain(albedo: Vec3) -> Material {
    Material { albedo, emission: splat(0.0), ks: 0.08, shininess: 8.0, rim: true, metal: false, sheen: 0.0 }
}

// How far the sky shows through at distance t: the fog, and towards the view
// distance (where rays stop) a fade into the sky, so there is no edge.
#[inline]
pub fn fog_amount(fog: f32, view_distance: f32, t: f32) -> f32 {
    (1.0 - (-fog * t * t).exp()).maximum(1.0 - view_fade_at(view_distance, t))
}

#[inline]
pub fn fog_at(prm: Params, t: f32) -> f32 {
    fog_amount(prm.fog, prm.view_distance, t)
}

// The frame of the structure at local point p: (kind, origin, yaw). (It
// looks up the ground there, so work it out once per pixel.)
pub fn structure_frame(pose: Pose, p: Vec3) -> (i32, Vec3, f32) {
    let (_, kind, o, yaw) = structure_at(land_of(pose.prm), pose.prm.structure_density, to_world(pose.fl, p));
    (kind, o, yaw)
}

// Local point p in that structure's own frame.
#[inline]
pub fn structure_q(pose: Pose, o: Vec3, yaw: f32, p: Vec3) -> Vec3 {
    yaw_inv(yaw, to_world(pose.fl, p) - o)
}

// Stone bricks in running bond, `size` high and twice as long, on a wall
// with coordinates (u along it, v up): (mortar: 1 in a joint .. 0 on a
// brick, faded out as the pixel footprint fp nears the brick size; a random
// value per brick).
pub fn bricks(size: f32, fp: f32, u: f32, v: f32) -> (f32, f32) {
    let row = (v / size).floor();
    let x = u / (2.0 * size) + 0.5 * (row - 2.0 * (row / 2.0).floor());
    let dx = (0.5 - (fract(x) - 0.5).abs()) * 2.0 * size;
    let dy = (0.5 - (fract(v / size) - 0.5).abs()) * size;
    let joint = 0.07 * size;
    let mortar = smoothstep(joint, 0.5 * joint, dx.minimum(dy)) * keep(fp, size);
    (mortar, hash2(ftoi(x.floor()), ftoi(row)))
}

// Wall coordinates on a cottage (along whichever face is nearest) or round
// a tower (whole bricks round it, so no seam).
pub fn wall_uv(size: f32, kind: i32, q: Vec3) -> (f32, f32) {
    if kind == kind_tower {
        let around = 1f32.maximum(round_even(PI / size)); // bricks round the tower
        (q.z.atan2(q.x) / (2.0 * PI) * around * 2.0 * size, q.y)
    } else if q.x.abs() / 1.8 > q.z.abs() / 1.3 {
        (q.z, q.y)
    } else {
        (q.x, q.y)
    }
}

#[inline]
pub fn is_brick(m: i32) -> bool {
    m == m_wall || m == m_tower
}

// The bricks' relief: the mortar sits back in the joints.
pub fn brick_height(pose: Pose, fp: f32, kind: i32, o: Vec3, yaw: f32, p: Vec3) -> f32 {
    let size = pose.prm.brick_size;
    let (u, v) = wall_uv(size, kind, structure_q(pose, o, yaw, p));
    -0.06 * size * bricks(size, fp, u, v).0
}

// Brick colours from a base colour: each brick a little different, the
// mortar greyer and darker.
pub fn brick_colour(size: f32, fp: f32, kind: i32, q: Vec3, base: Vec3) -> Vec3 {
    let (u, v) = wall_uv(size, kind, q);
    let (mortar, r) = bricks(size, fp, u, v);
    let brick = scale(0.75 + 0.4 * r, base);
    mix(brick, scale(0.6, mix(base, splat(0.5), 0.5)), mortar)
}

// What the structures' materials depend on, projected out of the pose (see
// sky_in).
#[derive(Clone, Copy)]
pub struct StructureIn {
    pub fl: Flight,
    pub land: Land,
    pub density: f32,
    pub brick_size: f32,
    pub wall: Vec3,
    pub thatch: Vec3,
    pub tower: Vec3,
    pub slate: Vec3,
    pub sandstone: Vec3,
    pub window: Vec3,
    pub window_glow: f32,
}

// The structures' materials: stone bricks with lit windows on cottages and
// towers.
#[inline(never)]
pub fn structure_shade(si: StructureIn, fp: f32, m: i32, p: Vec3) -> Material {
    let w = to_world(si.fl, p);
    let (_, kind, o, yaw) = structure_at(si.land, si.density, w);
    let q = yaw_inv(yaw, w - o);
    let grain = 0.85 + 0.3 * vnoise2(w.x * 3.0 + w.z, w.y * 3.0);
    let lit = move |window: bool| if window { scale(si.window_glow, si.window) } else { splat(0.0) };
    if m == m_wall {
        // windows on the long walls (two each) and the gable ends (one each),
        // and a door on the front
        let long = q.z.abs() > 1.25 && (q.x.abs() - 0.9).abs() < 0.22 && (q.y - 1.25).abs() < 0.28;
        let gable = q.x.abs() > 1.75 && q.z.abs() < 0.22 && (q.y - 1.25).abs() < 0.28;
        let door = q.z > 1.25 && q.x.abs() < 0.3 && q.y < 1.5;
        let window = (long || gable) && kind == kind_cottage;
        let c = if door { vec(0.18, 0.1, 0.05) } else { brick_colour(si.brick_size, fp, kind, q, scale(grain, si.wall)) };
        Material { emission: lit(window), ..plain(if window { splat(0.05) } else { c }) }
    } else if m == m_thatch {
        plain(scale(0.7 + 0.5 * vnoise2(q.x * 9.0, q.y * 20.0), si.thatch))
    } else if m == m_tower {
        // four narrow lit windows round the tower, on two floors
        let a = q.z.atan2(q.x) / (2.0 * PI) * 4.0;
        let slot = (fract(a) - 0.5).abs() < 0.06;
        let floor = (q.y > 2.3 && q.y < 3.0) || (q.y > 4.0 && q.y < 4.6);
        let window = slot && floor;
        Material {
            emission: lit(window),
            ..plain(if window { splat(0.05) } else { brick_colour(si.brick_size, fp, kind, q, scale(grain, si.tower)) })
        }
    } else if m == m_slate {
        Material { ks: 0.3, shininess: 30.0, ..plain(si.slate) }
    } else {
        plain(scale(grain, si.sandstone))
    }
}

pub fn structure_material(pose: Pose, fp: f32, m: i32, p: Vec3) -> Material {
    let prm = pose.prm;
    structure_shade(
        StructureIn {
            fl: pose.fl,
            land: land_of(prm),
            density: prm.structure_density,
            brick_size: prm.brick_size,
            wall: wall_colour,
            thatch: thatch_colour,
            tower: tower_colour,
            slate: slate_colour,
            sandstone: sandstone_colour,
            window: prm.window_colour,
            window_glow: prm.window_glow,
        },
        fp,
        m,
        p,
    )
}

// What the water depends on, projected out of the pose (see sky_in).
#[derive(Clone, Copy)]
pub struct WaterIn {
    pub sky: SkyIn,
    pub fl: Flight,
    pub moon: Vec3,
    pub pix: f32,
    pub colour: Vec3,
    pub fog: f32,
    pub view_distance: f32,
}

// The water: rippled (the ripples fade with the pixel footprint), dark below,
// reflecting the sky by Fresnel, with a glint of the moon.
#[inline(never)]
pub fn water_shade(wi: WaterIn, p: Vec3, rd: Vec3, t: f32) -> Vec3 {
    let w = to_world(wi.fl, p);
    let s = wi.sky.secs;
    let k = keep(t * wi.pix, 0.6);
    let (_, ax, az) = vnoise2_grad(w.x * 1.3 + s * 0.3, w.z * 1.3 - s * 0.2);
    let (_, bx, bz) = vnoise2_grad(w.x * 3.1 - s * 0.5, w.z * 3.1 + s * 0.4);
    let gx = k * (0.04 * ax * 1.3 + 0.015 * bx * 3.1);
    let gz = k * (0.04 * az * 1.3 + 0.015 * bz * 3.1);
    let n = to_local_dir(wi.fl, normalise(vec(-gx, 1.0, -gz)));
    let cosi = saturate(-(dot(n, rd)));
    let f = 0.02 + 0.98 * pow(1.0 - cosi, 5.0);
    let r = reflect(rd, n);
    let glint = scale(1.5 * pow(saturate(dot(r, wi.moon)), 400.0), vec(0.85, 0.9, 1.0));
    let c = mix(wi.colour, sky_colour(wi.sky, to_world_dir(wi.fl, r)), f) + glint;
    let fog = fog_amount(wi.fog, wi.view_distance, t);
    mix(c, sky_colour(wi.sky, to_world_dir(wi.fl, rd)), fog)
}

pub fn water_surface(pose: Pose, p: Vec3, rd: Vec3, t: f32) -> Vec3 {
    water_shade(
        WaterIn {
            sky: sky_in_of(pose),
            fl: pose.fl,
            moon: pose.moon,
            pix: pose.pix,
            colour: pose.prm.water_colour,
            fog: pose.prm.fog,
            view_distance: pose.prm.view_distance,
        },
        p,
        rd,
        t,
    )
}

// A platform's surface (see platform.fut): m is m_ground (its top),
// m_rock, m_ice or m_carpet. Its colours are passed in (each enemy has its
// own); the circle's runes are added by the caller. (rock: dark, light and
// top colours, as rock_dark etc.)
pub fn platform_surface(kind: i32, rock: (Vec3, Vec3, Vec3), moss: Vec3, carpet: Vec3, fp: f32, m: i32, p: Vec3) -> Material {
    if m == m_ground {
        if kind == platform_ice {
            Material { ks: 0.5, shininess: 30.0, rim: false, ..plain(frost_albedo(ice_colour, fp, p)) }
        } else if kind == platform_carpet {
            Material { ks: 0.02, sheen: 0.5, rim: false, ..plain(carpet_albedo(carpet, carpet_trim, fp, p)) }
        } else {
            let (edge, id) = flagstone(p);
            let grain = 0.8 + 0.35 * vnoise3(scale(9.0, p)) * keep(fp, 0.1);
            let stone = scale((0.72 + 0.45 * id) * grain, rock.2);
            let moss_part = smoothstep(0.03, 0.006, edge) * keep(fp, 0.06) * (0.4 + 0.6 * vnoise3(scale(5.0, p)));
            Material { ks: 0.3, shininess: 40.0, rim: false, ..plain(mix(stone, scale(0.8, moss), moss_part)) }
        }
    } else if m == m_ice {
        // glossy, with a faint glow from within
        Material { ks: 1.2, shininess: 90.0, emission: scale(0.03, ice_colour), ..plain(ice_albedo(ice_colour, fp, p)) }
    } else if m == m_carpet {
        Material { ks: 0.02, sheen: 0.5, rim: false, ..plain(carpet_albedo(carpet, carpet_trim, fp, p)) }
    } else {
        // red-orange sandstone, banded and mottled (strongly saturated, since
        // the blue moonlight mutes warm colours)
        let band = 0.5 + 0.5 * (p.y * 9.0 + 2.0 * vnoise3(scale(1.5, p))).sin();
        let c = mix(rock.0, rock.1, band);
        Material { ks: 0.1, ..plain(scale(0.6 + 0.7 * vnoise3(scale(4.0, p)), c)) }
    }
}

// (inlined into its one caller, surface: as a GPU function of its own, the
// whole pose it takes came to more than WGSL's 255 parameters)
pub fn material(pose: Pose, _g: Glow, fp: f32, m: i32, p: Vec3, n: Vec3) -> Material {
    let tm = pose.tm;
    if m == m_robe {
        Material { rim: false, sheen: pose.prm.sheen, ks: 0.03, ..plain(cloth_albedo(pose, p)) }
    } else if m == m_staff {
        Material { ks: 0.05, ..plain(bark_albedo(pose.prm, fp, staff_rest(pose, p))) }
    } else if m == m_skin {
        Material { ks: 0.15, shininess: 16.0, ..plain(pose.prm.skin_colour) }
    } else if m == m_beard {
        Material { ks: 0.0, ..plain(hair_albedo(pose, p)) }
    } else if m == m_hat {
        let s = hat_stars(pose, fp, p).0;
        Material {
            emission: scale(pose.prm.star_glow * s, pose.prm.star_colour),
            ..plain(mix(pose.prm.hat_colour, pose.prm.star_colour, s))
        }
    } else if m == m_gold {
        Material { ks: 1.6, shininess: 48.0, metal: true, ..plain(gold_colour) }
    } else if m == m_band {
        Material { ks: 1.6, shininess: 48.0, metal: true, ..plain(band_colour) }
    } else if m == m_eye {
        Material { ks: 1.0, shininess: 150.0, rim: false, ..plain(eye_albedo(pose, p)) }
    } else if m == m_brow {
        plain(brow_colour)
    } else if m == m_wood {
        plain(scale(0.75 + 0.25 * (p.y * 45.0 + 3.0 * (p.x * 40.0).sin()).sin(), vec(0.36, 0.21, 0.1)))
    } else if m == m_rock || m == m_ice || m == m_carpet {
        platform_surface(pose.prm.platform, (pose.prm.rock_dark, pose.prm.rock_light, pose.prm.rock_top), pose.prm.moss_colour, carpet_colour, fp, m, p)
    } else if m == m_terrain {
        // moss in the valleys, bare rock on steep slopes, snow on high flats;
        // sand in the desert, and in the mountains more rock and snow lower
        // down (moss: broad drier and lusher patches, and up close the grass's
        // blades; stone: strata, warped; far off, where trees would stand, the
        // forest's canopy as clumps of leaves)
        let w = to_world(pose.fl, p);
        let up = to_world_dir(pose.fl, n).y;
        let (wd, wa) = biome(w.x, w.z);
        let grain = 0.75 + 0.5 * vnoise2(w.x * 1.7, w.z * 1.7);
        let dry = smoothstep(0.45, 0.85, vnoise2(w.x * 0.11 + 3.1, w.z * 0.11 - 7.7));
        let moss = mix(pose.prm.moss_colour, pose.prm.moss_colour * vec(1.7, 1.4, 0.75), dry);
        let moss = if fp < 0.014 { scale(0.75 + 0.6 * grass_blades(fp, w), moss) } else { scale(1.05, moss) };
        let strata = 0.5 + 0.5 * (w.y * 5.0 + 3.0 * vnoise2(w.x * 0.3, w.z * 0.3)).sin();
        let stone = scale(0.7 + 0.45 * strata * keep(fp, 0.4), pose.prm.stone_colour);
        let snow = pose.prm.snow_colour;
        let c = mix(stone, moss, smoothstep(0.7 + 0.12 * wa, 0.85 + 0.08 * wa, up));
        let canopy = (if draw_trees { smoothstep(tree_near, tree_far, length(p)) } else { 1.0 })
            * pose.prm.tree_density
            * (1.0 - wd)
            * grove(w.x, w.z)
            * smoothstep(tree_line + 0.5, tree_line - 0.5, w.y)
            * smoothstep(pose.prm.sea_level + 0.4, pose.prm.sea_level + 0.9, w.y);
        let clumps = vnoise2(w.x * 0.45, w.z * 0.45);
        let leaves = if canopy > 0.0 { scale(0.6 + 0.8 * vnoise2(w.x * 1.9, w.z * 1.9), leaf_colour) } else { leaf_colour };
        let c = mix(c, leaves, canopy * smoothstep(0.3, 0.55, clumps) * smoothstep(0.6, 0.8, up));
        let c = mix(c, pose.prm.sand_colour, wd * smoothstep(0.5, 0.75, up));
        let snow_line = pose.prm.snow_line - 0.5 * wa;
        let c = mix(c, snow, smoothstep(snow_line, snow_line + 1.5, w.y) * smoothstep(0.6 + 0.12 * wa, 0.8 + 0.1 * wa, up));
        Material { rim: false, ..plain(scale(grain, c)) }
    } else if m >= m_wall && m <= m_sandstone {
        structure_material(pose, fp, m, p)
    } else if m == m_leather {
        Material { ks: 0.35, shininess: 28.0, ..plain(leather_albedo(pose, fp, p)) }
    } else if m == m_cork {
        Material { ks: 0.05, ..plain(cork_albedo(fp, p)) }
    } else if m == m_tome {
        Material { ks: 0.15, shininess: 16.0, ..plain(tome_colour) }
    } else if m == m_pages {
        plain(vec(0.85, 0.8, 0.66))
    } else if m >= m_potion && m < m_potion + 3 {
        // the glowing liquid (its glass is drawn by `glass`)
        let c = glow_colour(pose.prm, m);
        Material { emission: scale(0.8, c), ks: 0.6, shininess: 60.0, ..plain(scale(0.3, c)) }
    } else if m == m_ground {
        Material {
            emission: scale(0.9 * pose.runes * rune_pulse(pose.prm.rune_pulse, tm) * rune_paint(tm, p), pose.prm.rune_colour),
            ..platform_surface(pose.prm.platform, (pose.prm.rock_dark, pose.prm.rock_light, pose.prm.rock_top), pose.prm.moss_colour, carpet_colour, fp, m, p)
        }
    } else {
        plain(splat(0.5))
    }
}

// The trees' materials, at world point w. (Apart from `material`, whose
// GPU function would otherwise pass more than WGSL's 255 parameters.)
#[inline]
pub fn is_tree(m: i32) -> bool {
    m == m_trunk || m == m_leaves || m == m_needles
}

#[inline(never)]
pub fn tree_material(trunk: Vec3, leaf: Vec3, needle: Vec3, m: i32, w: Vec3) -> Material {
    if m == m_trunk {
        Material { ks: 0.02, ..plain(scale(0.7 + 0.6 * vnoise2(w.y * 9.0, w.z.atan2(w.x) * 3.0), trunk)) }
    } else {
        // clumps of lighter and darker leaves, a little sheen on the lit side
        let v = 0.6 * vnoise3(scale(3.0, w)) + 0.4 * vnoise3(scale(11.0, w));
        Material {
            ks: 0.12,
            shininess: 12.0,
            rim: false,
            ..plain(scale(0.55 + 0.9 * v, if m == m_leaves { leaf } else { needle }))
        }
    }
}

// The enemies' materials (see enemies.fut), at point p of his space.
#[inline]
pub fn is_enemy(m: i32) -> bool {
    (m >= m_enemy && m < m_enemy + enemy_parts * max_enemies)
        || (full_enemies && m >= m_enemy_full && m < m_enemy_full + full_parts * max_enemies)
}

// An enemy's part (in its space, q), before what happens to it.
pub fn enemy_surface(lk: Looks, sheen: f32, glow: f32, tm: f32, rock: (Vec3, Vec3, Vec3), moss: Vec3, fp: f32, part: i32, q: Vec3) -> Material {
    if part == part_robe {
        Material { rim: false, sheen, ks: 0.03, ..plain(lk.robe) }
    } else if part == part_skin {
        Material { ks: 0.15, shininess: 16.0, ..plain(lk.skin) }
    } else if part == part_beard {
        Material { ks: 0.05, ..plain(lk.beard) }
    } else if part == part_hat {
        Material { ks: 0.05, ..plain(lk.hat) }
    } else if part == part_band {
        Material { ks: 1.6, shininess: 48.0, metal: true, ..plain(vec(1.0, 0.6, 0.14)) }
    } else if part == part_staff {
        Material { ks: 0.05, ..plain(scale(0.75 + 0.25 * (q.y * 40.0).sin(), vec(0.3, 0.19, 0.1))) }
    } else if part == part_orb {
        Material { emission: scale(2.5, lk.circle), ks: 1.0, shininess: 80.0, ..plain(scale(0.3, lk.circle)) }
    } else {
        let side = if lk.kind == platform_ice { m_ice } else if lk.kind == platform_carpet { m_carpet } else { m_rock };
        let mat = platform_surface(lk.kind, rock, moss, lk.carpet, fp, if part == part_top { m_ground } else { side }, q);
        // (the circle turns the other way to his; without his glowing
        // media, a halo round its strokes is painted on instead)
        if part != part_top {
            mat
        } else {
            let halo = (-(0f32.maximum(glyph_sdf(-tm, q.x, q.z))) / 0.05).exp();
            Material { emission: scale((0.25 + 0.06 * glow) * (5.0 * rune_paint(-tm, q) + halo), lk.circle), ..mat }
        }
    }
}

// (with full_enemies, the parts of his figure are shaded as his are, in
// the enemy's pose)
#[inline(never)]
pub fn enemy_material(prm: Params, g: Glow, fs: Foes, secs: f32, tm: f32, fl: Flight, fp: f32, m: i32, p: Vec3, n: Vec3) -> Material {
    let full = m >= m_enemy_full;
    let i = if full { idiv(m - m_enemy_full, full_parts) } else { idiv(m - m_enemy, enemy_parts) };
    let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
    let lk = looks_of(prm.enemy_seed, e.r#gen, i);
    let q = enemy_space(fl, e, p);
    let mat = if full_enemies && full {
        let n = yaw_inv(e.yaw, to_world_dir(fl, n));
        material(enemy_pose(prm, lk, secs), g, fp, imod(m - m_enemy_full, full_parts), q, n)
    } else {
        enemy_surface(lk, prm.sheen, prm.rune_glow, tm, (prm.rock_dark, prm.rock_light, prm.rock_top), prm.moss_colour, fp, imod(m - m_enemy, enemy_parts), q)
    };
    // appearing, it flashes in its circle's colour; defeated, it chars and
    // smoulders as it falls, its circle going out
    if e.appear > 0.0 {
        Material { emission: mat.emission + scale(2.0 * e.appear * e.appear, lk.circle), ..mat }
    } else if e.dying < 0.0 {
        mat
    } else {
        let burnt = smoothstep(0.0, 0.6, e.dying);
        let ember = smoothstep(0.62, 0.85, vnoise3(scale(16.0, q) + vec(0.0, -4.0 * secs, 0.0)));
        Material {
            albedo: scale(1.0 - 0.85 * burnt, mat.albedo),
            emission: scale(1.0 - burnt, mat.emission) + scale(0.9 * burnt * smoothstep(fall_time, 0.5, e.dying) * ember, vec(1.0, 0.35, 0.08)),
            ..mat
        }
    }
}

// ---------------------------------------------------------------------
// Lighting

// Diffuse plus Blinn specular from one light in direction l. The colours
// carry the light's intensity (including any attenuation).
pub fn light(mat: Material, n: Vec3, rd: Vec3, l: Vec3, shadow: f32, diffuse: Vec3, specular: Vec3) -> Vec3 {
    let lit = saturate(dot(n, l)) * shadow;
    let spec = mat.ks * pow(saturate(dot(n, normalise(l - rd))), mat.shininess) * lit;
    if mat.metal {
        scale(0.25 * lit, mat.albedo * diffuse) + scale(spec, mat.albedo * specular)
    } else {
        scale(lit, mat.albedo * diffuse) + scale(spec, specular)
    }
}

// ---------------------------------------------------------------------
// Lights

// A light: directional (pos is then its direction) or a point light whose
// intensity falls off as 1 / (1 + falloff d^2), and smoothly to nothing at
// `range` (cheap lights only reach nearby surfaces). Point lights of some
// size stop their shadow rays short of it. Which layers it involves:
//   casters             block it (local layers only: shadow rays are only
//                       traced through the sphere around the wizard);
//   receivers           are lit by it;
//   shadowed_receivers  of those, take its shadows.
#[derive(Clone, Copy)]
pub struct Light {
    pub pos: Vec3,
    pub directional: bool,
    pub diffuse: Vec3,
    pub specular: Vec3,
    pub falloff: f32,
    pub range: f32,
    pub radius: f32,
    pub casters: i32,
    pub receivers: i32,
    pub shadowed_receivers: i32,
}

// The moon, the orb, and a weak light in each potion.
pub const n_lights: i32 = 5;

#[inline(never)] // (few call sites: the whole pose is passed)
pub fn light_at(pose: Pose, g: Glow, k: i32) -> Light {
    if k == 0 {
        Light {
            pos: pose.moon,
            directional: true,
            diffuse: scale(pose.prm.moon_strength, pose.prm.moon_colour),
            specular: scale(pose.prm.moon_strength, vec(0.7, 0.75, 1.0)),
            falloff: 0.0,
            range: INF,
            radius: 0.0,
            casters: local_layers,
            receivers: layers_all,
            shadowed_receivers: layers_all,
        }
    } else if k == 1 {
        // The orb's light flickers with the lightning, whitening in flashes.
        // Its shadow rays stop short of it; and the staff layer takes no
        // shadows from it (a branch cannot really shadow a light at its own
        // tip, and its shadow rays graze every swelling and knot).
        let c = scale(pose.prm.orb_light * g.level, mix(vec(0.6, 0.5, 1.0), vec(0.9, 0.95, 1.0), 0.6 * g.flash) * pose.bolt_tint);
        Light {
            pos: pose.orb,
            directional: false,
            diffuse: c,
            specular: c,
            falloff: 1.5,
            range: INF,
            radius: orb_radius,
            casters: local_layers,
            receivers: layers_all,
            shadowed_receivers: layers_all & !layers_staff,
        }
    } else {
        // The potions: weak, unshadowed, reaching about half a unit and only
        // lighting the wizard's things, each with a slight flicker of its own
        // (whole multiples of the loop).
        let i = k - 2;
        let flicker = 1.0 + 0.12 * ((7 + 3 * i) as f32 * pose.tm + i as f32 * 2.1).sin();
        let c = scale(pose.prm.potion_light * flicker, glow_colour(pose.prm, m_potion + i));
        Light {
            pos: from_body_space(pose, potion_centre(gear_of(pose.prm), i)),
            directional: false,
            diffuse: c,
            specular: scale(0.3, c),
            falloff: 1.0 / (0.12 * 0.12),
            range: 0.5,
            radius: 0.04,
            casters: 0,
            receivers: figure_layers,
            shadowed_receivers: 0,
        }
    }
}

// The lit colour of a surface hit at distance t along rd (fog included);
// m is the hit's tagged material (with its layer).
#[inline(never)] // (few call sites: the whole pose is passed)
#[allow(non_snake_case)]
pub fn surface(pose: Pose, g: Glow, tagged: i32, p: Vec3, rd: Vec3, t: f32) -> Vec3 {
    let layer = hit_layer(tagged);
    let m = hit_material(tagged);
    let vis = pose.prm.layers; // only visible layers occlude, or show in AO
    // (the enemies are far from everything but themselves: leaving them out
    // spares the normal and AO probes elsewhere a loop over them)
    let vis = if layer == layers_enemies { vis } else { vis & !layers_enemies };
    if m == m_water {
        return water_surface(pose, p, rd, t);
    }
    // The terrain's normal comes from its height gradient, worked out
    // analytically (one evaluation instead of four of the whole scene), and
    // it gets no AO: at its scale the probes see nothing.
    let n = if m == m_terrain {
        let w = to_world(pose.fl, p);
        to_local_dir(pose.fl, terrain_normal(land_of(pose.prm), terrain_detail(p), w.x, w.z))
    } else {
        normal(move |tol, q| dist(vis, pose, tol, q), p)
    };
    // textures tilt the normal used for lighting; shadows and AO keep the
    // geometric one
    let fp = t * pose.pix;
    let ng = n;
    let n = if has_bump(m) {
        bumped(move |q| bump_height(pose, fp, m, q), p, ng)
    } else if is_brick(m) && keep(fp, pose.prm.brick_size) > 0.0 {
        let (kind, o, yaw) = structure_frame(pose, p);
        bumped(move |q| brick_height(pose, fp, kind, o, yaw, q), p, ng)
    } else {
        ng
    };
    let mat = if is_tree(m) {
        tree_material(trunk_colour, leaf_colour, needle_colour, m, to_world(pose.fl, p))
    } else if is_enemy(m) {
        enemy_material(pose.prm, g, pose.foes, pose.secs, pose.tm, pose.fl, fp, m, p, n)
    } else {
        material(pose, g, fp, m, p, n)
    };
    let occ = if m == m_terrain { 1.0 } else { ambient_occlusion(move |tol, q| dist(vis, pose, tol, q), p, ng) };
    let po = p + scale(0.002, ng);
    // (shadow rays that pass well clear of the figure and rock are fully lit)
    let shadow = move |casters: i32, l: Vec3, tmax: f32| -> f32 {
        let casters = casters & vis & local_layers;
        if casters == 0 || shadow_misses(occluder_centre, occluder_radius, po, l, tmax) {
            1.0
        } else {
            // (and they start where they come near that sphere and end shortly
            // after leaving it: nothing lies outside it, and the shadow loop's
            // steps are capped at 0.25)
            let tmax = tmax.minimum(sphere_exit(occluder_centre, occluder_radius, po, l) + 1.0);
            soft_shadow(move |tol, q| shadow_scene(casters, pose, tol, q), po, l, shadow_start(occluder_centre, occluder_radius, po, l, tmax), tmax)
        }
    };
    // All the lights, in one loop so the lighting and shadow code is
    // compiled once. The moon's shadow is kept for the cloth sheen.
    let (mut lit, mut moon_shadow) = (splat(0.0), 0f32);
    let mut k = 0;
    while k < n_lights {
        let L = light_at(pose, g, k);
        let (l, d) = if L.directional {
            (L.pos, INF)
        } else {
            let v = L.pos - p;
            let d = length(v);
            (scale(1.0 / d, v), d)
        };
        // (an endless range does not fade: smoothstep of infinities would
        // be NaN, which GPUs' min and max resolve differently)
        let atten = if L.directional { 1.0 } else { (if isinf(L.range) { 1.0 } else { smoothstep(L.range, 0.6 * L.range, d) }) / (1.0 + L.falloff * d * d) };
        // (skipped where the light is negligible, e.g. the orb on the far terrain)
        let (lit2, moon_shadow2) = if atten < 0.01 || !(has(L.receivers, layer)) {
            (lit, moon_shadow)
        } else {
            // (nor where the light is behind the surface, past where the
            // moon's sheen and hair light reach)
            let facing = dot(n, l) > if k == 0 { -0.4 } else { 0.0 };
            let sh = if !(has(L.shadowed_receivers, layer)) || d < L.radius || !facing {
                1.0
            } else {
                shadow(L.casters, l, if L.directional { 12.0 } else { d - L.radius - 0.15 })
            };
            let c = light(mat, n, rd, l, sh, scale(atten, L.diffuse), scale(atten, L.specular));
            (lit + c, if k == 0 { sh } else { moon_shadow })
        };
        lit = lit2;
        moon_shadow = moon_shadow2;
        k += 1;
    }
    let to_orb = pose.orb - p;
    let od = length(to_orb);
    let ol = scale(1.0 / od, to_orb);
    let fill = scale(occ * (0.5 + 0.5 * n.y) * pose.prm.fill, mat.albedo * vec(0.2, 0.22, 0.45));
    let rim = if mat.rim { scale(occ * pow(1.0 - saturate(-(dot(n, rd))), 3.0) * pose.prm.rim, vec(0.35, 0.3, 0.8)) } else { splat(0.0) };
    // Metals mirror their surroundings: the sky, and the glowing orb where
    // the reflected ray points at it (it is right next to the gold cradle).
    let mirror = if !mat.metal {
        splat(0.0)
    } else {
        let r = reflect(rd, n);
        let cos_orb = dot(r, ol);
        let disc = 1f32.minimum(orb_radius / od).asin().cos();
        let orb_seen = smoothstep(disc - 0.05, disc, cos_orb) + 0.25 * pow(saturate(cos_orb), 6.0);
        mat.albedo * (scale(0.6, sky(pose, to_world_dir(pose.fl, r))) + scale(1.6 * g.level * orb_seen, vec(0.75, 0.9, 1.0)))
    };
    // cloth sheen: brightest where the surface turns away from view, lit by
    // the moon where it reaches and a little by the sky everywhere
    let sheen = if mat.sheen <= 0.0 {
        splat(0.0)
    } else {
        let f = pow(1.0 - saturate(-(dot(n, rd))), 4.0);
        let lit = 0.3 * occ + 0.7 * moon_shadow * saturate(dot(n, pose.moon) + 0.4);
        scale(mat.sheen * f * lit, scale(2.2, mat.albedo) + vec(0.05, 0.05, 0.1))
    };
    // hair: a Kajiya-Kay highlight, a band running across the strands
    let hair_spec = if m != m_beard {
        splat(0.0)
    } else {
        let hh = normalise(pose.moon - rd);
        let th = dot(hair_tangent(), hh);
        let k = pow(0f32.maximum(1.0 - th * th).sqrt(), 60.0);
        scale(0.35 * k * moon_shadow * saturate(dot(n, pose.moon) + 0.2), vec(0.75, 0.8, 1.0))
    };
    let c = lit + fill + rim + mirror + sheen + hair_spec + mat.emission;
    let fog = fog_at(pose.prm, t);
    match pose.prm.view {
        1 => scale(0.5, n + splat(1.0)),
        2 => mat.albedo,
        3 => splat(occ),
        _ => mix(c, sky(pose, to_world_dir(pose.fl, rd)), fog),
    }
}

// ---------------------------------------------------------------------
// The glass orb

// (whether the orb reflects, and its index of refraction, are uniforms:
// orb_reflections and orb_ior in params.json)
pub const glass_tint: Vec3 = Vec3::new(0.93, 0.97, 1.0);

// Distances (entry, exit) where the ray meets the orb; entry > exit on a miss.
#[inline]
pub fn orb_hit(pose: Pose, ro: Vec3, rd: Vec3) -> (f32, f32) {
    ray_sphere(pose.orb, orb_radius, ro, rd)
}

// What a glass surface at p, met as fr describes, reflects towards the
// eye: the sky, and (by `shine`) highlights of the moon and the orb.
pub fn glass_reflection(pose: Pose, g: Glow, shine: f32, p: Vec3, fr: Interface) -> Vec3 {
    let rr = fr.reflected;
    let to_orb = normalise(pose.orb - p);
    let highlights = scale(2.0 * shine * pow(saturate(dot(rr, pose.moon)), 300.0), vec(0.8, 0.85, 1.0))
        + scale(2.5 * shine * g.level * pow(saturate(dot(rr, to_orb)), 120.0), vec(0.7, 0.85, 1.0));
    scale(fr.f, sky(pose, to_world_dir(pose.fl, rr))) + highlights
}

pub const potion_ior: f32 = 1.5;

// The potions' glass, a thin wall rays pass through. Finds where the ray
// (up to t_end) first meets it and returns what the wall adds there (its
// reflection and a faint rim) and what it lets through (what neither of
// its faces reflects, slightly tinted). Only rays passing near the potions
// are traced.
#[inline(never)] // (few call sites: the whole pose is passed)
pub fn glass(pose: Pose, g: Glow, ro: Vec3, rd: Vec3, t_end: f32) -> (Vec3, Vec3) {
    // the potions' bounding sphere, in local space (body space leans)
    let c = from_body_space(pose, potions_at(gear_of(pose.prm)));
    let (enter, exit) = ray_sphere(c, potions_radius + 0.02, ro, rd);
    if enter > exit {
        return (splat(0.0), splat(1.0));
    }
    let (hit, t, n) = glass_hit(move |q| potion_glass(pose, q), 64, ro, rd, 0.02f32.maximum(enter), t_end.minimum(exit));
    if !hit {
        return (splat(0.0), splat(1.0));
    }
    let p = ro + scale(t, rd);
    let fr = fresnel_at(potion_ior, rd, n);
    let cosi = saturate(-(dot(fr.n, rd)));
    let added = glass_reflection(pose, g, 1.0, p, fr) + scale(0.08 * pow(1.0 - cosi, 2.0), vec(0.6, 0.7, 0.9));
    (added, scale((1.0 - fr.f) * (1.0 - fr.f), vec(0.9, 0.95, 0.97)))
}

// ---------------------------------------------------------------------
// The shield: a bubble round him, raised from the harness (pose.shield,
// 0 down .. 1 up; web/shield.ts). Glass-like (each face reflects by
// Fresnel), it glows at its rim and in a faint honeycomb, and flares and
// ripples each time it stops a spell (pose.shield_hit: the seconds since).

pub const shield_centre: Vec3 = Vec3::new(0.0, 0.85, 0.0);
pub const shield_radius: f32 = 1.85;
pub const shield_ior: f32 = 1.33;

pub fn shield_sdf(pose: Pose, p: Vec3) -> f32 {
    let q = p - shield_centre;
    let r = shield_radius * (0.25 + 0.75 * smoothstep(0.0, 1.0, pose.shield));
    // (the ripple's slope stays under 0.25, so the field stays safe to trace)
    let ripple = 0.02 * (-4.0 * pose.shield_hit).exp() * (12.0 * q.y - 40.0 * pose.shield_hit).sin();
    length(q) - r - ripple
}

// Distance to the edges of a grid of hexagons (apothem 0.5, one across
// per unit of u).
pub fn hex_edge(u: f32, v: f32) -> f32 {
    let m = |a: f32, b: f32| a - b * (a / b).floor();
    let h = 0.8660254; // (rows are sqrt 3 apart)
    let (ax, ay) = (m(u, 1.0) - 0.5, m(v, 2.0 * h) - h);
    let (bx, by) = (m(u - 0.5, 1.0) - 0.5, m(v - h, 2.0 * h) - h);
    let (gx, gy) = if ax * ax + ay * ay < bx * bx + by * by { (ax, ay) } else { (bx, by) };
    0.5 - (0.5 * gx.abs() + h * gy.abs()).maximum(gx.abs())
}

#[inline(never)] // (few call sites: the whole pose is passed)
pub fn shield_layer(pose: Pose, g: Glow, ro: Vec3, rd: Vec3, t_end: f32) -> (Vec3, Vec3) {
    let (enter, exit) = ray_sphere(shield_centre, shield_radius + 0.05, ro, rd);
    if pose.shield <= 0.005 || enter > exit || exit < 0.0 {
        return (splat(0.0), splat(1.0));
    }
    let sdf = move |q| shield_sdf(pose, q);
    let t1 = t_end.minimum(exit);
    // what one face adds: its reflection, and its glow
    let face = move |t: f32, n: Vec3| -> (Vec3, f32) {
        let p = ro + scale(t, rd);
        let fr = fresnel_at(shield_ior, rd, n);
        let q = normalise(p - shield_centre);
        // (30 cells round, so the grid meets itself)
        let k = 30.0 / (2.0 * PI);
        let cells = smoothstep(0.05, 0.0, hex_edge(k * q.z.atan2(q.x), k * q.y.asin())) * keep(t * pose.pix, 0.2);
        let rim = pow(1.0 - saturate(-(dot(fr.n, rd))), 3.0);
        let flare = 2.0 * (-5.0 * pose.shield_hit).exp();
        let glow = scale(pose.shield * (0.02 + 0.6 * rim + 0.09 * cells + flare * (0.15 + rim)), shield_colour);
        // (an energy skin, not glass: sharp highlights of the moon and the
        // orb, which is inside, only made blots on it)
        (glass_reflection(pose, g, 0.1, p, fr) + glow, 1.0 - fr.f)
    };
    // the near face, then the far one seen through it (a thin skin: each
    // face reflects, neither bends the ray)
    let (front, tf, nf) = glass_hit(sdf, 32, ro, rd, 0.02f32.maximum(enter), t1);
    if !front {
        return (splat(0.0), splat(1.0));
    }
    let (a1, k1) = face(tf, nf);
    let (back, tb, nb) = glass_hit(sdf, 32, ro, rd, tf + 0.02, t1);
    let (a2, k2) = if back { face(tb, nb) } else { (splat(0.0), 1.0) };
    (a1 + scale(k1, a2), splat(k1 * k2))
}

// Where a reflected path starts: (exists, point, direction, weight).
pub type Reflection = (bool, Vec3, Vec3, f32);

// Follows a ray through at most three straight segments - outside, inside
// the orb, outside again - refracting at the glass, and returns the colour
// it gathers, weighted by w. Also returns the reflection spawned where the
// ray first enters the orb, and how far the ray sees before anything opaque
// or the orb's glass (infinity if nothing).
#[inline(never)] // (few call sites: the whole pose is passed)
pub fn follow(pose: Pose, g: Glow, ro0: Vec3, rd0: Vec3, w0: f32) -> (Vec3, Reflection, f32) {
    let (mut ro, mut rd, mut w, mut acc, mut inside, mut done, mut refl, mut first): (Vec3, Vec3, Vec3, Vec3, bool, bool, Reflection, f32) =
        (ro0, rd0, splat(w0), splat(0.0), false, false, (false, ro0, rd0, 0f32), INF);
    let mut seg = 0;
    while seg < 3 {
        let next: (Vec3, Vec3, Vec3, Vec3, bool, bool, Reflection, f32) = if done {
            (ro, rd, w, acc, inside, done, refl, first)
        } else {
            let vis = pose.prm.layers;
            // (a hidden orb is no glass at all)
            let (t_in, t_out) = if has(vis, layers_orb) { orb_hit(pose, ro, rd) } else { (INF, -INF) };
            let enters = !inside && t_in > 1e-3 && t_in < t_out;
            let tmax = if inside { t_out } else if enters { t_in } else { pose.prm.view_distance };
            // Two passes: the terrain alone over the whole segment, then the
            // scene near the wizard up to the terrain hit or the edge of the
            // sphere that holds it (the peaks reach into that sphere, so the
            // terrain goes first). Neither pass evaluates the other's
            // geometry at every step.
            let (t_terrain, m_land) =
                if inside || enters || !(has(vis, land_layers)) { (INF, 0) } else { march_terrain(vis, pose, ro, rd, tmax) };
            // the enemies, in front of the land (a pass of their own, which
            // most rays skip: see march_enemies)
            let (t_terrain, m_land) = if inside || enters || !(has(vis, layers_enemies)) || pose.prm.enemies <= 0 {
                (t_terrain, m_land)
            } else {
                let (te, me) = march_enemies(pose.prm, pose.foes, pose.secs, pose.fl, ro, rd, tmax.minimum(t_terrain));
                if te < t_terrain { (te, me) } else { (t_terrain, m_land) }
            };
            let local_end = 0f32.maximum(ray_sphere(occluder_centre, local_radius, ro, rd).1);
            let local_max = tmax.minimum(t_terrain.minimum(local_end));
            let (t, m, emitted, tr) = march(move |tol, q| local_scene(vis, pose, tol, q), move |q| media(pose, g, q), ro, rd, local_max);
            let (t, m) = if m < 0 && t_terrain < tmax { (t_terrain, m_land) } else { (t, m) };
            // the glows are above the cloud slab, so in front of the clouds
            let (cloud_rad, cloud_tr) =
                if has(vis, layers_clouds) { clouds(pose, ro, rd, if m >= 0 { t } else { tmax }) } else { (splat(0.0), 1.0) };
            // the potions' glass, in front of whatever this segment reaches
            let (glass_added, glass_through) = if inside || !(has(vis, layers_gear)) {
                (splat(0.0), splat(1.0))
            } else {
                glass(pose, g, ro, rd, if m >= 0 { t } else { local_max })
            };
            let acc = acc + (w * glass_added);
            let w = w * glass_through;
            // his shield, if raised (outside the orb, the potions and him)
            let (shield_added, shield_through) =
                if inside { (splat(0.0), splat(1.0)) } else { shield_layer(pose, g, ro, rd, if m >= 0 { t } else { tmax }) };
            let acc = acc + (w * shield_added);
            let w = w * shield_through;
            let first = if seg > 0 { first } else if m >= 0 { t } else if enters { t_in } else { INF };
            let acc = acc + (w * emitted) + (scale(tr, w) * cloud_rad);
            // whatever lies beyond this segment is seen through its media
            let w = scale(tr * cloud_tr, w);
            if m >= 0 {
                (ro, rd, w, acc + (w * surface(pose, g, m, ro + scale(t, rd), rd, t)), inside, true, refl, first)
            } else if enters && t >= t_in {
                // into the glass
                let p = ro + scale(t_in, rd);
                let n = normalise(p - pose.orb);
                let fr = fresnel_at(pose.prm.orb_ior, rd, n);
                let refl = if refl.0 { refl } else { (true, p, fr.reflected, w.x * fr.f) };
                (p, fr.refracted, scale(1.0 - fr.f, w * glass_tint), acc, true, false, refl, first)
            } else if inside && t >= t_out {
                // out of the glass
                let p = ro + scale(t_out, rd);
                let n = normalise(p - pose.orb);
                (p, fresnel_at(1.0 / pose.prm.orb_ior, rd, n).refracted, w * glass_tint, acc, false, false, refl, first)
            } else {
                (ro, rd, w, acc + (w * sky(pose, to_world_dir(pose.fl, rd))), inside, true, refl, first)
            }
        };
        ro = next.0;
        rd = next.1;
        w = next.2;
        acc = next.3;
        inside = next.4;
        done = next.5;
        refl = next.6;
        first = next.7;
        seg += 1;
    }
    (acc, refl, first)
}

// The colour seen along a camera ray, and how far it sees (see follow).
// (#[noinline]: render calls it from more than one kernel when it
// anti-aliases, which would otherwise each hold a copy of all the shading.)
#[inline(never)]
pub fn shade(prm: Params, dr: Drive, secs: f32, pix: f32, ro: Vec3, rd: Vec3) -> (Vec3, f32) {
    // worked out once per pixel, not per march step
    let pose = Pose { pix, ..pose_at(secs, prm, dr) };
    let g = glow_now(pose.tm);
    // Path 0 is the camera ray; path 1 its reflection off the orb, if any.
    // A loop, so that follow (and all the shading) is compiled only once.
    let paths = if prm.orb_reflections { 2 } else { 1 };
    let (mut c, mut refl, mut first): (Vec3, Reflection, f32) = (splat(0.0), (false, ro, rd, 0f32), INF);
    let mut k = 0;
    while k < paths {
        let next: (Vec3, Reflection, f32) = if k == 1 && !refl.0 {
            (c, refl, first)
        } else {
            let (o, d, w) = if k == 0 { (ro, rd, 1.0) } else { (refl.1, refl.2, refl.3) };
            let (col, r, f) = follow(pose, g, o, d, w);
            (c + col, if k == 0 { r } else { refl }, if k == 0 { f } else { first })
        };
        c = next.0;
        refl = next.1;
        first = next.2;
        k += 1;
    }
    (c, first)
}
