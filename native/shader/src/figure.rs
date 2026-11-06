// fut/figure.fut: the wizard's parts: orb and cradle, robe, arms and hands,
// head, beard and hat, staff, and the gear at his waist; and the figure
// assembled from them.

use crate::prelude::*;

#[inline]
pub fn sd_orb(p: Vec3) -> f32 {
    sd_sphere(p - orb_pos(), orb_radius)
}

// The gold cradle holding the orb: a collar on the staff tip and prongs that
// curve up around the glass with a helical twist. Angles are polar angles
// from the orb's bottom, along the staff's axis.
pub const prongs: f32 = 5.0;
pub const prong_start: f32 = 0.45;
pub const prong_end: f32 = 1.37; // 78 degrees: 40% of the way up the orb
pub const prong_twist: f32 = 1.2; // radians around the orb per radian climbed
pub const prong_radius: f32 = 0.013;

#[inline(never)]
pub fn sd_cradle_exact(p: Vec3) -> f32 {
    // a frame around the staff's axis
    let ax = staff_dir();
    let e1 = normalise(cross(ax, vec(0.0, 0.0, 1.0)));
    let e2 = cross(ax, e1);
    let v = p - orb_pos();
    let rho = length(v);
    let theta = clamp(-1.0, 1.0, -(dot(v, ax)) / rho.maximum(1e-6)).acos();
    let phi = dot(v, e2).atan2(dot(v, e1));
    // prongs: tubes on a sphere just outside the glass, folded into one sector
    let r = orb_radius + 0.6 * prong_radius;
    let th = clamp(prong_start, prong_end, theta);
    let f = (th - prong_start) / (prong_end - prong_start);
    let sector = 2.0 * PI / prongs;
    let dphi = (fract((phi - prong_twist * th) / sector + 0.5) - 0.5) * sector;
    let radial = rho - r;
    let across = r * th.sin() * dphi;
    let along = r * (theta - th);
    // the twist stretches space by at most this much, so divide it out
    let stretch = (1.0 + (prong_twist * th.sin()).powf(2.0)).sqrt();
    let prong = ((radial * radial + across * across + along * along).sqrt() - prong_radius * (1.0 - 0.4 * f)) / stretch;
    let tip = ((radial * radial + across * across + (r * (theta - prong_end)).powf(2.0)).sqrt() - 0.014) / stretch;
    // collar: a ring where the prongs start, on a cup flaring from the staff
    let ring_centre = orb_pos() - scale(r * prong_start.cos(), ax);
    let ring_r = r * prong_start.sin();
    let w = p - ring_centre;
    let h = dot(w, ax);
    let ring = length2(vec2(length(w - scale(h, ax)) - ring_r, h)) - 0.014;
    let cup = sd_round_cone_ab(p, staff_top - scale(0.05, ax), ring_centre, staff_radius, 0.85 * ring_r);
    smin(0.02, ring, cup).minimum(smin(0.01, prong, tip))
}

// (the whole cradle fits in a sphere of radius 0.15 round this point)
#[inline]
pub fn sd_cradle(p: Vec3) -> f32 {
    sphere_bounded(orb_pos() - scale(0.04, staff_dir()), 0.15, 0.15, sd_cradle_exact, p)
}

pub fn sd_robe(pose: Pose, p: Vec3) -> f32 {
    let a = p.z.atan2(p.x);
    let hem = 0.02 * (9.0 * a + 2.0 * pose.tm).sin() * saturate(1.0 - p.y / 0.8);
    let cone = sd_capped_cone(p, 1.35, pose.prm.robe_hem, 0.17) - 0.03 + hem;
    // the chest fills out with each breath
    let b = pose.breath;
    let shoulders = sd_ellipsoid(p - vec(0.0, 1.3 + 0.01 * b, 0.004 * b), vec(0.29 + 0.008 * b, 0.14, 0.2 + 0.01 * b));
    smin(0.12, cone, shoulders)
}

// A collar standing up behind his neck and flaring outwards: a thin cone
// round the neck, kept inside an ellipsoid behind it, so it is tallest at
// the back, curves down to nothing at the sides and leaves the front open.
pub fn sd_collar(p: Vec3) -> f32 {
    let q = p - vec(0.0, 1.36, -0.01);
    let r = (q.x * q.x + q.z * q.z).sqrt();
    // (radius 0.17 + 0.35 h at height h; the distance to that cone, rescaled
    // by its slope)
    let shell = ((r - (0.17 + 0.3 * q.y)) / (1.0f32 + 0.3 * 0.3).sqrt()).abs() - 0.01;
    let keep = sd_ellipsoid(q - vec(0.0, 0.0, -0.24), vec(0.32, 0.24, 0.36));
    shell.maximum(keep.maximum(-q.y))
}

// An arm: upper arm and a forearm whose sleeve flares towards the cuff,
// left open: a narrower cone is cut out of its end, so the cuff is a tube
// the hand comes out of.
pub fn sd_arm(shoulder: Vec3, elbow: Vec3, wrist: Vec3, p: Vec3) -> f32 {
    let d = normalise(wrist - elbow);
    let fore = sd_round_cone_ab(p, elbow, wrist, 0.064, 0.078);
    // (the opening lies within 0.13 of the cuff; beyond 0.2 it cannot affect
    // the sleeve, so it is only evaluated near the cuff)
    let sleeve = if length(p - wrist) > 0.2 {
        fore
    } else {
        smax(0.008, fore, -(sd_round_cone_ab(p, wrist - scale(0.07, d), wrist + scale(0.06, d), 0.05, 0.062)))
    };
    smin(0.04, sd_capsule(p, shoulder, elbow, 0.075), sleeve)
}

