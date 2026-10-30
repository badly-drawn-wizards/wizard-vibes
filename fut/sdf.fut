-- Signed distance primitives and operators (after Inigo Quilez).

import "vec3"

def sd_sphere (p: vec3) (r: f32) : f32 = length p - r

def sd_box (p: vec3) (b: vec3) : f32 =
  let q = vabs p -^ b
  in length (vmaxs q 0) + f32.min (maxc q) 0

def sd_capsule (p: vec3) (a: vec3) (b: vec3) (r: f32) : f32 =
  let pa = p -^ a
  let ba = b -^ a
  let h = saturate (dot pa ba / dot ba ba)
  in length (pa -^ scale h ba) - r

-- Vertical capsule-ish cone: radius r1 at y=0, r2 at y=h.
def sd_round_cone (p: vec3) (r1: f32) (r2: f32) (h: f32) : f32 =
  let b = (r1 - r2) / h
  let a = f32.sqrt (1 - b * b)
  let q = vec2 (length2 (vec2 p.x p.z)) p.y
  let k = q.x * (-b) + q.y * a
  in if k < 0 then length2 q - r1
     else if k > a * h then length2 (vec2 q.x (q.y - h)) - r2
     else q.x * a + q.y * b - r1

-- Cone frustum: radius r1 at y=0, r2 at y=h.
def sd_capped_cone (p: vec3) (h: f32) (r1: f32) (r2: f32) : f32 =
  let hh = h * 0.5
  let qx = length2 (vec2 p.x p.z)
  let qy = p.y - hh
  let k1x = r2
  let k1y = hh
  let k2x = r2 - r1
  let k2y = 2 * hh
  let cax = qx - f32.min qx (if qy < 0 then r1 else r2)
  let cay = f32.abs qy - hh
  let t = saturate (((k1x - qx) * k2x + (k1y - qy) * k2y) / (k2x * k2x + k2y * k2y))
  let cbx = qx - k1x + k2x * t
  let cby = qy - k1y + k2y * t
  let s = if cbx < 0 && cay < 0 then -1 else 1
  in s * f32.sqrt (f32.min (cax * cax + cay * cay) (cbx * cbx + cby * cby))

-- Torus in the xz plane.
def sd_torus (p: vec3) (big: f32) (small: f32) : f32 =
  length2 (vec2 (length2 (vec2 p.x p.z) - big) p.y) - small

-- Vertical cylinder of radius r and half-height h.
def sd_cylinder (p: vec3) (r: f32) (h: f32) : f32 =
  let dx = length2 (vec2 p.x p.z) - r
  let dy = f32.abs p.y - h
  in f32.min (f32.max dx dy) 0 + length2 (vec2 (f32.max dx 0) (f32.max dy 0))

-- Approximate (not exact) ellipsoid distance.
def sd_ellipsoid (p: vec3) (r: vec3) : f32 =
  let k0 = length {x = p.x / r.x, y = p.y / r.y, z = p.z / r.z}
  let k1 = length {x = p.x / (r.x * r.x), y = p.y / (r.y * r.y), z = p.z / (r.z * r.z)}
  in k0 * (k0 - 1) / k1

def smin (k: f32) (a: f32) (b: f32) : f32 =
  let h = f32.max (k - f32.abs (a - b)) 0 / k
  in f32.min a b - h * h * k * 0.25

def smax (k: f32) (a: f32) (b: f32) : f32 = -(smin k (-a) (-b))

-- Bend space around the z axis as y increases (used for the droopy hat tip).
def bend (k: f32) (p: vec3) : vec3 =
  let a = k * p.y * p.y
  let r = rot2 a (vec2 p.x p.y)
  in {x = r.x, y = r.y, z = p.z}

-- Fold the xz plane into one of n angular sectors; returns local point.
def rep_angle (n: f32) (p: vec3) : vec3 =
  let sector = 2 * f32.pi / n
  let a = f32.atan2 p.z p.x
  let a' = (fract (a / sector + 0.5) - 0.5) * sector
  let r = length2 (vec2 p.x p.z)
  in {x = r * f32.cos a', y = p.y, z = r * f32.sin a'}

-- Distance paired with a material id; umin keeps the closer one.
type hit = (f32, i32)
def umin (a: hit) (b: hit) : hit = if a.0 <= b.0 then a else b
