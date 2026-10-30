-- Sphere-traced wizard. Compiled with `futhark webgpu` for the browser
-- harness and with `futhark multicore` for the CPU reference image.

import "vec3"
import "sdf"

-- Materials.
def m_robe : i32 = 1
def m_skin : i32 = 2
def m_beard : i32 = 3
def m_hat : i32 = 4
def m_gold : i32 = 5
def m_eye : i32 = 6
def m_wood : i32 = 7
def m_orb : i32 = 8
def m_ground : i32 = 9

-- Animation loops every `loop_seconds`. Inside the renderer `tm` is the loop
-- angle in [0, 2pi), and every animated term uses a whole-number multiple of
-- it, so the last frame of the loop matches the first.
def loop_seconds : f32 = 12
def loop_angle (seconds: f32) : f32 = 2 * f32.pi * fract (seconds / loop_seconds)

-- Scene constants.
def orb_pos : vec3 = vec 0.43 2.12 0.1
def orb_radius (_tm: f32) : f32 = 0.11
-- Three glow pulses per loop; the orb's light and emission follow along.
def pulse (tm: f32) : f32 = 0.85 + 0.15 * f32.sin (3 * tm)

-- The glow is a cloud around the orb whose density falls off exponentially
-- with distance from the orb's surface. Its strength and extent breathe
-- with the pulse.
def glow_density (tm: f32) (od: f32) : f32 =
  let s = f32.sin (3 * tm)
  let extent = 0.12 + 0.035 * s
  in (1.6 + 0.8 * s) * f32.exp (-(f32.max od 0) / extent)
def moon_dir : vec3 = normalise (vec (-0.45) 0.65 0.6)

-- Integer hash -> [0,1).
def hash3 (x: i32) (y: i32) (z: i32) : f32 =
  let h = u32.i32 x * 73856093 ^ u32.i32 y * 19349663 ^ u32.i32 z * 83492791
  let h = (h ^ (h >> 16)) * 0x7feb352d
  let h = (h ^ (h >> 15)) * 0x846ca68b
  let h = h ^ (h >> 16)
  in f32.u32 (h >> 8) / 16777216

def hash2 (x: i32) (y: i32) : f32 = hash3 x y 1337

---------------------------------------------------------------------
-- Geometry

def sd_orb (tm: f32) (p: vec3) : f32 = sd_sphere (p -^ orb_pos) (orb_radius tm)

def sd_robe (tm: f32) (p: vec3) : f32 =
  let a = f32.atan2 p.z p.x
  let hem = 0.02 * f32.sin (9 * a + 2 * tm) * saturate (1 - p.y / 0.8)
  let cone = sd_capped_cone p 1.35 0.55 0.17 - 0.03 + hem
  let shoulders = sd_ellipsoid (p -^ vec 0 1.3 0) (vec 0.29 0.14 0.2)
  in smin 0.12 cone shoulders

def sd_arms (p: vec3) : f32 =
  let right = sd_capsule p (vec 0.2 1.33 0) (vec 0.42 1.17 0.12) 0.075
  let left = sd_capsule p (vec (-0.2) 1.33 0) (vec (-0.27) 1.02 0.24) 0.075
  in f32.min right left

def sd_hands (p: vec3) : f32 =
  f32.min (sd_sphere (p -^ vec 0.44 1.16 0.13) 0.06)
          (sd_sphere (p -^ vec (-0.27) 0.97 0.29) 0.055)

def sd_head (p: vec3) : f32 =
  let head = sd_sphere (p -^ vec 0 1.6 0.02) 0.17
  let nose = sd_ellipsoid (p -^ vec 0 1.58 0.19) (vec 0.035 0.05 0.05)
  in smin 0.03 head nose

def sd_eyes (p: vec3) : f32 =
  let q = {x = f32.abs p.x, y = p.y, z = p.z}
  in sd_sphere (q -^ vec 0.065 1.64 0.155) 0.022

def sd_beard (tm: f32) (p: vec3) : f32 =
  let sway = 0.03 * f32.sin (2 * tm) * saturate (1.55 - p.y)
  let q = p -^ vec sway 1.54 0.11
  let q = {x = q.x, y = -q.y, z = q.z - 0.15 * q.y}
  let wave = 0.012 * f32.sin (30 * p.y + 10 * p.x)
  in sd_round_cone q 0.13 0.025 0.55 + wave

def hat_base : vec3 = vec 0 1.74 0

def sd_hat (tm: f32) (p: vec3) : f32 =
  let hp = p -^ hat_base
  let bp = bend (0.9 + 0.15 * f32.sin tm) hp
  let cone = sd_capped_cone bp 0.72 0.2 0.012
  let brim = f32.min (sd_torus hp 0.3 0.022) (sd_cylinder hp 0.31 0.01)
  in smin 0.04 cone brim