// Where the right hand's palm is: out of the sleeve, along the forearm.
#[inline]
pub fn right_palm(pose: Pose) -> Vec3 {
    pose.r_wrist + scale(0.085, normalise(pose.r_wrist - pose.r_elbow))
}

pub fn sd_arms(pose: Pose, p: Vec3) -> f32 {
    sd_arm(pose.l_shoulder, pose.l_elbow, pose.l_wrist, p).minimum(sd_arm(pose.r_shoulder, pose.r_elbow, pose.r_wrist, p))
}

// The arms as they cast shadows, with no branch: the sleeve shrunk by 0.002,
// with its opening always cut, by a capsule holding the real cut.
#[inline]
pub fn sd_arm_shadow(shoulder: Vec3, elbow: Vec3, wrist: Vec3, p: Vec3) -> f32 {
    let d = normalise(wrist - elbow);
    let fore = sd_round_cone_ab(p, elbow, wrist, 0.062, 0.076);
    let sleeve = fore.maximum(-sd_capsule(p, wrist - scale(0.07, d), wrist + scale(0.06, d), 0.064));
    smin(0.04, sd_capsule(p, shoulder, elbow, 0.075), sleeve)
}

pub fn sd_arms_shadow(pose: Pose, p: Vec3) -> f32 {
    sd_arm_shadow(pose.l_shoulder, pose.l_elbow, pose.l_wrist, p).minimum(sd_arm_shadow(pose.r_shoulder, pose.r_elbow, pose.r_wrist, p))
}

// Cartoon hands: a thumb and three fingers.

// A wrist: skin from inside the sleeve's cuff out to the palm.
#[inline]
pub fn sd_wrist(cuff: Vec3, palm: Vec3, p: Vec3) -> f32 {
    let d = normalise(palm - cuff);
    sd_capsule(p, cuff - scale(0.03, d), palm - scale(0.02, d), 0.03)
}

// The left hand, closed round the staff. A palm on the elbow's side, and
// digits made of short straight segments with knuckles, like the pointing
// hand's: three fingers wrapping round the staff below the thumb, which
// wraps the other way above them. (built round the staff wherever it is
// held, see pose_at)
pub fn sd_left_hand(pose: Pose, p: Vec3) -> f32 {
    let g = pose.l_grip;
    let staff_dir = pose.l_axis;
    let u = normalise(pose.l_palm - g); // to the palm
    let v = cross(u, staff_dir); // the way the fingers wrap
    let q = p - g;
    let palm = sd_ellipsoid(vec(dot(q, u) - 0.045, dot(q, v), dot(q, staff_dir)), vec(0.026, 0.036, 0.05));
    // a digit at height y along the staff: segments between points on a
    // circle of radius 0.048 round it, at the given angles from the palm
    let around = move |y: f32, phi: f32| g + scale(y, staff_dir) + scale(0.048 * phi.cos(), u) + scale(0.048 * phi.sin(), v);
    // (angles are scalars, not an array)
    let digit = move |y0: f32, y1: f32, a0: f32, a1: f32, a2: f32, a3: f32, r: f32| {
        let pt = move |k: f32, a: f32| around(mix1(y0, y1, k / 3.0), a);
        let p0 = pt(0.0, a0);
        let p1 = pt(1.0, a1);
        let p2 = pt(2.0, a2);
        let p3 = pt(3.0, a3);
        sd_capsule(p, p0, p1, r).minimum(sd_capsule(p, p1, p2, r - 0.001).minimum(sd_capsule(p, p2, p3, r - 0.002)))
    };
    let fingers = digit(-0.03, -0.03, 0.2, 1.3, 2.4, 3.35, 0.0155);
    let fingers = digit(0.0, 0.0, 0.2, 1.35, 2.5, 3.45, 0.016).minimum(fingers);
    let fingers = digit(0.03, 0.03, 0.2, 1.3, 2.4, 3.3, 0.0155).minimum(fingers);
    let thumb = digit(0.05, 0.062, 0.4, -0.5, -1.35, -1.9, 0.016);
    smin(0.012, smin(0.02, palm, sd_wrist(pose.l_wrist, pose.l_palm, p)), fingers.minimum(thumb))
}

