-- Materials, sky and lighting for the wizard scene.

import "vec3"
import "sdf"
import "hash"
import "anim"
import "march"
import "scene"

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

def material (tm: f32) (m: i32) (p: vec3) (n: vec3) (rd: vec3) : material =
  if m == m_robe then plain (vec 0.07 0.05 0.33)
  else if m == m_skin then plain (vec 0.85 0.6 0.48) with ks = 0.15 with shininess = 16
  else if m == m_beard then plain (vec 0.85 0.85 0.9)
  else if m == m_hat then
    let s = hat_stars tm p
    in plain (mix (vec 0.09 0.08 0.36) (vec 1 0.8 0.3) s) with emission = scale (0.6 * s) (vec 1 0.75 0.3)
  else if m == m_gold then plain (vec 0.95 0.72 0.25) with ks = 1 with shininess = 60
  else if m == m_eye then plain (splat 0.02) with ks = 0.8 with shininess = 120
  else if m == m_wood then
    plain (scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1))
  else if m == m_orb then
    let fres = pow (1 - saturate (-(dot n rd))) 2
    in plain (splat 0.5) with emission = scale (1.3 * pulse tm) (mix (vec 0.2 0.8 1) (vec 0.7 0.3 1) fres)
  else if m == m_ground then
    let tile = hash2 (i32.f32 (f32.floor (p.x * 2))) (i32.f32 (f32.floor (p.z * 2)))
    in plain (scale (0.8 + 0.4 * tile) (vec 0.11 0.11 0.13))
       with emission = scale (0.9 * rune_paint tm p) rune_glow_colour
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

def shade (tm: f32) (ro: vec3) (rd: vec3) : vec3 =
  let (t, m, glow) = march (scene tm) (media tm) ro rd
  in if m < 0 then sky rd +^ glow
     else
       let p = ro +^ scale t rd
       let n = normal (dist tm) p
       let mat = material tm m p n rd
       let occ = ambient_occlusion (dist tm) p n
       let po = p +^ scale 0.002 n
       let moon_shadow = soft_shadow (dist tm) po moon_dir 12
       let moon = light mat n rd moon_dir moon_shadow (vec 0.375 0.45 0.75) (vec 0.7 0.75 1)
       let to_orb = orb_pos -^ p
       let od = length to_orb
       let ol = scale (1 / od) to_orb
       let atten = 2.4 * pulse tm / (1 + 1.5 * od * od)
       let orb_shadow = if m == m_orb then 0 else soft_shadow (dist tm) po ol (od - orb_radius - 0.03)
       let orb_colour = scale atten (vec 0.6 0.5 1)
       let orb = light mat n rd ol orb_shadow orb_colour orb_colour
       let fill = scale (occ * (0.5 + 0.5 * n.y) * 0.35) (mat.albedo *^ vec 0.2 0.22 0.45)
       let rim = if mat.rim then scale (occ * pow (1 - saturate (-(dot n rd))) 3 * 0.25) (vec 0.35 0.3 0.8)
                 else splat 0
       let c = moon +^ orb +^ fill +^ rim +^ mat.emission
       let fog = 1 - f32.exp (-0.0025 * t * t)
       in mix c (sky rd) fog +^ glow
