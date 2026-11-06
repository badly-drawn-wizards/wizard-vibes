// fut/textures.fut: surface textures: bark on the staff, cloth on the robe,
// leather on the belt; bark, leaves and needles on the trees. Each material has a height field (in world units,
// along the normal) that tilts the shading normal (bump mapping), and colour
// variation.
//
// Fine detail aliases once a pixel covers more than a fraction of a
// wavelength, so every fine term is faded by the pixel's footprint on the
// surface, fp (distance times the angle one pixel spans).

use crate::prelude::*;

// 1 while the footprint is under a quarter wavelength, 0 beyond a half.
#[inline]
pub fn keep(fp: f32, wavelength: f32) -> f32 {
    smoothstep(0.5 * wavelength, 0.25 * wavelength, fp)
}

// ---------------------------------------------------------------------
// Bark: plates separated by furrows, running along the staff. The plates
// are contour bands of a two-octave 3D noise stretched along the axis (so
// the contours run lengthwise, with no seam around the staff); finer cracks
// on top.
//
// Bark comes in three scales, each fading only once a pixel covers it, so
// the staff keeps a bark look at every distance rather than averaging to a
// flat brown: fine plates (~0.005 across), broad plates and furrows
// (~0.025), and lighter and darker patches along the length (never faded).

// One scale of plates: (plate in [0, 1]; 0 in a furrow, faded to 0.5 once
// too small to show). `across`/`along` are noise frequencies per unit;
// `bands` sets how many plates per unit of noise.
#[inline]
pub fn plates(fp: f32, wavelength: f32, across: f32, along: f32, bands: f32, q: Vec3) -> f32 {
    let s = vec(q.x * across, q.y * along, q.z * across);
    let n = 0.65 * vnoise3(s) + 0.35 * vnoise3(scale(2.3, s));
    let band = fract(n * bands);
    mix1(0.5, 0.5 - 0.5 * (2.0 * PI * band).cos(), keep(fp, wavelength))
}

// (height, fine plate, broad plate, patch tone)
#[inline(never)]
pub fn bark(fp: f32, p: Vec3) -> (f32, f32, f32, f32) {
    let q = staff_local(p);
    // the noise changes by ~80 per unit across the staff at the fine scale
    // and ~25 at the broad one; these amplitudes keep each bump's slope
    // near 0.5
    let fine = plates(fp, 0.005, 75.0, 6.0, 3.0, q);
    let broad = plates(fp, 0.025, 25.0, 2.5, 2.0, q);
    let tone = vnoise3(vec(q.x * 8.0, q.y * 1.6 + 3.1, q.z * 8.0));
    let cracks = vnoise3(vec(q.x * 200.0, q.y * 30.0, q.z * 200.0)) * keep(fp, 0.003);
    (0.0008 * fine + 0.002 * broad + 0.0002 * cracks, fine, broad, tone)
}

pub fn bark_albedo(_prm: Params, fp: f32, p: Vec3) -> Vec3 {
    let (_, fine, broad, tone) = bark(fp, p);
    let q = staff_local(p);
    // Bark reads as narrow dark crevices between lighter plates: darken only
    // near each plate's edge (plate values under ~0.2), at both scales.
    let crevice = |v: f32| smoothstep(0.2, 0.0, v);
    let wood = wood_colour;
    let base = scale((1.0 - 0.85 * crevice(broad)) * (1.0 - 0.6 * crevice(fine)), wood);
    // lighter and darker stretches along the length
    let base = scale(0.7 + 0.6 * tone, base);
    // a few grey-green patches of lichen
    let lichen = smoothstep(0.8, 0.86, vnoise3(vec(q.x * 60.0, q.y * 9.0, q.z * 60.0)));
    mix(base, vec(0.3, 0.33, 0.25), 0.6 * lichen)
}

// ---------------------------------------------------------------------
// Cloth: soft vertical folds, deeper towards the hem, and (close up) a
// basket weave. Measured in the robe's own space, so it leans with him.