// The right hand, which points: a palm in a frame along the forearm, three
// fingers of two straight segments side by side, and a thumb. At rest the
// fingers are nearly straight with a slight bend; as he points into a turn
// the index finger straightens and the others close. Thumbing back over his
// shoulder (a thumbs-up tossed up and behind him), all three close into a
// fist, its palm to his head, and the thumb sticks out behind him (-z).
pub fn sd_right_hand(pose: Pose, p: Vec3) -> f32 {
    let f = normalise(pose.r_wrist - pose.r_elbow); // along the forearm
    // away from the thumb (thumbing back, the forearm rises nearly upright:
    // then the thumb is behind, the back of the hand out to his right, -x)
    let side = normalise(cross(f, normalise(mix(vec(0.0, 1.0, 0.0), vec(-1.0, 0.0, 0.0), pose.thumb))));
    let up = cross(side, f); // the back of the hand
    let centre = right_palm(pose);
    let q = p - centre;
    let lq = vec(dot(q, side), dot(q, up), dot(q, f));
    let palm = sd_ellipsoid(lq, vec(0.036, 0.026, 0.045));
    // a finger across the palm, two segments of lengths l1 and l2, bent by
    // c1 and c1 + c2 radians towards the palm
    let finger = move |across: f32, c1: f32, c2: f32, l1: f32, l2: f32| {
        let dir = move |c: f32| scale(c.cos(), f) - scale(c.sin(), up);
        let base = centre + scale(0.035, f) + scale(across, side);
        let knuckle = base + scale(l1, dir(c1));
        let tip = knuckle + scale(l2, dir(c1 + c2));
        sd_capsule(p, base, knuckle, 0.015).minimum(sd_capsule(p, knuckle, tip, 0.0145))
    };
    let pt = pose.point;
    let th = pose.thumb;
    let index = finger(-0.025, mix1(mix1(0.15, 0.0, pt), 1.1, th), mix1(mix1(0.2, 0.0, pt), 1.3, th), 0.032, 0.028);
    // (the other two close to point, and to make the fist)
    let closed = pt.max(th);
    let middle = finger(0.0, mix1(0.2, 1.1, closed), mix1(0.25, 1.3, closed), 0.034, 0.028);
    let ring = finger(0.025, mix1(0.25, 1.1, closed), mix1(0.3, 1.3, closed), 0.03, 0.025);
    // the thumb: from the inner side of the palm, along the index finger
    let tb = centre - scale(0.03, side) + scale(0.005, f);
    let along = normalise(scale(0.85, f) - scale(0.4, side) - scale(0.2 + 0.25 * pt, up));
    let tdir = normalise(mix(along, vec(0.0, 0.25, -1.0), th));
    let thumb = sd_capsule(p, tb, tb + scale(0.042 + 0.03 * th, tdir), 0.015 + 0.002 * th);
    let wrist = sd_wrist(pose.r_wrist, centre, p);
    smin(0.012, smin(0.02, palm, wrist), index.minimum(middle).minimum(ring.minimum(thumb)))
}

// (each hand, with its wrist, fits in a sphere of radius 0.14)
pub fn sd_hands(pose: Pose, p: Vec3) -> f32 {
    sphere_bounded(pose.l_palm, 0.14, 0.15, move |q| sd_left_hand(pose, q), p)
        .minimum(sphere_bounded(right_palm(pose), 0.14, 0.15, move |q| sd_right_hand(pose, q), p))
}

// The head, beard and hat move together: offset by the pose, and turned
// about the neck to look where he points (or the camera looks), and tilted
// up or down.
pub const neck: Vec3 = Vec3::new(0.0, 1.45, 0.0);

#[inline]
pub fn head_space(pose: Pose, p: Vec3) -> Vec3 {
    neck + tilt_up(-pose.nod, yaw(-pose.look, p - pose.head - neck))
}

// v turned about x by a: +z rising towards +y for positive a.
#[inline]
pub fn tilt_up(a: f32, v: Vec3) -> Vec3 {
    let (s, c) = (a.sin(), a.cos());
    vec(v.x, v.y * c + v.z * s, v.z * c - v.y * s)
}

// The body above the hips leans sideways into turns (a shear; the hem stays
// put), and twists from the hips to the shoulders to look the camera's way.
// The lean is at most 0.15 and the twist about 0.45 over 0.6, so distances
// stretch by a few percent.
#[inline]
pub fn body_space(pose: Pose, p: Vec3) -> Vec3 {
    let q = p - vec(pose.lean * 0f32.maximum(p.y - 0.45), 0.0, 0.0);
    yaw(-pose.twist * twist_at(q.y), q)
}

// head space, mirrored onto the left side
#[inline]
pub fn mirror_x(p: Vec3) -> Vec3 {
    vec(p.x.abs(), p.y, p.z)
}

// An old man's head: a skull deeper than it is wide, cheekbones, a long
// bulbous nose with a bridge, and ears that come to a point (the jaw is
// under the beard). The face's front stays where the eyes and brows sit.
pub fn sd_head(pose: Pose, p: Vec3) -> f32 {
    let p = head_space(pose, p);
    let skull = sd_ellipsoid(p - vec(0.0, 1.62, 0.0), vec(0.165, 0.18, 0.19));
    let q = mirror_x(p);
    let cheek = sd_sphere(q - vec(0.072, 1.565, 0.115), 0.055);
    let jaw = sd_ellipsoid(p - vec(0.0, 1.5, 0.06), vec(0.12, 0.08, 0.11));
    let face = smin(0.05, jaw, smin(0.04, skull, cheek));
    let bridge = sd_round_cone_ab(p, vec(0.0, 1.64, 0.168), vec(0.0, 1.585, 0.212), 0.015, 0.022);
    let tip = sd_sphere(p - vec(0.0, 1.57, 0.215), 0.032);
    let wings = sd_sphere(q - vec(0.026, 1.56, 0.19), 0.02);
    let nose = smin(0.01, wings, smin(0.015, bridge, tip));
    let ear = sd_round_cone_ab(q, vec(0.158, 1.575, 0.005), vec(0.18, 1.685, -0.045), 0.028, 0.005);
    let ear = ear.maximum(-(q.x - 0.15)); // (flat against the head)
    smin(0.012, ear, smin(0.02, face, nose))
}

