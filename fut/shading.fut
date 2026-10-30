-- Materials, sky and lighting for the wizard scene.

import "vec3"
import "sdf"
import "hash"
import "anim"
import "march"
import "scene"
import "lightning"
import "media"
import "world"
import "textures"


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
def hat_stars (pose: pose) (p: vec3) : f32 =
  let bp = bend (hat_bend pose.tm) (head_space pose p -^ hat_base)
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

-- The night sky in a world direction.
def sky (rd: vec3) : vec3 =
  let up = saturate (rd.y * 0.5 + 0.5)
  let base = mix (vec 0.06 0.04 0.14) (vec 0.004 0.006 0.03) (f32.sqrt up)
  let md = dot rd moon_world
  let moon = smoothstep 0.9993 0.9996 md
  let halo = 0.15 * f32.exp (-(1 - md) * 60)
  let q = scale 220 rd
  let s = hash3 (i32.f32 (f32.floor q.x)) (i32.f32 (f32.floor q.y)) (i32.f32 (f32.floor q.z))
  let star = if s > 0.9975 && rd.y > 0.02 then (s - 0.9975) * 400 * saturate (rd.y * 4) else 0
  in base +^ scale (moon * 2 + halo) (vec 0.85 0.9 1) +^ splat star

---------------------------------------------------------------------
-- Materials

-- Metals tint their highlights with their albedo and have little diffuse.
-- sheen: a cloth-like glow at grazing angles, tinted by the albedo.
type material = {albedo: vec3, emission: vec3, ks: f32, shininess: f32, rim: bool, metal: bool, sheen: f32}

def plain (albedo: vec3) : material =
  {albedo, emission = splat 0, ks = 0.08, shininess = 8, rim = true, metal = false, sheen = 0}

def material (pose: pose) (_g: glow) (fp: f32) (m: i32) (p: vec3) (n: vec3) : material =
  let tm = pose.tm in
  if m == m_robe then plain (cloth_albedo pose p) with rim = false with sheen = 0.9 with ks = 0.03
  else if m == m_staff then plain (bark_albedo fp p) with ks = 0.05
  else if m == m_skin then plain (vec 0.85 0.6 0.48) with ks = 0.15 with shininess = 16
  else if m == m_beard then plain (hair_albedo pose p) with ks = 0
  else if m == m_hat then
    let s = hat_stars pose p
    in plain (mix (vec 0.09 0.08 0.36) (vec 1 0.8 0.3) s) with emission = scale (0.6 * s) (vec 1 0.75 0.3)
  else if m == m_gold then plain (vec 1 0.6 0.14) with ks = 1.6 with shininess = 48 with metal = true
  else if m == m_eye then plain (splat 0.02) with ks = 0.8 with shininess = 120
  else if m == m_wood then
    plain (scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1))
  else if m == m_rock then
    -- red-orange sandstone, banded and mottled (strongly saturated, since
    -- the blue moonlight mutes warm colours)
    let band = 0.5 + 0.5 * f32.sin (p.y * 9 + 2 * vnoise3 (scale 1.5 p))
    let c = mix (vec 0.55 0.15 0.05) (vec 0.72 0.33 0.1) band
    in plain (scale (0.6 + 0.7 * vnoise3 (scale 4 p)) c) with ks = 0.1
  else if m == m_terrain then
    -- moss in the valleys, bare rock on steep slopes, snow on high flats
    let w = to_world pose.fl p
    let up = (to_world_dir pose.fl n).y
    let grain = 0.75 + 0.5 * vnoise2 (w.x * 1.7) (w.z * 1.7)
    let moss = vec 0.09 0.14 0.08
    let stone = vec 0.2 0.18 0.17
    let snow = vec 0.75 0.8 0.88
    let c = mix stone moss (smoothstep 0.7 0.85 up)
    let c = mix c snow (smoothstep 7 8.5 w.y * smoothstep 0.6 0.8 up)
    in plain (scale grain c) with rim = false
  else if m == m_leather then plain (leather_albedo pose fp p) with ks = 0.35 with shininess = 28
  else if m == m_cork then plain (cork_albedo fp p) with ks = 0.05
  else if m == m_tome then plain (vec 0.32 0.05 0.06) with ks = 0.15 with shininess = 16
  else if m == m_pages then plain (vec 0.85 0.8 0.66)
  else if m >= m_potion && m < m_potion + 3 then
    -- the glowing liquid (its glass is drawn by `glass`)
    let c = glow_colour m
    in plain (scale 0.3 c) with emission = scale 0.8 c with ks = 0.6 with shininess = 60
  else if m == m_ground then
    let tile = hash2 (i32.f32 (f32.floor (p.x * 2))) (i32.f32 (f32.floor (p.z * 2)))
    in plain (scale (0.8 + 0.4 * tile) (vec 0.3 0.14 0.08))
       with emission = scale (0.9 * rune_pulse tm * rune_paint tm p) rune_glow_colour
       with ks = 0.3 with shininess = 40 with rim = false
  else plain (splat 0.5)