#[inline]
pub fn cloth_coords(pose: Pose, p: Vec3) -> (f32, f32, f32) {
    // (angle, arc length, height)
    let b = body_space(pose, p);
    let a = b.z.atan2(b.x);
    (a, a * length2(vec2(b.x, b.z)), b.y)
}

#[inline]
pub fn fold(a: f32, y: f32) -> f32 {
    (a * 13.0 + 0.7 * (a * 5.0 + y * 2.0).sin()).sin() * (0.25 + 0.75 * saturate(1.0 - y / 1.1))
}

// (the patterns below are real GPU functions taking the point in their own
// space, which the function after each works out from the pose: the bump
// mapping evaluates them four times a pixel)
#[inline(never)]
pub fn cloth_pattern(fp: f32, a: f32, u: f32, y: f32) -> f32 {
    // basket weave: in alternate cells the warp (running up) or the weft
    // (running round) is on top, each a rounded thread
    let weave_w = 0.008;
    let cu = (u / weave_w).floor();
    let cy = (y / weave_w).floor();
    let warp_on_top = imod(ftoi(cu).wrapping_add(ftoi(cy)), 2) == 0;
    let thread = |x: f32| (PI * fract(x / weave_w)).sin();
    let weave = if warp_on_top { thread(u) } else { thread(y) };
    0.012 * fold(a, y) + 0.00025 * weave * keep(fp, 2.0 * weave_w)
}

#[inline]
pub fn cloth(pose: Pose, fp: f32, p: Vec3) -> f32 {
    let (a, u, y) = cloth_coords(pose, p);
    cloth_pattern(fp, a, u, y)
}

pub fn cloth_albedo(pose: Pose, p: Vec3) -> Vec3 {
    let (a, _, y) = cloth_coords(pose, p);
    let mottle = 0.9 + 0.2 * vnoise3(scale(25.0, p));
    // the creases of the folds catch less light and look deeper in colour
    scale(mottle * (0.82 + 0.18 * (0.5 + 0.5 * fold(a, y))), pose.prm.robe_colour)
}

// ---------------------------------------------------------------------
// Leather: a pebbled grain and, on the belt, a row of stitches along each
// edge, with the edges worn darker.

// (arc length round the body, height above the belt's centre, on the belt?)
pub fn belt_coords(pose: Pose, p: Vec3) -> (f32, f32, bool) {
    let b = body_space(pose, p);
    let prm = pose.prm;
    let dy = b.y - prm.belt_height;
    let r = length2(vec2(b.x, b.z));
    (
        b.x.atan2(b.z) * robe_radius(gear_of(prm), prm.belt_height),
        dy,
        dy.abs() < 0.035 && (r - robe_radius(gear_of(prm), b.y)).abs() < 0.03,
    )
}

// How much of a stitch is at this point, in [0, 1].
#[inline]
pub fn stitch(u: f32, dy: f32) -> f32 {
    let across = (dy - 0.017).abs().minimum((dy + 0.017).abs());
    let phase = fract(u / 0.011);
    let along = if phase < 0.65 { (PI * phase / 0.65).sin().sqrt() } else { 0.0 };
    along * smoothstep(0.0026, 0.0008, across)
}

#[inline(never)]
pub fn leather_pattern(fp: f32, u: f32, dy: f32, on_belt: bool, p: Vec3) -> f32 {
    let g = vnoise3(scale(85.0, p));
    let grain = 0.0006 * g * g * keep(fp, 0.012);
    let st = if on_belt { 0.0011 * stitch(u, dy) * keep(fp, 0.011) } else { 0.0 };
    grain + st
}

#[inline]
pub fn leather(pose: Pose, fp: f32, p: Vec3) -> f32 {
    let (u, dy, on_belt) = belt_coords(pose, p);
    leather_pattern(fp, u, dy, on_belt, p)
}

pub fn leather_albedo(pose: Pose, fp: f32, p: Vec3) -> Vec3 {
    let (u, dy, on_belt) = belt_coords(pose, p);
    let g = vnoise3(scale(85.0, p));
    let base = scale(0.75 + 0.35 * mix1(0.6, g, keep(fp, 0.012)), leather_colour);
    if !on_belt {
        base
    } else {
        let worn = mix1(1.0, 0.6, smoothstep(0.018, 0.03, dy.abs()));
        let s = stitch(u, dy) * keep(fp, 0.011);
        mix(scale(worn, base), vec(0.55, 0.45, 0.3), s)
    }
}