// White hair round the back of his head, below the hat: a shell a little
// outside the skull, behind the ears, down to the collar.
pub fn sd_hair(pose: Pose, p: Vec3) -> f32 {
    let p = head_space(pose, p);
    let shell = sd_ellipsoid(p - vec(0.0, 1.6, -0.01), vec(0.178, 0.2, 0.205));
    shell.maximum(p.z + 0.03 - 0.1 * (p.y - 1.5))
}

// The eyes: eyeballs set into the face (their iris and pupil are shading,
// see eye_albedo), with upper lids that close to blink.
// (the head's surface is at z ~ 0.173 here, so the eye stands ~0.011 proud)
pub const eye_centre: Vec3 = Vec3::new(0.062, 1.632, 0.159); // the left eye's (+x); mirrored
pub const eye_radius: f32 = 0.025;

#[inline]
pub fn sd_eyes(pose: Pose, p: Vec3) -> f32 {
    sd_sphere(mirror_x(head_space(pose, p)) - eye_centre, eye_radius)
}

// The upper lids: skin over the top of each eyeball, down to a lid line
// that drops to below the eye's equator to blink.
pub fn sd_eyelids(pose: Pose, p: Vec3) -> f32 {
    let q = mirror_x(head_space(pose, p)) - eye_centre;
    let line = mix1(0.6 * eye_radius, -1.05 * eye_radius, pose.blink);
    sd_sphere(q, eye_radius + 0.003).maximum(line - q.y)
}

// The eyebrows: bushy arcs above the eyes, from inner to outer end. When he
// points they knit into a determined look: the inner ends drop and draw
// together, the outer ends lift.
pub fn sd_brows(pose: Pose, p: Vec3) -> f32 {
    let q = mirror_x(head_space(pose, p));
    let k = pose.point;
    let inner = vec(0.03 - 0.008 * k, 1.664 - 0.012 * k, 0.174);
    let mid = vec(0.064, 1.676 + 0.002 * k, 0.172);
    let outer = vec(0.1, 1.668 + 0.008 * k, 0.145);
    smin(0.006, sd_round_cone_ab(q, inner, mid, 0.012, 0.01), sd_round_cone_ab(q, mid, outer, 0.01, 0.006))
}

// The beard hangs from the cheeks to a forked tip resting on the chest.
pub const beard_top: Vec3 = Vec3::new(0.0, 1.52, 0.11);
pub const beard_tip: Vec3 = Vec3::new(0.0, 0.98, 0.27);

// Beard space: swaying with the loop, and squashed sideways so the beard is
// wider than deep (dividing x only contracts space, so distances stay safe).
pub fn beard_space(pose: Pose, p: Vec3) -> Vec3 {
    let p = head_space(pose, p);
    let tm = pose.tm;
    let sway = 0.03 * (2.0 * tm).sin() * saturate((1.5 - p.y) / 0.5);
    vec((p.x - sway) / 1.45, p.y, p.z)
}

// The beard's locks, at a point in beard space. Around the beard's axis
// the angle is divided into about 18 locks; noise warps the divisions so
// locks vary in width and wander as they hang. Shared by the shape and the
// material, so shading follows the locks.
#[derive(Clone, Copy)]
pub struct Lock {
    pub h: f32,     // along the beard, 0 at the cheeks .. 1 at the tip
    pub phase: f32, // across the locks: lock k spans [k, k + 1)
    pub crown: f32, // 0 in the groove between locks .. 1 on a lock's crown
    pub rnd: f32,   // per-lock random value
}

pub fn beard_lock(q: Vec3) -> Lock {
    let ba = beard_tip - beard_top;
    let h = saturate(dot(q - beard_top, ba) / dot(ba, ba));
    let rv = q - (beard_top + scale(h, ba));
    let phi = rv.x.atan2(rv.z);
    let warp = 1.6 * vnoise2(h * 3.0, phi * 1.5 + 7.0) + 0.4 * (h * 9.0 + phi * 2.0).sin();
    let phase = phi * 18.0 / (2.0 * PI) + warp;
    let id = ftoi(phase.floor());
    Lock { h, phase, crown: (PI * fract(phase)).sin().powf(0.7), rnd: hash2(id, 51) }
}

pub fn sd_beard(pose: Pose, p: Vec3) -> f32 {
    let q = beard_space(pose, p);
    let p = head_space(pose, p);
    // fork the lower half into two points
    let fork = 0.035 * smoothstep(1.3, 0.98, q.y);
    let fq = vec(q.x.abs() - fork, q.y, q.z);
    let body = sd_round_cone_ab(fq, beard_top, beard_tip, 0.13, 0.018);
    // grooves between the locks, deeper for some locks than others, and each
    // lock ending at its own length, so the tip is ragged rather than cut
    let l = beard_lock(q);
    let groove = 0.008 * (1.0 - l.h * 0.5) * (1.0 - l.crown) * (0.6 + 0.8 * l.rnd);
    let len = length(beard_tip - beard_top);
    let cut = (l.h - (0.86 + 0.14 * l.rnd)) * len;
    let body = smax(0.02, body + groove, cut);
    let jaw = sd_ellipsoid(p - vec(0.0, 1.53, 0.05), vec(0.155, 0.1, 0.13));
    let moustache = {
        let m = vec(p.x.abs(), p.y, p.z);
        sd_round_cone_ab(m, vec(0.01, 1.548, 0.178), vec(0.105, 1.49, 0.13), 0.024, 0.01)
    };
    // (the warped locks stretch the field a little; scale to stay safe)
    0.85 * smin(0.03, smin(0.06, body, jaw), moustache)
}