def sd_band (p: vec3) : f32 = sd_torus (p -^ (hat_base +^ vec 0 0.05 0)) 0.186 0.025

def sd_staff (p: vec3) : f32 = sd_capsule p (vec 0.48 0 0.15) (vec 0.43 2.02 0.1) 0.028

def scene (tm: f32) (p: vec3) : hit =
  let body = smin 0.06 (sd_robe tm p) (sd_arms p)
  in (body, m_robe)
     |> umin (sd_hands p, m_skin)
     |> umin (sd_head p, m_skin)
     |> umin (sd_eyes p, m_eye)
     |> umin (sd_beard tm p, m_beard)
     |> umin (sd_hat tm p, m_hat)
     |> umin (sd_band p, m_gold)
     |> umin (sd_staff p, m_wood)
     |> umin (sd_orb tm p, m_orb)
     |> umin (p.y, m_ground)

def dist (tm: f32) (p: vec3) : f32 = (scene tm p).0

---------------------------------------------------------------------
-- Marching

-- Returns (distance, material or -1, orb glow integrated along the ray up to
-- the hit). Near the orb, steps are capped to a fraction of the distance to
-- it, so the glow integral is sampled finely where its density changes fast.
def march (tm: f32) (ro: vec3) (rd: vec3) : (f32, i32, f32) =
  let (t, _, m, g) =
    loop (t, i, m, g) = (0.02f32, 0i32, -1i32, 0f32)
    while i < 220 && m < 0 && t < 40 do
      let p = ro +^ scale t rd
      let (d, mat) = scene tm p
      in if d < 0.0004 * t then (t, i + 1, mat, g)
         else
           let od = sd_orb tm p
           let dt = f32.min (d * 0.85) (0.3 * od + 0.015)
           in (t + dt, i + 1, -1, g + glow_density tm od * dt)
  in (t, m, g)

-- Tetrahedral gradient, written as a loop so the scene is only inlined once.
def normal (tm: f32) (p: vec3) : vec3 =
  let e = 0.0006
  let n = loop n = splat 0 for i < 4 do
            let k = if i == 0 then vec 1 (-1) (-1)
                    else if i == 1 then vec (-1) (-1) 1
                    else if i == 2 then vec (-1) 1 (-1)
                    else vec 1 1 1
            in n +^ scale (dist tm (p +^ scale e k)) k
  in normalise n

def soft_shadow (tm: f32) (ro: vec3) (rd: vec3) (tmax: f32) : f32 =
  let (res, _, _) =
    loop (res, t, i) = (1f32, 0.02f32, 0i32)
    while i < 48 && t < tmax && res > 0.002 do
      let h = dist tm (ro +^ scale t rd)
      in (f32.min res (10 * h / t), t + clamp 0.01 0.25 h, i + 1)
  in smoothstep 0 1 (saturate res)

def ambient_occlusion (tm: f32) (p: vec3) (n: vec3) : f32 =
  let (occ, _) =
    loop (occ, sca) = (0f32, 1f32) for i < 5 do
      let h = 0.01 + 0.12 * f32.i32 i / 4
      let d = dist tm (p +^ scale h n)
      in (occ + (h - d) * sca, sca * 0.95)
  in saturate (1 - 3 * occ)

---------------------------------------------------------------------
-- Surface detail

-- Five-pointed star mask around a point in 2D local coords.
def star_mask (x: f32) (y: f32) (size: f32) : f32 =
  let r = f32.sqrt (x * x + y * y)
  let a = f32.atan2 y x
  let shape = size * (0.55 + 0.45 * f32.cos (5 * a))
  in 1 - smoothstep (shape * 0.8) shape r

def hat_stars (p: vec3) : f32 =
  let hp = p -^ hat_base
  let u = (f32.atan2 hp.z hp.x / (2 * f32.pi) + 0.5) * 12
  let v = hp.y * 9
  let cu = f32.floor u
  let cv = f32.floor v
  let h = hash2 (i32.f32 cu) (i32.f32 cv)
  let ox = 0.25 + 0.5 * hash2 (i32.f32 cu + 17) (i32.f32 cv)
  let oy = 0.25 + 0.5 * hash2 (i32.f32 cu) (i32.f32 cv + 31)
  in if h < 0.55 then star_mask (u - cu - ox) (v - cv - oy) 0.3 else 0