// ---------------------------------------------------------------------
// Hair: fine strands within each beard lock, faded by footprint, and the
// locks' colours varying from white through cream to light grey.

#[inline(never)]
pub fn hair_pattern(fp: f32, b: Vec3) -> f32 {
    let l = beard_lock(b);
    // six strands per lock; a lock is ~0.035 across, so ~0.006 per strand
    let strand = (PI * fract(l.phase * 6.0 + 0.3 * (l.h * 20.0).sin())).sin().abs();
    0.0003 * strand * keep(fp, 0.006)
}

#[inline]
pub fn hair(pose: Pose, fp: f32, p: Vec3) -> f32 {
    hair_pattern(fp, beard_space(pose, p))
}

pub fn hair_albedo(pose: Pose, p: Vec3) -> Vec3 {
    let l = beard_lock(beard_space(pose, p));
    let tint = pose.prm.beard_tint * mix(vec(0.93, 0.92, 0.88), vec(0.78, 0.79, 0.82), l.rnd);
    scale(0.62 + 0.38 * l.crown, tint)
}

// Direction the hair runs (down the beard), in the wizard's local space.
// (computed, so a function rather than a const)
#[inline]
pub fn hair_tangent() -> Vec3 {
    normalise(beard_tip - beard_top)
}

// ---------------------------------------------------------------------
// Eyes: sclera, an icy blue iris with a darker rim, and a pupil, laid out
// by the angle from each eye's gaze. The eyes glance further than the head
// turns when he points, or as he looks the camera's way.

pub fn eye_albedo(pose: Pose, p: Vec3) -> Vec3 {
    let q = head_space(pose, p);
    let side = if q.x < 0.0 { -1.0 } else { 1f32 };
    let c = vec(side * eye_centre.x, eye_centre.y, eye_centre.z);
    let dir = normalise(q - c);
    let gaze = normalise(yaw(pose.glance, tilt_up(pose.glance_up, vec(0.0, -0.08, 1.0))));
    let cosa = dot(dir, gaze);
    let iris = smoothstep(0.86, 0.875, cosa);
    let pupil = smoothstep(0.955, 0.965, cosa);
    let rim = 1.0 - 0.5 * smoothstep(0.92, 0.88, cosa); // darker towards the iris's edge
    let sclera = vec(0.9, 0.88, 0.85);
    let blue = scale(rim, iris_colour);
    mix(mix(sclera, blue, iris), splat(0.02), pupil)
}

// ---------------------------------------------------------------------
// Hat stars: five-pointed stars stitched onto the cone, standing slightly
// proud of it.

// Signed distance to a five-pointed star with outer radius r and inner
// radius rf * r, pointing up (after Inigo Quilez).
pub fn sd_star5(x: f32, y: f32, r: f32, rf: f32) -> f32 {
    let (k1x, k1y) = (0.809016994f32, -0.587785252f32);
    let (k2x, k2y) = (-k1x, k1y);
    let x = x.abs();
    let d1 = 2.0 * (k1x * x + k1y * y).maximum(0.0);
    let (x, y) = (x - d1 * k1x, y - d1 * k1y);
    let d2 = 2.0 * (k2x * x + k2y * y).maximum(0.0);
    let (x, y) = (x - d2 * k2x, y - d2 * k2y);
    let x = x.abs();
    let y = y - r;
    let (bax, bay) = (rf * -k1y, rf * k1x - 1.0);
    let h = clamp(0.0, r, (x * bax + y * bay) / (bax * bax + bay * bay));
    let dx = x - bax * h;
    let dy = y - bay * h;
    (dx * dx + dy * dy).sqrt() * sgn(y * bax - x * bay)
}