---------------------------------------------------------------------
-- Lighting

-- Diffuse plus Blinn specular from one light in direction l. The colours
-- carry the light's intensity (including any attenuation).
def light (mat: material) (n: vec3) (rd: vec3) (l: vec3) (shadow: f32) (diffuse: vec3) (specular: vec3) : vec3 =
  let lit = saturate (dot n l) * shadow
  let spec = mat.ks * pow (saturate (dot n (normalise (l -^ rd)))) mat.shininess * lit
  in if mat.metal then scale (0.25 * lit) (mat.albedo *^ diffuse) +^ scale spec (mat.albedo *^ specular)
     else scale lit (mat.albedo *^ diffuse) +^ scale spec specular

---------------------------------------------------------------------
-- Lights

-- A light: directional (pos is then its direction) or a point light whose
-- intensity falls off as 1 / (1 + falloff d^2), and smoothly to nothing at
-- `range` (cheap lights only reach nearby surfaces). Point lights of some
-- size stop their shadow rays short of it. One material can be exempt from
-- a light's shadows.
type light = { pos: vec3, directional: bool
             , diffuse: vec3, specular: vec3
             , falloff: f32, range: f32, radius: f32
             , shadowed: bool, unshadowed: i32 }

-- The moon, the orb, and a weak light in each potion.
def n_lights : i32 = 5

def light_at (pose: pose) (g: glow) (k: i32) : light =
  if k == 0 then
    { pos = pose.moon, directional = true
    , diffuse = vec 0.375 0.45 0.75, specular = vec 0.7 0.75 1
    , falloff = 0, range = f32.inf, radius = 0, shadowed = true, unshadowed = -1 }
  else if k == 1 then
    -- The orb's light flickers with the lightning, whitening in flashes.
    -- Its shadow rays stop short of it: the staff pokes into its underside,
    -- and a penumbra estimate taken right next to the light bands badly.
    -- The staff holds the orb on its tip, so its shadow rays towards the
    -- orb run along its own surface and graze every swelling and knot; a
    -- branch cannot really shadow a light at its own tip, so it is exempt.
    let c = scale (2.4 * g.level) (mix (vec 0.6 0.5 1) (vec 0.9 0.95 1) (0.6 * g.flash))
    in { pos = orb_pos, directional = false, diffuse = c, specular = c
       , falloff = 1.5, range = f32.inf, radius = orb_radius, shadowed = true, unshadowed = m_staff }
  else
    -- The potions: weak, unshadowed, reaching about half a unit, each with
    -- a slight flicker of its own (whole multiples of the loop).
    let i = k - 2
    let flicker = 1 + 0.12 * f32.sin (f32.i32 (7 + 3 * i) * pose.tm + f32.i32 i * 2.1)
    let c = scale (1.2 * flicker) (glow_colour (m_potion + i))
    in { pos = from_body_space pose (potion_centre i), directional = false, diffuse = c, specular = scale 0.3 c
       , falloff = 1 / (0.12 * 0.12), range = 0.5, radius = 0.04, shadowed = false, unshadowed = -1 }