-- Glowing rune circle on the ground.
def runes (tm: f32) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let ring w c = 1 - smoothstep (w * 0.5) w (f32.abs (r - c))
  let rings = f32.max (ring 0.018 0.95) (ring 0.012 1.3)
  let n = 14f32
  let sector = 2 * f32.pi / n
  -- The glyph band turns once per loop.
  let q = rot2 tm (vec2 p.x p.z)
  let a = f32.atan2 q.y q.x + f32.pi
  let idx = i32.f32 (f32.floor (a / sector))
  let lp = rep_angle n {x = q.x, y = 0, z = q.y}
  let c = 1.125
  let h1 = hash2 idx 3
  let h2 = hash2 idx 7
  let seg (ax: f32) (az: f32) (bx: f32) (bz: f32) =
    sd_capsule lp (vec (c + ax) 0 az) (vec (c + bx) 0 bz) 0.008
  let g1 = seg (-0.08) (-0.05 + 0.1 * h1) 0.08 (0.05 - 0.1 * h1)
  let g2 = seg (-0.08 + 0.16 * h2) (-0.06) (0.08 - 0.16 * h1) 0.06
  let g3 = if h2 > 0.5 then seg 0 (-0.07) 0 0.07 else sd_torus (lp -^ vec c 0 0) 0.05 0.004
  let glyph = 1 - smoothstep 0.0 0.008 (f32.min g1 (f32.min g2 g3))
  -- Inner tick ring counter-rotates.
  let tq = rot2 (-2 * tm) (vec2 p.x p.z)
  let tp = rep_angle 48 {x = tq.x, y = 0, z = tq.y}
  let ticks = 1 - smoothstep 0.0 0.006 (sd_capsule tp (vec 0.83 0 0) (vec 0.89 0 0) 0.005)
  in f32.max (f32.max rings glyph) ticks * pulse tm

def sky (rd: vec3) : vec3 =
  let up = saturate (rd.y * 0.5 + 0.5)
  let base = mix (vec 0.06 0.04 0.14) (vec 0.004 0.006 0.03) (f32.sqrt up)
  let md = dot rd moon_dir
  let moon = smoothstep 0.9993 0.9996 md
  let halo = 0.15 * f32.exp (-(1 - md) * 60)
  let q = scale 220 rd
  let sx = i32.f32 (f32.floor q.x)
  let sy = i32.f32 (f32.floor q.y)
  let sz = i32.f32 (f32.floor q.z)
  let s = hash3 sx sy sz
  let star = if s > 0.9975 && rd.y > 0.02 then (s - 0.9975) * 400 * saturate (rd.y * 4) else 0
  in base +^ scale (moon * 2 + halo) (vec 0.85 0.9 1) +^ splat star

def albedo (tm: f32) (m: i32) (p: vec3) : vec3 =
  if m == m_robe then vec 0.07 0.05 0.33
  else if m == m_skin then vec 0.85 0.6 0.48
  else if m == m_beard then vec 0.85 0.85 0.9
  else if m == m_hat then mix (vec 0.09 0.08 0.36) (vec 1 0.8 0.3) (hat_stars p)
  else if m == m_gold then vec 0.95 0.72 0.25
  else if m == m_eye then splat 0.02
  else if m == m_wood then
    scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1)
  else if m == m_ground then
    let tile = hash2 (i32.f32 (f32.floor (p.x * 2))) (i32.f32 (f32.floor (p.z * 2)))
    let _ = tm
    in scale (0.8 + 0.4 * tile) (vec 0.11 0.11 0.13)
  else splat 0.5

def emission (tm: f32) (m: i32) (p: vec3) (n: vec3) (rd: vec3) : vec3 =
  if m == m_orb then
    let fres = pow (1 - saturate (-(dot n rd))) 2
    in scale (1.3 * pulse tm) (mix (vec 0.2 0.8 1) (vec 0.7 0.3 1) fres)
  else if m == m_ground then scale (0.9 * runes tm p) (vec 0.25 0.5 1)
  else if m == m_hat then scale (0.6 * hat_stars p) (vec 1 0.75 0.3)
  else splat 0

def specular (m: i32) : (f32, f32) = -- (strength, shininess)
  if m == m_gold then (1.0, 60)
  else if m == m_eye then (0.8, 120)
  else if m == m_skin then (0.15, 16)
  else if m == m_ground then (0.3, 40)
  else (0.08, 8)

---------------------------------------------------------------------
-- Shading