// (coverage in [0, 1], height above the cloth). The pattern lives in the
// same bent space as the cone's SDF, so it follows the drooping tip. Rows of
// square cells run up the cone, each row with as many cells as fit around
// it, and the star is measured in arc length, so stars keep their shape as
// the cone narrows. Each star has its own size and tilt; its edge is
// antialiased by the pixel footprint fp, and its bevel fades out with it.
// (bp: in the bent hat's space; y0: the band's top)
#[inline(never)]
pub fn hat_star_pattern(fp: f32, y0: f32, bp: Vec3) -> (f32, f32) {
    let cell = 0.085;
    let y0 = y0 + 0.012; // start above the band
    let v = (bp.y - y0) / cell;
    let cv = v.floor();
    let row_radius = hat_radius(y0 + (cv + 0.5) * cell);
    let cells = (1.0f32).maximum(round_even(2.0 * PI * row_radius / cell));
    let u = (bp.z.atan2(bp.x) / (2.0 * PI) + 0.5) * cells;
    let cu = u.floor();
    let iu = ftoi(cu);
    let iv = ftoi(cv);
    if bp.y < y0 || row_radius < 0.035 || hash2(iu, iv) >= 0.55 {
        return (0.0, 0.0);
    }
    let ox = 0.32 + 0.36 * hash2(iu + 17, iv);
    let oy = 0.32 + 0.36 * hash2(iu, iv + 31);
    // arc length per u unit at this point, in cell units
    let stretch = 2.0 * PI * length2(vec2(bp.x, bp.z)) / cells / cell;
    let tilt = 0.5 * (hash2(iu + 3, iv + 5) - 0.5);
    let size = 0.22 + 0.1 * hash2(iu + 11, iv + 7);
    let r = rot2(tilt, vec2((u - cu - ox) * stretch, v - cv - oy));
    let d = sd_star5(r.x, r.y, size, 0.45); // in cell units
    let aa = (0.02f32).maximum(fp / cell);
    let cover = smoothstep(aa, -aa, d);
    let height = 0.0025 * smoothstep(0.01, -0.06, d) * keep(fp, 0.01);
    (cover, height)
}

#[inline]
pub fn hat_stars(pose: Pose, fp: f32, p: Vec3) -> (f32, f32) {
    hat_star_pattern(fp, band_top(pose.prm), bend(pose.prm.hat_droop, head_space(pose, p) - hat_base))
}

// ---------------------------------------------------------------------
// Cork: tan, with dark speckled pores (small pits in the bump).

#[inline(never)]
pub fn cork_pores(fp: f32, p: Vec3) -> f32 {
    smoothstep(0.7, 0.82, vnoise3(scale(420.0, p))) * keep(fp, 0.004)
}

#[inline]
pub fn cork(fp: f32, p: Vec3) -> f32 {
    -0.0002 * cork_pores(fp, p)
}

pub fn cork_albedo(fp: f32, p: Vec3) -> Vec3 {
    let mottle = 0.85 + 0.3 * vnoise3(scale(120.0, p));
    scale(mottle * (1.0 - 0.45 * cork_pores(fp, p)), vec(0.62, 0.4, 0.2))
}

// ---------------------------------------------------------------------
// The rock's top: flagstones laid in rings round the centre, each a little
// different in shade, with mossy grooves between them.

// (distance to the stone's edge, the stone's hash) at a point of the top
pub fn flagstone(p: Vec3) -> (f32, f32) {
    let ring = 0.32;
    let r = length2(vec2(p.x, p.z));
    let ri = (r / ring).floor();
    let n = (2.0f32).maximum((2.0 * PI * (ri + 0.5) * ring / 0.42).floor());
    let a = (p.z.atan2(p.x) / (2.0 * PI) + 0.5) * n + 0.7 * hash2(ftoi(ri), 5);
    let ai = a.floor();
    let (fr, fa) = (r / ring - ri, a - ai);
    let edge = (fr.minimum(1.0 - fr) * ring).minimum(fa.minimum(1.0 - fa) * 2.0 * PI * r.maximum(0.05) / n);
    (edge, hash2(ftoi(ri).wrapping_mul(7).wrapping_add(3), ftoi(ai - n * (ai / n).floor())))
}