-- The lit colour of a surface hit at distance t along rd (fog included).
def surface (pose: pose) (g: glow) (m: i32) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  -- The terrain's normal comes from its height gradient, worked out
  -- analytically (one evaluation instead of four of the whole scene), and it
  -- gets no AO: at its scale the probes see nothing.
  let n = if m == m_terrain
          then let w = to_world pose.fl p
               in to_local_dir pose.fl (terrain_normal (terrain_detail p) w.x w.z)
          else normal (dist pose) p
  -- textures tilt the normal used for lighting; shadows and AO keep the
  -- geometric one
  let fp = t * pose.pix
  let ng = n
  let n = if has_bump m then bumped (bump_height pose fp m) p ng else ng
  let mat = material pose g fp m p n
  let occ = if m == m_terrain then 1 else ambient_occlusion (dist pose) p ng
  let po = p +^ scale 0.002 ng
  -- (shadow rays that pass well clear of the figure and rock are fully lit)
  let shadow (l: vec3) (tmax: f32) =
    if shadow_misses occluder_centre occluder_radius po l tmax then 1
    -- (and they end shortly after leaving that sphere: nothing lies beyond,
    -- and the shadow loop's steps are capped at 0.25, so marching on to
    -- tmax only crawled through empty space)
    else soft_shadow (occluder_dist pose) po l
                     (f32.min tmax (sphere_exit occluder_centre occluder_radius po l + 1))
  -- All the lights, in one loop so the lighting and shadow code is
  -- compiled once. The moon's shadow is kept for the cloth sheen.
  let (lit, moon_shadow) =
    loop (lit, moon_shadow) = (splat 0, 0f32) for k < n_lights do
      let L = light_at pose g k
      let (l, d) = if L.directional then (L.pos, f32.inf)
                   else let v = L.pos -^ p
                        let d = length v
                        in (scale (1 / d) v, d)
      let atten = if L.directional then 1
                  else smoothstep L.range (0.6 * L.range) d / (1 + L.falloff * d * d)
      -- (skipped where the light is negligible, e.g. the orb on the far terrain)
      in if atten < 0.01 then (lit, moon_shadow) else
      let sh = if !L.shadowed || d < L.radius || m == L.unshadowed then 1
               else shadow l (if L.directional then 12 else d - L.radius - 0.15)
      let c = light mat n rd l sh (scale atten L.diffuse) (scale atten L.specular)
      in (lit +^ c, if k == 0 then sh else moon_shadow)
  let to_orb = orb_pos -^ p
  let od = length to_orb
  let ol = scale (1 / od) to_orb
  let fill = scale (occ * (0.5 + 0.5 * n.y) * 0.35) (mat.albedo *^ vec 0.2 0.22 0.45)
  let rim = if mat.rim then scale (occ * pow (1 - saturate (-(dot n rd))) 3 * 0.25) (vec 0.35 0.3 0.8)
            else splat 0
  -- Metals mirror their surroundings: the sky, and the glowing orb where
  -- the reflected ray points at it (it is right next to the gold cradle).
  let mirror =
    if !mat.metal then splat 0
    else let r = reflect rd n
         let cos_orb = dot r ol
         let disc = f32.cos (f32.asin (f32.min 1 (orb_radius / od)))
         let orb_seen = smoothstep (disc - 0.05) disc cos_orb + 0.25 * pow (saturate cos_orb) 6
         in mat.albedo *^ (scale 0.6 (sky (to_world_dir pose.fl r)) +^ scale (1.6 * g.level * orb_seen) (vec 0.75 0.9 1))
  -- cloth sheen: brightest where the surface turns away from view, lit by
  -- the moon where it reaches and a little by the sky everywhere
  let sheen =
    if mat.sheen <= 0 then splat 0
    else let f = pow (1 - saturate (-(dot n rd))) 4
         let lit = 0.3 * occ + 0.7 * moon_shadow * saturate (dot n pose.moon + 0.4)
         in scale (mat.sheen * f * lit) (scale 2.2 mat.albedo +^ vec 0.05 0.05 0.1)
  -- hair: a Kajiya-Kay highlight, a band running across the strands
  let hair_spec =
    if m != m_beard then splat 0
    else let hh = normalise (pose.moon -^ rd)
         let th = dot hair_tangent hh
         let k = pow (f32.sqrt (f32.max 0 (1 - th * th))) 60
         in scale (0.35 * k * moon_shadow * saturate (dot n pose.moon + 0.2)) (vec 0.75 0.8 1)
  let c = lit +^ fill +^ rim +^ mirror +^ sheen +^ hair_spec +^ mat.emission
  let fog = 1 - f32.exp (-0.0007 * t * t)
  in mix c (sky (to_world_dir pose.fl rd)) fog

---------------------------------------------------------------------
-- The glass orb

-- Compile-time switch: reflect the scene in the orb's glass too.
def orb_reflections : bool = true
def orb_ior : f32 = 1.45
def glass_tint : vec3 = vec 0.93 0.97 1

-- Distances (entry, exit) where the ray meets the orb; entry > exit on a miss.
def orb_hit (ro: vec3) (rd: vec3) : (f32, f32) = ray_sphere orb_pos orb_radius ro rd

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

