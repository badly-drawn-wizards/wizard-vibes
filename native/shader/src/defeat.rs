// The defeat screen's magic, drawn over the scene (the overlay entry
// point: at the window's own resolution, apart from the scene's dynamic
// scale, with alpha): an arcane circle turning round where the title goes,
// a quarter of the way down, the edges tinted; and by what defeated him,
// fire (embers rising, a glow from below), lightning (forked bolts striking
// down now and then, flashes) or wind (gusts swirling round the screen).
// t seconds since his defeat; kind as a kill code's (kill_*), else none.

use crate::prelude::*;

// (each death's colour)
#[inline]
fn tint_of(kind: i32) -> Vec3 {
    if kind == kill_fire {
        vec(1.0, 0.45, 0.12)
    } else if kind == kill_strike {
        vec(0.55, 0.7, 1.0)
    } else if kind == kill_wind {
        vec(0.45, 0.95, 1.0)
    } else {
        vec(0.8, 0.55, 1.0)
    }
}

// A glowing line at distance d from it, w wide.
#[inline]
fn glow(d: f32, w: f32) -> f32 {
    let u = d / w;
    (-(u * u)).exp()
}

// The overlay at pixel (x, y) of a vw x h window: premultiplied colour and
// coverage.
pub fn defeat_overlay(kind: i32, t: f32, vw: i32, h: i32, x: f32, y: f32) -> Vec4 {
    let fh = h as f32;
    // (screen space: y down, the height 2 across)
    let p = vec2((x - 0.5 * vw as f32) / fh * 2.0, (y - 0.5 * fh) / fh * 2.0);
    let fade = smoothstep(0.0, 1.5, t);
    let tint = tint_of(kind);
    // the edges darkened, in the death's colour
    let edge = smoothstep(0.55, 1.6, vec2(p.x * 0.75, p.y).length());
    let mut cover = 0.55 * fade * edge;
    let mut light = scale(0.12 * fade * edge, tint);
    // the arcane circle round the title: two rings, runes between them
    // turning one way, and ticks outside turning the other, growing in
    let c = p - vec2(0.0, -0.5);
    let grow = smoothstep(0.3, 2.0, t);
    let r = vec2(c.x * 0.55, c.y).length() / (0.35 + 0.05 * grow);
    let a = c.y.atan2(c.x * 0.55);
    let ring = glow(r - 1.0, 0.012) + 0.7 * glow(r - 0.82, 0.008) + 0.4 * glow(r - 1.18, 0.006);
    let runes = {
        let s = fract((a + 0.25 * t) * 24.0 / (2.0 * PI));
        let cell = ((a + 0.25 * t) * 24.0 / (2.0 * PI)).floor() as i32;
        let on = hash2(cell, 7) > 0.35;
        if on { smoothstep(0.3, 0.2, (s - 0.5).abs()) * smoothstep(0.08, 0.0, (r - 0.91).abs()) } else { 0.0 }
    };
    let ticks = {
        let s = fract((a - 0.4 * t) * 72.0 / (2.0 * PI));
        smoothstep(0.2, 0.0, (s - 0.5).abs()) * smoothstep(0.05, 0.0, (r - 1.1).abs())
    };
    let pulse = 0.85 + 0.15 * (3.0 * t).sin();
    light = light + scale(1.6 * grow * pulse * (ring + 0.8 * runes + 0.6 * ticks), tint);
    // a soft light behind the title
    light = light + scale(0.25 * grow * (-(r * r) * 1.3).exp(), tint);
    // by what defeated him
    if kind == kill_fire {
        // embers rising, drifting and flickering out; a glow from below
        let mut e = 0.0f32;
        let mut j = 0;
        while j < 48 {
            let h1 = hash2(j, 11);
            let h2 = hash2(j, 12);
            let speed = 0.12 + 0.2 * h2;
            let f = fract(t * speed + h1);
            let ex = (h1 * 2.0 - 1.0) * 1.9 + 0.08 * (6.0 * f + 9.0 * h2).sin();
            let ey = 1.1 - 2.4 * f;
            let d = (p - vec2(ex, ey)).length();
            let size = 0.004 + 0.006 * hash2(j, 13);
            let flick = 0.6 + 0.4 * (t * (7.0 + 9.0 * h2) + 6.0 * h1).sin();
            e += glow(d, size) * flick * smoothstep(1.0, 0.7, f) + 0.25 * glow(d, 4.0 * size) * smoothstep(1.0, 0.7, f);
            j += 1;
        }
        light = light + scale(1.8 * fade * e, vec(1.0, 0.55, 0.15));
        let below = smoothstep(0.3, 1.0, p.y);
        light = light + scale(0.35 * fade * below * (0.8 + 0.2 * (t * 2.3).sin()), vec(1.0, 0.3, 0.05));
        cover += 0.15 * fade * below;
    } else if kind == kill_strike {
        // now and then a forked bolt down from the top, a flash with it
        let period = 1.3;
        let k = (t / period).floor() as i32;
        let age = t - k as f32 * period;
        let bright = (-age * 9.0).exp() * fade;
        let x0 = (hash2(k, 21) * 2.0 - 1.0) * 1.4;
        let mut d = 1e9f32;
        let mut from = vec2(x0, -1.05);
        let mut s = 0;
        while s < 10 {
            let to = vec2(from.x + (hash2(k * 31 + s, 22) - 0.5) * 0.3, from.y + 0.16);
            let v = to - from;
            let u = saturate((p - from).dot(v) / v.dot(v));
            d = d.minimum((p - (from + v * u)).length());
            // (a fork off it, at every third)
            if s % 3 == 1 {
                let fork = vec2(to.x + (hash2(k * 17 + s, 23) - 0.5) * 0.4, to.y + 0.12);
                let w = fork - to;
                let uu = saturate((p - to).dot(w) / w.dot(w));
                d = d.minimum((p - (to + w * uu)).length() * 1.5);
            }
            from = to;
            s += 1;
        }
        light = light + scale(bright * (2.5 * glow(d, 0.006) + 0.6 * glow(d, 0.05)), vec(0.8, 0.9, 1.0));
        light = light + scale(0.18 * bright, tint);
        // crackles round the circle, always
        let crackle = smoothstep(0.92, 1.0, hash2((a * 40.0).floor() as i32, ((t * 12.0).floor()) as i32)) * glow(r - 1.0, 0.04);
        light = light + scale(1.5 * grow * crackle, vec(0.8, 0.9, 1.0));
    } else if kind == kill_wind {
        // gusts swirling round the screen: spiral bands, broken up
        let q = p - vec2(0.0, -0.2);
        let rr = q.length().maximum(0.05);
        let th = q.y.atan2(q.x);
        let band = (5.0 * th + 7.0 * rr.ln() - 2.6 * t).sin();
        let streak = smoothstep(0.85, 1.0, band) * smoothstep(0.2, 0.7, rr) * smoothstep(1.9, 1.0, rr);
        let broken = smoothstep(0.35, 0.75, vnoise2(th * 3.0 + rr * 4.0 - t * 1.5, rr * 6.0));
        light = light + scale(0.9 * fade * streak * broken, tint);
        cover += 0.1 * fade * streak * broken;
    }
    let cover = saturate(cover);
    vec4(light.x, light.y, light.z, cover)
}