pub fn flagstone_height(fp: f32, p: Vec3) -> f32 {
    let (edge, _) = flagstone(p);
    (-0.008 * smoothstep(0.025, 0.004, edge) + 0.0015 * vnoise3(scale(25.0, p))) * keep(fp, 0.03)
}

// ---------------------------------------------------------------------
// The ice and the carpet (see platform.rs).

// Clear blue ice, deeper and darker towards the point, with pale cracks.
pub fn ice_albedo(base: Vec3, fp: f32, p: Vec3) -> Vec3 {
    let deep = smoothstep(0.0, -2.5, p.y);
    let crack = smoothstep(0.035, 0.0, (vnoise3(scale(2.5, p)) - 0.5).abs()) * keep(fp, 0.08);
    mix(mix(base, base * vec(0.3, 0.45, 0.7), deep), vec(0.8, 0.9, 1.0), 0.7 * crack)
}

// Its top: frost, patchy and sparkling a little.
pub fn frost_albedo(base: Vec3, fp: f32, p: Vec3) -> Vec3 {
    let patch = vnoise3(scale(3.0, p));
    let sparkle = smoothstep(0.82, 0.9, vnoise3(scale(40.0, p))) * keep(fp, 0.05);
    mix(mix(base, vec(0.75, 0.85, 0.95), 0.4 + 0.5 * patch), vec(1.0, 1.0, 1.0), sparkle)
}

// A woven rug: a field of `field` with a lattice of darker lines and a
// dark blue medallion in the middle, inside a border of `trim` with a
// zigzag, edged with cream lines; the tassels cream. In the carpet's
// frame (at the platform's origin, x across, z along).
pub fn carpet_albedo(field: Vec3, trim: Vec3, fp: f32, p: Vec3) -> Vec3 {
    let cream = vec(0.85, 0.75, 0.55);
    let navy = vec(0.04, 0.06, 0.2);
    let (ax, az) = (p.x.abs(), p.z.abs());
    if az >= 1.6 {
        return cream;
    }
    // (lines widen to the pixel's footprint, fading as they do)
    let w = (0.012f32).maximum(fp);
    let fade = 0.012 / w;
    let line = |d: f32| fade * smoothstep(w, 0.5 * w, d.abs());
    let e = (1.15 - ax).minimum(1.6 - az);
    // the border: along whichever edge is nearer, a zigzag
    let along = if 1.15 - ax < 1.6 - az { p.z } else { p.x };
    let zig = 0.07 + 0.06 * (2.0 * fract(along * 5.0) - 1.0).abs();
    let border = smoothstep(0.2 + 0.5 * w, 0.2 - 0.5 * w, e);
    let band = mix(trim, field, smoothstep(zig - 0.5 * w, zig + 0.5 * w, e) * smoothstep(zig + 0.05, zig + 0.05 - w, e));
    // the field: a lattice and the medallion
    let lattice = line(fract((p.x + p.z) * 3.0) - 0.5).maximum(line(fract((p.x - p.z) * 3.0) - 0.5));
    let dm = ax / 0.75 + az / 1.1;
    let c = mix(field, scale(0.55, field), lattice);
    let c = mix(c, navy, smoothstep(0.55 + 0.5 * w, 0.55 - 0.5 * w, dm));
    let c = mix(c, trim, smoothstep(0.2 + 0.5 * w, 0.2 - 0.5 * w, dm));
    let c = mix(c, cream, line(dm - 0.55).maximum(line(dm - 0.6)));
    let c = mix(c, band, border);
    let c = mix(c, cream, line(e - 0.03).maximum(line(e - 0.2)));
    // the pile
    scale(0.85 + 0.3 * vnoise3(scale(30.0, p)) * keep(fp, 0.04), c)
}

// ---------------------------------------------------------------------
// The terrain up close: rough rock, and grass in tufts of blades. The
// normal of the land itself comes from its height (see surface); this
// tilts it by detail too fine for that, fading as the footprint grows.