def shade (tm: f32) (ro: vec3) (rd: vec3) : vec3 =
  let (t, m, glow) = march tm ro rd
  let glow_col = scale glow (vec 0.35 0.55 1)
  in if m < 0 then sky rd +^ glow_col
     else
       let p = ro +^ scale t rd
       let n = normal tm p
       let col = albedo tm m p
       let occ = ambient_occlusion tm p n
       let (ks, shin) = specular m
       let po = p +^ scale 0.002 n
       -- moonlight
       let ml = moon_dir
       let mdiff = saturate (dot n ml) * soft_shadow tm po ml 12
       let mh = normalise (ml -^ rd)
       let mspec = ks * pow (saturate (dot n mh)) shin * mdiff
       let moon = scale (mdiff * 0.75) (col *^ vec 0.5 0.6 1.0) +^ scale mspec (vec 0.7 0.75 1)
       -- orb point light
       let to_orb = orb_pos -^ p
       let od = length to_orb
       let ol = scale (1 / od) to_orb
       let lit = if m == m_orb then 0 else
                   saturate (dot n ol)
                   * soft_shadow tm po ol (od - orb_radius tm - 0.03)
       let atten = 2.4 * pulse tm / (1 + 1.5 * od * od)
       let oh = normalise (ol -^ rd)
       let ospec = ks * pow (saturate (dot n oh)) shin * lit
       let orb = scale (lit * atten) (col *^ vec 0.6 0.5 1) +^ scale (ospec * atten) (vec 0.6 0.5 1)
       -- sky fill and rim
       let fill = scale (occ * (0.5 + 0.5 * n.y) * 0.35) (col *^ vec 0.2 0.22 0.45)
       let rim = scale (occ * pow (1 - saturate (-(dot n rd))) 3 * 0.25) (vec 0.35 0.3 0.8)
       let rim = if m == m_ground then splat 0 else rim
       let c = moon +^ orb +^ fill +^ rim +^ emission tm m p n rd
       let fog = 1 - f32.exp (-0.0025 * t * t)
       in mix c (sky rd) fog +^ glow_col

def aces (x: f32) : f32 = saturate ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))

-- Camera orbits a target; returns ray direction for pixel (x, y) with y down.
def camera_ray (vw: i32) (h: i32) (yaw: f32) (pitch: f32) (dst: f32) (x: i32) (y: i32) : (vec3, vec3) =
  let target = vec 0 1.15 0
  let ro = target +^ scale dst (vec (f32.cos pitch * f32.sin yaw) (f32.sin pitch) (f32.cos pitch * f32.cos yaw))
  let ro = {x = ro.x, y = f32.max ro.y 0.1, z = ro.z}
  let fw = normalise (target -^ ro)
  let rt = normalise (cross fw (vec 0 1 0))
  let up = cross rt fw
  let fh = f32.i32 h
  let u = (f32.i32 x + 0.5 - 0.5 * f32.i32 vw) / fh * 2
  let v = (0.5 * fh - f32.i32 y - 0.5) / fh * 2
  let rd = normalise (scale 2.414 fw +^ scale u rt +^ scale v up)
  in (ro, rd)

def pixel (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) (x: i32) (y: i32) : vec3 =
  let (ro, rd) = camera_ray vw h yaw pitch dst x y
  let c = shade (loop_angle tm) ro rd
  let c = vmap aces (scale 1.1 c)
  let c = vmap (\v -> (v ** (1 / 2.2))) c
  let ux = (f32.i32 x / f32.i32 vw - 0.5)
  let uy = (f32.i32 y / f32.i32 h - 0.5)
  let vig = 1 - 0.6 * (ux * ux + uy * uy)
  in scale vig c

def to_u8 (v: f32) : u32 = u32.f32 (saturate v * 255 + 0.5)

-- w is the padded row length (multiple of 64 so rows are 256-byte aligned);
-- columns x >= vw are padding. Pixels are packed BGRA8, little-endian, to
-- match a bgra8unorm canvas (the format every WebGPU implementation supports
-- for canvases; Firefox crashes copying buffers into rgba8unorm ones).
entry render (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) : []u32 =
  tabulate (i64.i32 w * i64.i32 h) (\i ->
    let x = i32.i64 (i % i64.i32 w)
    let y = i32.i64 (i / i64.i32 w)
    in if x >= vw then 0xff000000
       else let c = pixel vw h tm yaw pitch dst x y
            in to_u8 c.z | (to_u8 c.y << 8) | (to_u8 c.x << 16) | 0xff000000)

-- CPU reference image.
entry render_rgb (w: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) : [][][3]u8 =
  tabulate_2d (i64.i32 h) (i64.i32 w) (\y x ->
    let c = pixel w h tm yaw pitch dst (i32.i64 x) (i32.i64 y)
    in [u8.u32 (to_u8 c.x), u8.u32 (to_u8 c.y), u8.u32 (to_u8 c.z)])