pub const hat_base: Vec3 = Vec3::new(0.0, 1.74, 0.0);
pub const hat_height: f32 = 0.72;
#[inline]
pub fn hat_radius(y: f32) -> f32 {
    0.2 - (0.2 - 0.012) * saturate(y / hat_height)
}

pub fn sd_hat(pose: Pose, p: Vec3) -> f32 {
    let hp = head_space(pose, p) - hat_base;
    let bp = bend(pose.prm.hat_droop, hp);
    let cone = sd_capped_cone(bp, hat_height, hat_radius(0.0), hat_radius(hat_height));
    // a flat disc with a slim rolled (welt) edge
    let brim = sd_torus(hp, 0.25, 0.012).minimum(sd_cylinder(hp, 0.255, 0.01));
    smin(0.04, cone, brim)
}

// A flat gold band round the hat just above the brim: a short frustum
// following the cone's slope, standing a few millimetres proud of it, with
// softened edges. (The hat's bend is negligible this low.)
pub const band_bottom: f32 = 0.018; // above the hat's base
#[inline]
pub fn band_top(prm: Params) -> f32 {
    band_bottom + prm.band_height
}

pub fn sd_band(pose: Pose, p: Vec3) -> f32 {
    let y0 = band_bottom;
    let y1 = band_top(pose.prm);
    let proud = 0.006;
    let q = head_space(pose, p) - (hat_base + vec(0.0, y0, 0.0));
    sd_capped_cone(q, y1 - y0, hat_radius(y0) + proud - 0.002, hat_radius(y1) + proud - 0.002) - 0.002
}

// A frame around the staff: (along the axis from the foot, and two unit
// vectors across it). (not const-evaluable in Rust: a function)
#[inline]
pub fn staff_frame() -> (Vec3, Vec3) {
    let e1 = normalise(cross(staff_dir(), vec(0.0, 0.0, 1.0)));
    (e1, cross(staff_dir(), e1))
}

// Coordinates in the staff's frame: x, z across the axis, y along it.
#[inline]
pub fn staff_local(p: Vec3) -> Vec3 {
    let (e1, e2) = staff_frame();
    let v = p - staff_foot;
    vec(dot(v, e1), dot(v, staff_dir()), dot(v, e2))
}

// (not const-evaluable in Rust: a function)
#[inline]
pub fn staff_length() -> f32 {
    length(staff_top - staff_foot)
}

// Knots: swellings at a few heights, each on one side of the branch.

// A branch rather than a pole: the radius swells and narrows along the
// length, thickening a little towards the foot, with a few knots. (The
// field is stretched by under 2%, so it stays safe to trace.)
#[inline(never)]
pub fn sd_staff(p: Vec3) -> f32 {
    let q = staff_local(p);
    let y = clamp(0.0, staff_length(), q.y);
    let r = staff_radius * (1.12 - 0.12 * y / staff_length() + 0.07 * (y * 6.3 + 1.0).sin() + 0.04 * (y * 17.1).sin());
    let shaft = length(q - vec(0.0, y, 0.0)) - r;
    let knot = move |ky: f32, ka: f32| {
        let c = vec(staff_radius * 0.75 * ka.cos(), ky, staff_radius * 0.75 * ka.sin());
        sd_ellipsoid(q - c, vec(0.022, 0.035, 0.022))
    };
    let knots = knot(0.55, 0.6).minimum(knot(1.32, 2.6).minimum(knot(1.72, 4.4)));
    smin(0.015, shaft, knots)
}

// ---------------------------------------------------------------------
// Belt, buckle, potions and tome

// What the gear at his waist depends on: the uniforms' belt height and
// robe hem, as a record of their own (with the same field names) so its
// parts can be real GPU functions without taking all the uniforms.
#[derive(Clone, Copy)]
pub struct Gear {
    pub belt_height: f32,
    pub robe_hem: f32,
}
#[inline]
pub fn gear_of(prm: Params) -> Gear {
    Gear { belt_height: prm.belt_height, robe_hem: prm.robe_hem }
}

// The robe's surface radius at height y (the -0.03 in sd_robe rounds it
// outward).
#[inline]
pub fn robe_radius(prm: Gear, y: f32) -> f32 {
    prm.robe_hem - (prm.robe_hem - 0.17) * y / 1.35 + 0.03
}

// Outward horizontal direction at an angle round the body (0 = front, +x at 90 degrees).
#[inline]
pub fn around(a: f32) -> Vec3 {
    vec(a.sin(), 0.0, a.cos())
}
#[inline]
pub fn deg(d: f32) -> f32 {
    d * PI / 180.0
}

#[inline(never)]
pub fn sd_belt(prm: Gear, p: Vec3) -> f32 {
    let r = length2(vec2(p.x, p.z));
    let q = vec2(r - (robe_radius(prm, p.y) + 0.008), p.y - prm.belt_height);
    // a rounded rectangle in the (radius, height) plane, swept round the body
    let d = vec2(q.x.abs() - 0.008, q.y.abs() - 0.024);
    length2(vec2(d.x.maximum(0.0), d.y.maximum(0.0))) + d.x.maximum(d.y).minimum(0.0) - 0.006
}