// (in world coordinates)
pub fn grass_blades(fp: f32, w: Vec3) -> f32 {
    // tufts, and the blades in them: narrow ridges across a warped field
    let tuft = vnoise2(w.x * 3.1, w.z * 3.1);
    let warp = 2.5 * vnoise2(w.x * 9.0, w.z * 9.0);
    let blade = 1.0 - (2.0 * fract(w.x * 41.0 + w.z * 13.0 + warp) - 1.0).abs();
    let cross = 1.0 - (2.0 * fract(w.z * 37.0 - w.x * 11.0 + 1.7 * warp) - 1.0).abs();
    tuft * (blade * blade * keep(fp, 0.024) + cross * cross * keep(fp, 0.027))
}

// (2D noise, cheap enough for nearly every pixel of the frame, four times
// over; the grass only where it shows)
pub fn terrain_bump(fp: f32, w: Vec3) -> f32 {
    0.03 * vnoise2(w.x * 4.0 + 0.4 * w.y, w.z * 4.0 - 0.4 * w.y) * keep(fp, 0.25)
        + (if fp < 0.014 { 0.006 * grass_blades(fp, w) } else { 0.0 })
}

// ---------------------------------------------------------------------
// Trees, in world space (the trunks and branches mostly stand up, so bark
// runs along y). Each fades by the footprint as the rest do; a far tree is
// its colours alone.

// Bark: plates and furrows stretched up the trunk (about 0.06 across),
// finer cracks, and lighter and darker patches: (height, plate, tone).
#[inline(never)]
pub fn tree_bark(fp: f32, w: Vec3) -> (f32, f32, f32) {
    let plate = plates(fp, 0.06, 16.0, 2.2, 2.0, w);
    let cracks = vnoise3(vec(w.x * 70.0, w.y * 9.0, w.z * 70.0)) * keep(fp, 0.012);
    let tone = vnoise3(vec(w.x * 3.0, w.y * 0.8, w.z * 3.0));
    (0.006 * plate + 0.0015 * cracks, plate, tone)
}

// Leaves: a 3D cellular pattern, a leaf to a cell (about 0.05 across), each
// stretched along a direction of its own (so leaves, not tiles), domed
// (high at its middle, low at its edges) and tilted along its length (so
// each takes the light its own way), its own shade, with a midrib
// along that direction: (height, edge (1 on an edge), shade (0..1), vein
// (1 on the midrib)). Too small to show, none of it.
#[inline(never)]
pub fn leaf_cells(fp: f32, w: Vec3) -> (f32, f32, f32, f32) {
    let k = keep(fp, 0.05);
    if k <= 0.0 {
        return (0.0, 0.0, 0.5, 0.0);
    }
    let q = scale(20.0, w);
    let (cx, cy, cz) = (q.x.floor(), q.y.floor(), q.z.floor());
    let (mut f1, mut f2) = (9.0f32, 9.0f32);
    let (mut id, mut near, mut tilt) = (0.5f32, splat(0.0), 0f32);
    let mut i = 0;
    while i < 27 {
        let (dx, dy, dz) = (imod(i, 3) - 1, imod(idiv(i, 3), 3) - 1, idiv(i, 9) - 1);
        let (ix, iy, iz) = (ftoi(cx) + dx, ftoi(cy) + dy, ftoi(cz) + dz);
        let c = vec(ix as f32 + 0.15 + 0.7 * hash3(ix, iy, iz), iy as f32 + 0.15 + 0.7 * hash3(iy, iz, ix), iz as f32 + 0.15 + 0.7 * hash3(iz, ix, iy));
        // (the leaf's direction, and the distance squeezed along it: an
        // ellipse twice as long as wide)
        let a = 2.0 * PI * hash3(ix - 5, iy + 2, iz + 9);
        let dir = normalise(vec(a.cos(), 0.6 * hash3(iz, iy, ix) - 0.3, a.sin()));
        let d = q - c;
        let along = dot(d, dir);
        let d = d - scale(0.5 * along, dir);
        let d2 = dot(d, d);
        if d2 < f1 {
            f2 = f1;
            f1 = d2;
            id = hash3(ix + 7, iy - 3, iz + 11);
            tilt = hash3(iy + 4, ix - 6, iz + 1) - 0.5;
            near = vec(along, length(d - scale(dot(d, dir), dir)), 0.0);
        } else if d2 < f2 {
            f2 = d2;
        }
        i += 1;
    }
    let (f1, f2) = (f1.sqrt(), f2.sqrt());
    let edge = smoothstep(0.15, 0.0, f2 - f1) * k;
    // (the midrib: the line through the leaf's middle along its direction;
    // near.y is the distance from it)
    let vein = smoothstep(0.05, 0.0, near.y) * smoothstep(0.7, 0.2, f1) * keep(fp, 0.02);
    // (each leaf also tilted along its length, by its own amount: some
    // catch the light, some do not; the tilt fading out at its edge, where
    // the next leaf's starts, so the height has no step there to sparkle)
    let inner = smoothstep(0.0, 0.25, f2 - f1);
    (0.004 * k * ((1.0 - f1 * f1).maximum(0.0) + 1.6 * tilt * near.x * inner), edge, id, vein)
}

