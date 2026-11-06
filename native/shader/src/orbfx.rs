// fut/orbfx.fut: what goes on in and around the orb, by the spell in hand
// (the lightning's arcs are in lightning.rs): media samples at q, in the
// staff's rest frame, so they go where the staff holds the orb. secs is the
// time in seconds.

use crate::prelude::*;

pub const no_media: MediaSample = MediaSample { cap: 1e9, radiance: Vec3::splat(0.0), extinction: 0.0 };

// How far from the orb's centre anything here reaches.
pub const orbfx_reach: f32 = orb_radius + 0.5;

// Fire: flames licking upwards inside the glass, hottest low down, and
// embers drifting up off the top of the orb, swaying as they rise and cool.
#[inline(never)]
pub fn fire_media(secs: f32, q: Vec3) -> MediaSample {
    let v = q - orb_pos();
    let r = length(v);
    if r > orbfx_reach {
        return MediaSample { cap: r - orbfx_reach + 0.02, ..no_media };
    }
    let flames = if r >= orb_radius {
        0.0
    } else {
        let rise = vec(0.0, -1.4 * secs, 0.0);
        let f = vnoise3(scale(18.0, v) + rise) + 0.5 * vnoise3(scale(37.0, v) + scale(1.7, rise));
        let shape = 1.25 - 0.55 * (v.y / orb_radius) - 0.4 * (r / orb_radius);
        smoothstep(0.9, 1.3, f * shape)
    };
    let flame = scale(8.0 * flames, mix(vec(1.0, 0.3, 0.04), vec(1.0, 0.85, 0.45), flames));
    // embers: each rises from the orb's top over its own cycle
    let (mut acc, mut near) = (0f32, INF);
    let mut j = 0i32;
    while j < 7 {
        let h = |n: i32| hash2(j * 13 + n, 57);
        let life = 1.2 + 0.8 * h(1);
        let ph = fract(secs / life + h(2));
        let a = 6.283 * h(3) + 1.5 * ph;
        let spread = 0.03 + 0.06 * ph;
        let pos = orb_pos() + vec(spread * a.cos(), 0.8 * orb_radius + 0.38 * ph, spread * a.sin());
        let d = length(q - pos);
        let acc_n = acc + (1.0 - ph) * (-(d * d) / 0.00012).exp();
        let near_n = near.minimum(d);
        acc = acc_n;
        near = near_n;
        j += 1;
    }
    let embers = acc;
    MediaSample {
        cap: if r < orb_radius { 0.012 } else { (0.3 * near + 0.003).minimum(0.3 * (r - orb_radius) + 0.012) },
        radiance: flame + scale(14.0 * embers, vec(1.0, 0.55, 0.15)),
        extinction: 0.0,
    }
}

// Wind: a vortex of pale streaks spinning inside the glass, and wisps
// circling the orb on a tilted ring, each trailing a fading tail.
#[inline(never)]
pub fn wind_media(secs: f32, q: Vec3) -> MediaSample {
    let v = q - orb_pos();
    let r = length(v);
    if r > orbfx_reach {
        return MediaSample { cap: r - orbfx_reach + 0.02, ..no_media };
    }
    let swirl = if r >= orb_radius {
        0.0
    } else {
        let th = v.z.atan2(v.x);
        let h = v.y / orb_radius;
        let rxz = (v.x * v.x + v.z * v.z).sqrt() / orb_radius;
        let band = 0.5 + 0.5 * (3.0 * th - 7.0 * secs + 6.0 * h + 2.0 * vnoise3(scale(20.0, v))).sin();
        smoothstep(0.7, 1.0, band) * smoothstep(0.05, 0.45, rxz) * (0.4 + 0.6 * rxz) + 0.08
    };
    // the ring, tilted, about 0.18 from the centre
    let w = vec(v.x, v.y * 0.94 - v.z * 0.34, v.z * 0.94 + v.y * 0.34);
    let ring_r = 0.18;
    let rxz = (w.x * w.x + w.z * w.z).sqrt();
    let dt = ((rxz - ring_r) * (rxz - ring_r) + w.y * w.y).sqrt();
    let alpha = w.z.atan2(w.x);
    let mut acc = 0f32;
    let mut j = 0i32;
    while j < 3 {
        let head = 4.0 * secs + 2.094 * j as f32;
        let da = alpha - head;
        let da = da - 6.283 * round_even(da / 6.283); // (-pi, pi]: behind the head is negative
        let tail = if da > 0.05 || da < -1.6 { 0.0 } else { 1.0 + da / 1.6 };
        acc = acc + tail * tail;
        j += 1;
    }
    let wisps = acc;
    let wisp = wisps * (-(dt * dt) / 0.00015).exp();
    MediaSample {
        cap: if r < orb_radius { 0.012 } else { 0.3 * dt + 0.004 },
        radiance: scale(13.0 * swirl, vec(0.45, 0.9, 1.0)) + scale(5.0 * wisp, vec(0.7, 0.95, 1.0)),
        extinction: 0.0,
    }
}
