-- Materials, sky and lighting for the wizard scene.

import "vec3"
import "sdf"
import "hash"
import "anim"
import "march"
import "scene"
import "lightning"
import "media"

def moon_dir : vec3 = normalise (vec (-0.45) 0.65 0.6)

---------------------------------------------------------------------
-- Surface detail

-- Five-pointed star mask around a point in 2D local coords.
def star_mask (x: f32) (y: f32) (size: f32) : f32 =
  let r = f32.sqrt (x * x + y * y)
  let a = f32.atan2 y x
  let shape = size * (0.68 + 0.32 * f32.cos (5 * a))
  in 1 - smoothstep (shape * 0.8) shape r

-- Stars scattered over the hat's cone. The pattern lives in the same bent
-- space as the cone's SDF, so it follows the drooping tip. Rows of square
-- cells run up the cone, each row with as many cells as fit around it, and
-- the star is measured in arc length, so stars keep their shape as the cone
-- narrows.
def hat_stars (tm: f32) (p: vec3) : f32 =
  let bp = bend (hat_bend tm) (p -^ hat_base)
  let cell = 0.085
  let y0 = 0.08 -- start above the band, keeping the brim clear
  let v = (bp.y - y0) / cell
  let cv = f32.floor v
  let row_radius = hat_radius (y0 + (cv + 0.5) * cell)
  let cells = f32.max 1 (f32.round (2 * f32.pi * row_radius / cell))
  let u = (f32.atan2 bp.z bp.x / (2 * f32.pi) + 0.5) * cells
  let cu = f32.floor u
  let iu = i32.f32 cu
  let iv = i32.f32 cv
  let ox = 0.3 + 0.4 * hash2 (iu + 17) iv
  let oy = 0.3 + 0.4 * hash2 iu (iv + 31)
  -- arc length per u unit at this point, in cell units
  let stretch = 2 * f32.pi * length2 (vec2 bp.x bp.z) / cells / cell
  in if bp.y < y0 || row_radius < 0.035 || hash2 iu iv >= 0.55 then 0
     else star_mask ((u - cu - ox) * stretch) (v - cv - oy) 0.28

def sky (rd: vec3) : vec3 =
  let up = saturate (rd.y * 0.5 + 0.5)
  let base = mix (vec 0.06 0.04 0.14) (vec 0.004 0.006 0.03) (f32.sqrt up)
  let md = dot rd moon_dir
  let moon = smoothstep 0.9993 0.9996 md
  let halo = 0.15 * f32.exp (-(1 - md) * 60)
  let q = scale 220 rd
  let s = hash3 (i32.f32 (f32.floor q.x)) (i32.f32 (f32.floor q.y)) (i32.f32 (f32.floor q.z))
  let star = if s > 0.9975 && rd.y > 0.02 then (s - 0.9975) * 400 * saturate (rd.y * 4) else 0
  in base +^ scale (moon * 2 + halo) (vec 0.85 0.9 1) +^ splat star

---------------------------------------------------------------------
-- Materials

type material = {albedo: vec3, emission: vec3, ks: f32, shininess: f32, rim: bool}

def plain (albedo: vec3) : material =
  {albedo, emission = splat 0, ks = 0.08, shininess = 8, rim = true}

def material (tm: f32) (g: glow) (m: i32) (p: vec3) : material =
  if m == m_robe then plain (vec 0.07 0.05 0.33)
  else if m == m_skin then plain (vec 0.85 0.6 0.48) with ks = 0.15 with shininess = 16
  else if m == m_beard then plain (scale (0.75 + 0.25 * beard_strands (beard_space tm p)) (vec 0.88 0.88 0.92))
  else if m == m_hat then
    let s = hat_stars tm p
    in plain (mix (vec 0.09 0.08 0.36) (vec 1 0.8 0.3) s) with emission = scale (0.6 * s) (vec 1 0.75 0.3)
  else if m == m_gold then plain (vec 0.95 0.72 0.25) with ks = 1 with shininess = 60
  else if m == m_eye then plain (splat 0.02) with ks = 0.8 with shininess = 120
  else if m == m_wood then
    plain (scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1))
  else if m == m_ground then
    let tile = hash2 (i32.f32 (f32.floor (p.x * 2))) (i32.f32 (f32.floor (p.z * 2)))
    in plain (scale (0.8 + 0.4 * tile) (vec 0.11 0.11 0.13))
       with emission = scale (0.9 * g.level * rune_paint tm p) rune_glow_colour
       with ks = 0.3 with shininess = 40 with rim = false
  else plain (splat 0.5)

---------------------------------------------------------------------
-- Lighting

-- Diffuse plus Blinn specular from one light in direction l. The colours
-- carry the light's intensity (including any attenuation).
def light (mat: material) (n: vec3) (rd: vec3) (l: vec3) (shadow: f32) (diffuse: vec3) (specular: vec3) : vec3 =
  let lit = saturate (dot n l) * shadow
  let spec = mat.ks * pow (saturate (dot n (normalise (l -^ rd)))) mat.shininess * lit
  in scale lit (mat.albedo *^ diffuse) +^ scale spec specular