// Needles: streaks running down and round each tier, broken into bunches
// (a noise's patches) with darker gaps between: (height, streak (1 along a
// needle, 0 in a gap)).
#[inline(never)]
pub fn needle_streaks(fp: f32, w: Vec3) -> (f32, f32) {
    let warp = 2.0 * vnoise3(scale(5.0, w));
    let a = fract(w.x * 26.0 + w.z * 21.0 + 9.0 * w.y + warp);
    let bunch = smoothstep(0.3, 0.65, vnoise3(vec(w.x * 9.0, w.y * 5.0, w.z * 9.0)));
    let streak = (1.0 - (2.0 * a - 1.0).abs()) * bunch * keep(fp, 0.04);
    (0.004 * streak * streak, streak)
}

// ---------------------------------------------------------------------

// Height field of material m, or 0 where it has none.
pub fn bump_height(pose: Pose, fp: f32, m: i32, p: Vec3) -> f32 {
    if m == m_staff {
        bark(fp, staff_rest(pose, p)).0
    } else if m == m_robe {
        cloth(pose, fp, p)
    } else if m == m_leather {
        leather(pose, fp, p)
    } else if m == m_cork {
        cork(fp, p)
    } else if m == m_hat {
        hat_stars(pose, fp, p).1
    } else if m == m_beard {
        hair(pose, fp, p)
    } else if m == m_ground {
        if pose.prm.platform == platform_rock { flagstone_height(fp, p) } else { 0.0 }
    } else if m == m_terrain {
        terrain_bump(fp, to_world(pose.fl, p))
    } else if m == m_trunk {
        tree_bark(fp, to_world(pose.fl, p)).0
    } else if m == m_leaves {
        leaf_cells(fp, to_world(pose.fl, p)).0
    } else if m == m_needles {
        needle_streaks(fp, to_world(pose.fl, p)).0
    } else {
        0.0
    }
}

#[inline]
pub fn has_bump(m: i32) -> bool {
    m == m_staff || m == m_robe || m == m_leather || m == m_beard || m == m_cork || m == m_hat || m == m_ground || m == m_terrain || m == m_trunk || m == m_leaves || m == m_needles
}

// Tilts normal n by the gradient of the height field h across the surface.
// The gradient comes from four taps on a tetrahedron, small enough for the
// finest features (the stitches are 0.0016 wide).
pub fn bumped(h: impl Fn(Vec3) -> f32, p: Vec3, n: Vec3) -> Vec3 {
    let e = 0.0004;
    let mut g = splat(0.0);
    let mut i = 0;
    while i < 4 {
        let k = if i == 0 {
            vec(1.0, -1.0, -1.0)
        } else if i == 1 {
            vec(-1.0, -1.0, 1.0)
        } else if i == 2 {
            vec(-1.0, 1.0, -1.0)
        } else {
            vec(1.0, 1.0, 1.0)
        };
        g = g + scale(h(p + scale(e, k)), k);
        i += 1;
    }
    let g = scale(1.0 / (4.0 * e), g);
    let tangential = g - scale(dot(g, n), n);
    normalise(n - tangential)
}