// A gold pentagram in a ring, on the front of the belt. Every stroke is a
// round tube, so it reads as cast metal.
#[inline]
pub fn buckle_centre(prm: Gear) -> Vec3 {
    vec(0.0, prm.belt_height, robe_radius(prm, prm.belt_height) + 0.03)
}

#[inline(never)]
pub fn sd_buckle(prm: Gear, p: Vec3) -> f32 {
    let q = p - buckle_centre(prm);
    let rad = 0.05;
    let corner = move |k: i32| {
        let a = PI / 2.0 + 2.0 * PI * k as f32 / 5.0;
        vec(rad * a.cos(), rad * a.sin(), 0.0)
    };
    let mut d = 1e9f32;
    let mut k = 0;
    while k < 5 {
        d = d.minimum(sd_capsule(q, corner(k), corner(srem(k + 2, 5)), 0.006));
        k += 1;
    }
    let star = d;
    let ring = length2(vec2(length2(vec2(q.x, q.y)) - (rad + 0.008), q.z)) - 0.007;
    star.minimum(ring)
}

// A flat strap (0.022 wide, 0.005 thick) from a to b, its face along n.
pub fn sd_strap(p: Vec3, a: Vec3, b: Vec3, n: Vec3) -> f32 {
    let ab = b - a;
    let len = length(ab);
    let d = scale(1.0 / len, ab);
    let w = normalise(cross(d, n));
    let nn = cross(w, d);
    let v = p - mix(a, b, 0.5);
    sd_box(vec(dot(v, d), dot(v, w), dot(v, nn)), vec(0.5 * len + 0.002, 0.01, 0.0015)) - 0.001
}

// Three potions hanging from the belt on the left hip: two round flasks
// either side of a tall bottle, each with a short neck and a cork. The glass
// is not part of the marched scene (rays pass through it; see potion_glass
// and glass in shading.fut); the liquid inside is, and glows.
#[inline]
pub fn potion_angle(i: i32) -> f32 {
    deg(70.0 + 24.0 * i as f32)
}

// Each potion hangs below the belt from a strap round its neck.
#[inline]
pub fn potion_neck_top(prm: Gear, i: i32) -> Vec3 {
    let y = prm.belt_height - 0.06;
    scale(robe_radius(prm, y) + 0.022, around(potion_angle(i))) + vec(0.0, y, 0.0)
}

#[inline]
pub fn potion_neck_radius(i: i32) -> f32 {
    if i == 1 { 0.012 } else { 0.013 }
}

// The centre of potion i's body, in body space.
#[inline]
pub fn potion_centre(prm: Gear, i: i32) -> Vec3 {
    let body_top = potion_neck_top(prm, i) - vec(0.0, 0.03, 0.0);
    if i == 1 { body_top - vec(0.0, 0.055, 0.0) } else { body_top - vec(0.0, 0.032, 0.0) }
}

// (outer glass surface, cork, liquid surface height) of potion i
#[inline(never)]
pub fn potion_parts(prm: Gear, i: i32, p: Vec3) -> (f32, f32, f32) {
    let neck_top = potion_neck_top(prm, i);
    let body_top = neck_top - vec(0.0, 0.03, 0.0);
    let c = potion_centre(prm, i);
    let (glass, level) = if i == 1 {
        // tall bottle: a rounded cylinder under the neck, four-fifths full
        (
            smin(0.01, sd_capsule(p, c - vec(0.0, 0.03, 0.0), c + vec(0.0, 0.025, 0.0), 0.03), sd_capsule(p, body_top, neck_top, 0.012)),
            c.y + 0.03,
        )
    } else {
        // round flask, two-thirds full
        let r = 0.04;
        (smin(0.015, sd_sphere(p - c, r), sd_capsule(p, body_top, neck_top, 0.013)), c.y + 0.25 * r)
    };
    // an open mouth (the neck's capsule would otherwise dome over the cork)
    // with a rolled lip round it
    let nr = potion_neck_radius(i);
    let glass = glass.maximum(p.y - (neck_top.y - 0.002));
    let glass = smin(0.002, glass, sd_torus(p - (neck_top - vec(0.0, 0.002, 0.0)), nr, 0.0035));
    // the cork: pushed into the neck (filling it inside the glass wall) and
    // standing a little proud of the lip
    let cork = sd_cylinder(p - (neck_top - vec(0.0, 0.004, 0.0)), nr - 0.0045, 0.011) - 0.0015;
    (glass, cork, level)
}