-- The lit colour of a surface hit at distance t along rd (fog included).
def surface (tm: f32) (g: glow) (m: i32) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  let n = normal (dist tm) p
  let mat = material tm g m p
  let occ = ambient_occlusion (dist tm) p n
  let po = p +^ scale 0.002 n
  let moon_shadow = soft_shadow (occluder_dist tm) po moon_dir 12
  let moon = light mat n rd moon_dir moon_shadow (vec 0.375 0.45 0.75) (vec 0.7 0.75 1)
  let to_orb = orb_pos -^ p
  let od = length to_orb
  let ol = scale (1 / od) to_orb
  -- the orb's light flickers with the lightning, whitening in flashes
  let atten = 2.4 * g.level / (1 + 1.5 * od * od)
  -- Stop short of the orb: the staff pokes into its underside, and a
  -- penumbra estimate taken right next to the light bands badly.
  let orb_shadow = if od < orb_radius then 1 else soft_shadow (occluder_dist tm) po ol (od - orb_radius - 0.15)
  let orb_colour = scale atten (mix (vec 0.6 0.5 1) (vec 0.9 0.95 1) (0.6 * g.flash))
  let orb = light mat n rd ol orb_shadow orb_colour orb_colour
  let fill = scale (occ * (0.5 + 0.5 * n.y) * 0.35) (mat.albedo *^ vec 0.2 0.22 0.45)
  let rim = if mat.rim then scale (occ * pow (1 - saturate (-(dot n rd))) 3 * 0.25) (vec 0.35 0.3 0.8)
            else splat 0
  let c = moon +^ orb +^ fill +^ rim +^ mat.emission
  let fog = 1 - f32.exp (-0.0025 * t * t)
  in mix c (sky rd) fog

---------------------------------------------------------------------
-- The glass orb

-- Compile-time switch: reflect the scene in the orb's glass too.
def orb_reflections : bool = true
def orb_ior : f32 = 1.45
def glass_tint : vec3 = vec 0.93 0.97 1

-- Distances (entry, exit) where the ray meets the orb; entry > exit on a miss.
def orb_hit (ro: vec3) (rd: vec3) : (f32, f32) =
  let oc = ro -^ orb_pos
  let b = dot oc rd
  let disc = b * b - (dot oc oc - orb_radius * orb_radius)
  in if disc <= 0 then (1e9, -1e9)
     else let s = f32.sqrt disc in (-b - s, -b + s)

-- Refraction of i at a surface with normal n facing against i, for the ratio
-- eta of refractive indices (from / to). Reflects on total internal reflection.
def refract (i: vec3) (n: vec3) (eta: f32) : vec3 =
  let cosi = -(dot n i)
  let k = 1 - eta * eta * (1 - cosi * cosi)
  in if k < 0 then reflect i n
     else scale eta i +^ scale (eta * cosi - f32.sqrt k) n

-- Schlick's approximation of the reflected fraction.
def fresnel (cosi: f32) : f32 =
  let r0 = ((1 - orb_ior) / (1 + orb_ior)) ** 2
  in r0 + (1 - r0) * pow (1 - saturate cosi) 5

-- Where a reflected path starts: (exists, point, direction, weight).
type reflection = (bool, vec3, vec3, f32)

-- Follows a ray through at most three straight segments - outside, inside
-- the orb, outside again - refracting at the glass, and returns the colour
-- it gathers, weighted by w. Also returns the reflection spawned where the
-- ray first enters the orb.
def follow (tm: f32) (g: glow) (ro0: vec3) (rd0: vec3) (w0: f32) : (vec3, reflection) =
  let (_, _, _, acc, _, _, refl) =
    loop (ro, rd, w, acc, inside, done, refl) =
      (ro0, rd0, splat w0, splat 0, false, false, (false, ro0, rd0, 0f32))
    for _seg < 3 do
      if done then (ro, rd, w, acc, inside, done, refl) else
      let (t_in, t_out) = orb_hit ro rd
      let enters = !inside && t_in > 1e-3 && t_in < t_out
      let tmax = if inside then t_out else if enters then t_in else 40
      let (t, m, emitted) = march (scene tm) (media tm g) ro rd tmax
      let acc = acc +^ (w *^ emitted)
      in if m >= 0 then
           (ro, rd, w, acc +^ (w *^ surface tm g m (ro +^ scale t rd) rd t), inside, true, refl)
         else if enters && t >= t_in then
           -- into the glass
           let p = ro +^ scale t_in rd
           let n = normalise (p -^ orb_pos)
           let f = fresnel (-(dot n rd))
           let refl = if refl.0 then refl else (true, p, reflect rd n, w.x * f)
           in (p, refract rd n (1 / orb_ior), scale (1 - f) (w *^ glass_tint), acc, true, false, refl)
         else if inside && t >= t_out then
           -- out of the glass
           let p = ro +^ scale t_out rd
           let n = normalise (p -^ orb_pos)
           in (p, refract rd (scale (-1) n) orb_ior, w *^ glass_tint, acc, false, false, refl)
         else (ro, rd, w, acc +^ (w *^ sky rd), inside, true, refl)
  in (acc, refl)

def shade (tm: f32) (ro: vec3) (rd: vec3) : vec3 =
  let g = glow_now tm -- once per pixel, not per march step
  -- Path 0 is the camera ray; path 1 its reflection off the orb, if any.
  -- A loop, so that follow (and all the shading) is compiled only once.
  let paths = if orb_reflections then 2 else 1
  let (c, _) =
    loop (c, refl) = (splat 0, (false, ro, rd, 0f32)) for k < paths do
      if k == 1 && !refl.0 then (c, refl) else
      let (o, d, w) = if k == 0 then (ro, rd, 1) else (refl.1, refl.2, refl.3)
      let (col, r) = follow tm g o d w
      in (c +^ col, if k == 0 then r else refl)
  in c