-- The potions' glass, a thin wall rays pass through. Finds where the ray
-- (up to t_end) first meets it and returns what the wall adds there (a
-- Fresnel reflection of the sky, highlights from the moon and the orb, and a
-- faint rim) and what it lets through (Fresnel transmission, slightly
-- tinted). Only rays passing near the potions are traced.
def glass (pose: pose) (g: glow) (ro: vec3) (rd: vec3) (t_end: f32) : (vec3, vec3) =
  -- the potions' bounding sphere, in local space (body space leans)
  let c = from_body_space pose potions_at
  let r = potions_radius + 0.02
  let (enter, exit) = ray_sphere c r ro rd
  in if enter > exit then (splat 0, splat 1) else
  let t0 = f32.max 0.02 enter
  let t1 = f32.min t_end exit
  let (t, _, hit) =
    loop (t, i, hit) = (t0, 0i32, false) while !hit && i < 64 && t < t1 do
      let d = potion_glass pose (ro +^ scale t rd)
      in if d < 0.0004 * t then (t, i + 1, true) else (t + d * 0.9, i + 1, false)
  in if !hit then (splat 0, splat 1) else
  let p = ro +^ scale t rd
  let n = normal (\_ q -> potion_glass pose q) p
  let n = if dot n rd > 0 then scale (-1) n else n
  let cosi = saturate (-(dot n rd))
  let f = 0.04 + 0.96 * pow (1 - cosi) 5
  let rr = reflect rd n
  let to_orb = normalise (orb_pos -^ p)
  let highlights = scale (2 * pow (saturate (dot rr pose.moon)) 300) (vec 0.8 0.85 1)
                   +^ scale (2.5 * g.level * pow (saturate (dot rr to_orb)) 120) (vec 0.7 0.85 1)
  let added = scale f (sky (to_world_dir pose.fl rr)) +^ highlights +^ scale (0.08 * pow (1 - cosi) 2) (vec 0.6 0.7 0.9)
  in (added, scale (1 - f) (vec 0.9 0.95 0.97))

-- Where a reflected path starts: (exists, point, direction, weight).
type reflection = (bool, vec3, vec3, f32)

-- Follows a ray through at most three straight segments - outside, inside
-- the orb, outside again - refracting at the glass, and returns the colour
-- it gathers, weighted by w. Also returns the reflection spawned where the
-- ray first enters the orb.
def follow (pose: pose) (g: glow) (ro0: vec3) (rd0: vec3) (w0: f32) : (vec3, reflection) =
  let (_, _, _, acc, _, _, refl) =
    loop (ro, rd, w, acc, inside, done, refl) =
      (ro0, rd0, splat w0, splat 0, false, false, (false, ro0, rd0, 0f32))
    for _seg < 3 do
      if done then (ro, rd, w, acc, inside, done, refl) else
      let (t_in, t_out) = orb_hit ro rd
      let enters = !inside && t_in > 1e-3 && t_in < t_out
      let tmax = if inside then t_out else if enters then t_in else 60
      -- Two passes: the terrain alone over the whole segment, then the scene
      -- near the wizard up to the terrain hit or the edge of the sphere that
      -- holds it (the peaks reach into that sphere, so the terrain goes
      -- first). Neither pass evaluates the other's geometry at every step.
      let t_terrain = if inside || enters then f32.inf else march_terrain pose ro rd tmax
      let local_end = f32.max 0 (ray_sphere occluder_centre local_radius ro rd).1
      let local_max = f32.min tmax (f32.min t_terrain local_end)
      let (t, m, emitted, tr) = march (local_scene pose) (media pose g) ro rd local_max
      let (t, m) = if m < 0 && t_terrain < tmax then (t_terrain, m_terrain) else (t, m)
      -- the glows are above the cloud slab, so in front of the clouds
      let (cloud_rad, cloud_tr) = clouds pose ro rd (if m >= 0 then t else tmax)
      -- the potions' glass, in front of whatever this segment reaches
      let (glass_added, glass_through) =
        if inside then (splat 0, splat 1) else glass pose g ro rd (if m >= 0 then t else local_max)
      let acc = acc +^ (w *^ glass_added)
      let w = w *^ glass_through
      let acc = acc +^ (w *^ emitted) +^ (scale tr w *^ cloud_rad)
      -- whatever lies beyond this segment is seen through its media
      let w = scale (tr * cloud_tr) w
      in if m >= 0 then
           (ro, rd, w, acc +^ (w *^ surface pose g m (ro +^ scale t rd) rd t), inside, true, refl)
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
         else (ro, rd, w, acc +^ (w *^ sky (to_world_dir pose.fl rd)), inside, true, refl)
  in (acc, refl)

def shade (secs: f32) (pix: f32) (ro: vec3) (rd: vec3) : vec3 =
  -- worked out once per pixel, not per march step
  let pose = pose_at secs with pix = pix
  let g = glow_now pose.tm
  -- Path 0 is the camera ray; path 1 its reflection off the orb, if any.
  -- A loop, so that follow (and all the shading) is compiled only once.
  let paths = if orb_reflections then 2 else 1
  let (c, _) =
    loop (c, refl) = (splat 0, (false, ro, rd, 0f32)) for k < paths do
      if k == 1 && !refl.0 then (c, refl) else
      let (o, d, w) = if k == 0 then (ro, rd, 1) else (refl.1, refl.2, refl.3)
      let (col, r) = follow pose g o d w
      in (c +^ col, if k == 0 then r else refl)
  in c