// The leather that holds potion i: a band round its neck, and a strap from
// the band up under the belt.
#[inline(never)]
pub fn potion_strap(prm: Gear, i: i32, p: Vec3) -> f32 {
    let neck_top = potion_neck_top(prm, i);
    let nr = potion_neck_radius(i);
    let band_y = neck_top.y - 0.011;
    let q = p - vec(neck_top.x, band_y, neck_top.z);
    let rq = vec2(length2(vec2(q.x, q.z)) - (nr + 0.0035), q.y);
    let band = length2(vec2((rq.x.abs() - 0.0015).maximum(0.0), (rq.y.abs() - 0.0045).maximum(0.0))) - 0.001;
    let a = potion_angle(i);
    let out = around(a);
    // from the band's side facing the body, up against the robe and on up
    // underneath the belt (the belt's band spans robe_radius - 0.006 to
    // + 0.022, so a strap at + 0.006 is hidden inside it)
    let start = vec(neck_top.x, band_y, neck_top.z) - scale(nr + 0.004, out);
    let below = scale(robe_radius(prm, prm.belt_height - 0.04) + 0.006, out) + vec(0.0, prm.belt_height - 0.04, 0.0);
    let tucked = scale(robe_radius(prm, prm.belt_height) + 0.006, out) + vec(0.0, prm.belt_height, 0.0);
    let strap = sd_strap(p, below, tucked, out).minimum(sd_strap(p, start, below, out));
    band.minimum(strap)
}

// The liquid fills the inside of the glass (0.004 thick) up to its level.
#[inline]
pub fn potion_liquid(prm: Gear, i: i32, p: Vec3) -> f32 {
    let (glass, _, level) = potion_parts(prm, i, p);
    smax(0.004, glass + 0.004, p.y - level)
}

pub fn potion(prm: Gear, i: i32, p: Vec3) -> Hit {
    let (_, cork, _) = potion_parts(prm, i, p);
    umin((potion_strap(prm, i, p), m_leather), umin((cork, m_cork), (potion_liquid(prm, i, p), m_potion + i)))
}

// Where the potions hang, in body space, and a sphere holding all three.
#[inline]
pub fn potions_at(prm: Gear) -> Vec3 {
    let y = prm.belt_height - 0.08;
    scale(robe_radius(prm, y) + 0.05, around(potion_angle(1))) + vec(0.0, y, 0.0)
}
pub const potions_radius: f32 = 0.26;

// From body space (which leans into turns) back to the figure's local space.
#[inline]
pub fn from_body_space(pose: Pose, q: Vec3) -> Vec3 {
    let q = yaw(pose.twist * twist_at(q.y), q);
    q + vec(pose.lean * 0f32.maximum(q.y - 0.45), 0.0, 0.0)
}

// The potions' glass: distance to the nearest outer glass surface, in the
// figure's local space.
pub fn potion_glass(pose: Pose, p: Vec3) -> f32 {
    let prm = gear_of(pose.prm);
    sphere_bounded(
        potions_at(prm),
        potions_radius,
        0.05,
        move |q| potion_parts(prm, 0, q).0.minimum(potion_parts(prm, 1, q).0.minimum(potion_parts(prm, 2, q).0)),
        body_space(pose, p),
    )
}

// The nearest potion's liquid: (distance, material id), for the glow.
pub fn nearest_liquid(pose: Pose, p: Vec3) -> Hit {
    let prm = gear_of(pose.prm);
    sphere_bounded_hit(
        potions_at(prm),
        potions_radius,
        0.1,
        m_potion,
        move |q| umin((potion_liquid(prm, 2, q), m_potion + 2), umin((potion_liquid(prm, 1, q), m_potion + 1), (potion_liquid(prm, 0, q), m_potion))),
        body_space(pose, p),
    )
}

// A tome hanging on the right hip, lying against the robe's slope.
#[inline(never)]
pub fn tome(prm: Gear, p: Vec3) -> Hit {
    let a = deg(-100.0);
    let y = prm.belt_height - 0.12;
    let out = normalise(around(a) + vec(0.0, 0.28, 0.0)); // the robe's cone slopes inwards
    let up = normalise(cross(cross(out, vec(0.0, 1.0, 0.0)), out));
    let side = cross(up, out);
    let c = scale(robe_radius(prm, y) + 0.035, around(a)) + vec(0.0, y, 0.0);
    let v = p - c;
    let q = vec(dot(v, side), dot(v, up), dot(v, out)); // width, height, thickness
    let box_ = move |cx: f32, cz: f32, b: Vec3, r: f32| sd_box(q - vec(cx, 0.0, cz), b - splat(r)) - r;
    let covers = box_(0.0, 0.027, vec(0.08, 0.1, 0.006), 0.003).minimum(box_(0.0, -0.027, vec(0.08, 0.1, 0.006), 0.003));
    let spine = box_(-0.077, 0.0, vec(0.008, 0.1, 0.03), 0.006);
    let pages = box_(0.004, 0.0, vec(0.074, 0.094, 0.022), 0.002);
    let clasp = box_(0.08, 0.0, vec(0.006, 0.016, 0.031), 0.003);
    // a strap from low on the front cover, up and over the top of the belt
    let on_cover = move |u: f32| c + scale(u, up) + scale(0.035, out);
    let belt_front = scale(robe_radius(prm, prm.belt_height) + 0.019, around(a)) + vec(0.0, prm.belt_height + 0.016, 0.0);
    let belt_top = scale(robe_radius(prm, prm.belt_height) + 0.002, around(a)) + vec(0.0, prm.belt_height + 0.027, 0.0);
    let strap = sd_strap(p, on_cover(-0.04), on_cover(0.1), out);
    let strap = sd_strap(p, on_cover(0.1), belt_front, out).minimum(strap);
    let strap = sd_strap(p, belt_front, belt_top, out).minimum(strap);
    umin((strap, m_leather), umin((clasp, m_gold), umin((pages, m_pages), (covers.minimum(spine), m_tome))))
}

pub fn waist_gear(prm: Gear, p: Vec3) -> Hit {
    let ty = prm.belt_height - 0.1;
    let tome_at = scale(robe_radius(prm, ty) + 0.035, around(deg(-100.0))) + vec(0.0, ty, 0.0);
    let h = (sd_belt(prm, p), m_leather);
    let h = umin(sphere_bounded_hit(buckle_centre(prm), 0.07, 0.15, m_gold, move |q| (sd_buckle(prm, q), m_gold), p), h);
    let h = umin(
        sphere_bounded_hit(potions_at(prm), potions_radius, 0.15, m_leather, move |q| umin(potion(prm, 2, q), umin(potion(prm, 1, q), potion(prm, 0, q))), p),
        h,
    );
    umin(sphere_bounded_hit(tome_at, 0.19, 0.15, m_tome, move |q| tome(prm, q), p), h)
}

// ---------------------------------------------------------------------
// The figure

// The parts of the figure in the layers of `mask`.
// The head's parts are bounded in groups (in head space, which only moves
// and turns, so distances there are distances here): far from a group, its
// bound stands in for it, so a step near the robe does not evaluate the
// beard's locks or the bent hat, and the reverse.
// (the face: head, eyes, lids and brows all lie within 0.3 of its centre;
// the hat, however it droops, within 0.75 of its base, bending keeping its
// length; the beard, swaying and widened, within 0.3 of its axis)
#[inline]
pub fn bounded(b: f32, m: i32, exact: impl Fn() -> Hit) -> Hit {
    if b > 0.1 { (b, m) } else { exact() }
}

// (pseudo-layers in the mask: the figure without its hat, or the hat alone:
// for an enemy's hat flying off; see enemy_figure)
pub const no_hat: i32 = 1 << 20;
pub const only_hat: i32 = 1 << 21;

pub fn wizard(mask: i32, pose: Pose, p: Vec3) -> Hit {
    let hq = head_space(pose, p);
    let face = move || {
        umin(
            (sd_brows(pose, p), m_brow),
            umin((sd_eyelids(pose, p), m_skin), umin((sd_eyes(pose, p), m_eye), umin((sd_hair(pose, p), m_beard), (sd_head(pose, p), m_skin)))),
        )
    };
    let hat = move || umin((sd_band(pose, p), m_band), (sd_hat(pose, p), m_hat));
    let beard = move || (sd_beard(pose, p), m_beard);
    let figure = if !(has(mask, layers_figure)) {
        nothing
    } else if has(mask, only_hat) {
        tag(layers_figure, bounded(length(hq - hat_base) - 0.8, m_hat, hat))
    } else {
        let h = (sd_collar(body_space(pose, p)).minimum(smin(0.06, sd_robe(pose, body_space(pose, p)), sd_arms(pose, p))), m_robe);
        let h = umin((sd_hands(pose, p), m_skin), h);
        let h = umin(bounded(length(hq - vec(0.0, 1.6, 0.06)) - 0.32, m_skin, face), h);
        let h = umin(bounded(sd_capsule(hq, beard_top, beard_tip, 0.3), m_beard, beard), h);
        let h = if has(mask, no_hat) { h } else { umin(bounded(length(hq - hat_base) - 0.8, m_hat, hat), h) };
        tag(layers_figure, h)
    };
    let gear = if !(has(mask, layers_gear)) { nothing } else { tag(layers_gear, waist_gear(gear_of(pose.prm), body_space(pose, p))) };
    umin(gear, figure)
}

// The staff and its cradle, which move together when he holds the staff
// out (at q, in the staff's rest frame).
pub fn held_staff(mask: i32, q: Vec3) -> Hit {
    if !(has(mask, layers_staff)) {
        nothing
    } else {
        tag(layers_staff, umin((sd_cradle(q), m_gold), (sd_staff(q), m_staff)))
    }
}

#[inline]
pub fn staff_bound(q: Vec3) -> f32 {
    sd_capsule(q, staff_foot, orb_pos(), 0.25)
}

// (with room for the left arm reaching out with the staff: its hand reaches
// at most 1.02 from the axis, over every way he can aim)
#[inline]
pub fn wizard_bound(p: Vec3) -> f32 {
    sd_capsule(p, vec(0.0, 0.3, 0.0), vec(0.0, 2.3, 0.0), 1.03)
}

pub const figure_layers: i32 = layers_figure | layers_staff | layers_gear;

// The figure, staff and gear in the layers of `mask`. Every wizard part lies
// inside the capsule of wizard_bound, and the staff in that of staff_bound,
// so their distances are lower bounds: beyond the requested tolerance,
// return the bound and skip evaluating the parts (see the distance contract
// in march.fut).
#[inline(never)]
pub fn bounded_figure(mask: i32, pose: Pose, tol: f32, p: Vec3) -> Hit {
    let b = wizard_bound(p);
    let q = staff_rest(pose, p);
    let sb = staff_bound(q);
    if !(has(mask, figure_layers)) {
        nothing
    } else {
        umin(
            if sb > tol { (sb, m_staff) } else { held_staff(mask, q) },
            if b > tol { (b, m_robe) } else { wizard(mask, pose, p) },
        )
    }
}

// The orb is glass, refracted analytically by the shading (see shading.fut),
// so it is not part of the marched scene; what is inside it is drawn by the
// media (see media.fut and lightning.fut).
