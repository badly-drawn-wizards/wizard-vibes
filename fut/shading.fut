-- Materials, sky and lighting for the wizard scene.

import "vec3"
import "params"
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

-- The night sky in a world direction: a gradient, the moon, and stars.
def sky (pose: pose) (rd: vec3) : vec3 =
  let up = saturate (rd.y * 0.5 + 0.5)
  let base = mix pose.prm.sky_horizon pose.prm.sky_zenith (f32.sqrt up)
  -- The moon: a disc of angular radius moon_size, with darker maria and
  -- limb darkening, in a soft halo.
  let m = pose.moon_world
  let r = pose.prm.moon_size
  let md = dot rd m
  let ang = f32.acos (clamp (-1) 1 md) -- angle from the moon's centre
  let rr = ang / r -- 0 at the centre, 1 at the limb
  let moon =
    if rr > 1.05 then 0 else
    let e1 = normalise (cross m (vec 0 1 0))
    let e2 = cross e1 m
    let u = dot rd e1 / r
    let v = dot rd e2 / r
    let maria = smoothstep 0.5 0.65 (0.6 * vnoise2 (u * 1.8 + 3.1) (v * 1.8 - 1.7) + 0.4 * vnoise2 (u * 4.3) (v * 4.3 + 5))
    let craters = smoothstep 0.72 0.8 (vnoise2 (u * 9 + 11) (v * 9 - 7))
    let limb = f32.sqrt (f32.max 0 (1 - rr * rr)) ** 0.4
    in smoothstep 1 0.96 rr * (0.55 + 0.45 * limb) * (1 - 0.45 * maria) * (1 - 0.2 * craters)
  let halo = 0.15 * f32.exp (-(1 - md) * 60) + pose.prm.moon_halo * f32.exp (-f32.max 0 (ang - r) / 0.06)
  -- Stars: a few cells of a direction grid each hold a small round star at a
  -- random point (kept off the cell's edges), twinkling at its own rate.
  let q = scale 220 rd
  let c = vmap f32.floor q
  let (ix, iy, iz) = (i32.f32 c.x, i32.f32 c.y, i32.f32 c.z)
  let s = hash3 ix iy iz
  let star =
    let thr = 1 - pose.prm.star_density
    in if s <= thr || rd.y <= 0.02 then 0 else
    let at = c +^ vec (0.2 + 0.6 * hash3 ix iy (iz + 71)) (0.2 + 0.6 * hash3 (ix + 13) iy iz) (0.2 + 0.6 * hash3 ix (iy + 29) iz)
    let d = length (q -^ at) / 0.22 -- in units of the star's size (~1.3 pixels at 1080p)
    let rate = 2 + 5 * hash3 (ix + 5) (iy + 3) iz
    let twinkle = 1 - pose.prm.twinkle + pose.prm.twinkle * f32.sin (pose.secs * rate + 6.28 * hash3 ix (iy + 7) (iz + 3))
    in (s - thr) / 0.0025 * pose.prm.star_brightness * 0.0025 * f32.exp (-(d * d)) * twinkle * saturate (rd.y * 4) * smoothstep 1.2 2 rr
  in base +^ scale (moon * pose.prm.moon_brightness + halo) (vec 0.85 0.9 1) +^ splat star

---------------------------------------------------------------------
-- Materials

-- Metals tint their highlights with their albedo and have little diffuse.
-- sheen: a cloth-like glow at grazing angles, tinted by the albedo.
type material = {albedo: vec3, emission: vec3, ks: f32, shininess: f32, rim: bool, metal: bool, sheen: f32}

def plain (albedo: vec3) : material =
  {albedo, emission = splat 0, ks = 0.08, shininess = 8, rim = true, metal = false, sheen = 0}

-- The structures' materials, with lit windows on cottages and towers.
def structure_material (pose: pose) (m: i32) (p: vec3) : material =
  let prm = pose.prm
  let w = to_world pose.fl p
  let (_, kind, o, yaw) = structure_at (land_of prm) prm.structure_density w
  let q = yaw_inv yaw (w -^ o)
  let grain = 0.85 + 0.3 * vnoise2 (w.x * 3 + w.z) (w.y * 3)
  let lit (window: bool) = if window then scale prm.window_glow prm.window_colour else splat 0
  in if m == m_wall then
       -- windows on the long walls (two each) and the gable ends (one each),
       -- and a door on the front
       let long = f32.abs q.z > 1.25 && f32.abs (f32.abs q.x - 0.9) < 0.22 && f32.abs (q.y - 1.25) < 0.28
       let gable = f32.abs q.x > 1.75 && f32.abs q.z < 0.22 && f32.abs (q.y - 1.25) < 0.28
       let door = q.z > 1.25 && f32.abs q.x < 0.3 && q.y < 1.5
       let window = (long || gable) && kind == kind_cottage
       let c = if door then vec 0.18 0.1 0.05 else scale grain prm.wall_colour
       in plain (if window then splat 0.05 else c) with emission = lit window
     else if m == m_thatch then plain (scale (0.7 + 0.5 * vnoise2 (q.x * 9) (q.y * 20)) prm.thatch_colour)
     else if m == m_tower then
       -- four narrow lit windows round the tower, on two floors
       let a = f32.atan2 q.z q.x / (2 * f32.pi) * 4
       let slot = f32.abs (fract a - 0.5) < 0.06
       let floor = (q.y > 2.3 && q.y < 3) || (q.y > 4 && q.y < 4.6)
       let window = slot && floor
       let block = 0.8 + 0.2 * hash2 (i32.f32 (f32.floor (a * 6))) (i32.f32 (f32.floor (q.y * 3)))
       in plain (if window then splat 0.05 else scale (block * grain) prm.tower_colour) with emission = lit window
     else if m == m_slate then plain prm.slate_colour with ks = 0.3 with shininess = 30
     else plain (scale grain prm.sandstone_colour)

-- The water: rippled (the ripples fade with the pixel footprint), dark below,
-- reflecting the sky by Fresnel, with a glint of the moon.
def water_surface (pose: pose) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  let w = to_world pose.fl p
  let s = pose.secs
  let k = keep (t * pose.pix) 0.6
  let (_, ax, az) = vnoise2_grad (w.x * 1.3 + s * 0.3) (w.z * 1.3 - s * 0.2)
  let (_, bx, bz) = vnoise2_grad (w.x * 3.1 - s * 0.5) (w.z * 3.1 + s * 0.4)
  let gx = k * (0.04 * ax * 1.3 + 0.015 * bx * 3.1)
  let gz = k * (0.04 * az * 1.3 + 0.015 * bz * 3.1)
  let n = to_local_dir pose.fl (normalise (vec (-gx) 1 (-gz)))
  let cosi = saturate (-(dot n rd))
  let f = 0.02 + 0.98 * pow (1 - cosi) 5
  let r = reflect rd n
  let glint = scale (1.5 * pow (saturate (dot r pose.moon)) 400) (vec 0.85 0.9 1)
  let c = mix pose.prm.water_colour (sky pose (to_world_dir pose.fl r)) f +^ glint
  let fog = 1 - f32.exp (-pose.prm.fog * t * t)
  in mix c (sky pose (to_world_dir pose.fl rd)) fog

def material (pose: pose) (_g: glow) (fp: f32) (m: i32) (p: vec3) (n: vec3) : material =
  let tm = pose.tm in
  if m == m_robe then plain (cloth_albedo pose p) with rim = false with sheen = pose.prm.sheen with ks = 0.03
  else if m == m_staff then plain (bark_albedo pose.prm fp p) with ks = 0.05
  else if m == m_skin then plain pose.prm.skin_colour with ks = 0.15 with shininess = 16
  else if m == m_beard then plain (hair_albedo pose p) with ks = 0
  else if m == m_hat then
    let s = (hat_stars pose fp p).0
    in plain (mix pose.prm.hat_colour pose.prm.star_colour s) with emission = scale (pose.prm.star_glow * s) pose.prm.star_colour
  else if m == m_gold then plain pose.prm.gold_colour with ks = 1.6 with shininess = 48 with metal = true
  else if m == m_band then plain pose.prm.band_colour with ks = 1.6 with shininess = 48 with metal = true
  else if m == m_eye then plain (eye_albedo pose p) with ks = 1 with shininess = 150 with rim = false
  else if m == m_brow then plain pose.prm.brow_colour
  else if m == m_wood then
    plain (scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1))
  else if m == m_rock then
    -- red-orange sandstone, banded and mottled (strongly saturated, since
    -- the blue moonlight mutes warm colours)
    let band = 0.5 + 0.5 * f32.sin (p.y * 9 + 2 * vnoise3 (scale 1.5 p))
    let c = mix pose.prm.rock_dark pose.prm.rock_light band
    in plain (scale (0.6 + 0.7 * vnoise3 (scale 4 p)) c) with ks = 0.1
  else if m == m_terrain then
    -- moss in the valleys, bare rock on steep slopes, snow on high flats
    let w = to_world pose.fl p
    let up = (to_world_dir pose.fl n).y
    let grain = 0.75 + 0.5 * vnoise2 (w.x * 1.7) (w.z * 1.7)
    let moss = pose.prm.moss_colour
    let stone = pose.prm.stone_colour
    let snow = pose.prm.snow_colour
    let c = mix stone moss (smoothstep 0.7 0.85 up)
    let c = mix c snow (smoothstep pose.prm.snow_line (pose.prm.snow_line + 1.5) w.y * smoothstep 0.6 0.8 up)
    in plain (scale grain c) with rim = false
  else if m >= m_wall && m <= m_sandstone then structure_material pose m p
  else if m == m_leather then plain (leather_albedo pose fp p) with ks = 0.35 with shininess = 28
  else if m == m_cork then plain (cork_albedo fp p) with ks = 0.05
  else if m == m_tome then plain pose.prm.tome_colour with ks = 0.15 with shininess = 16
  else if m == m_pages then plain (vec 0.85 0.8 0.66)
  else if m >= m_potion && m < m_potion + 3 then
    -- the glowing liquid (its glass is drawn by `glass`)
    let c = glow_colour pose.prm m
    in plain (scale 0.3 c) with emission = scale 0.8 c with ks = 0.6 with shininess = 60
  else if m == m_ground then
    let tile = hash2 (i32.f32 (f32.floor (p.x * 2))) (i32.f32 (f32.floor (p.z * 2)))
    in plain (scale (0.8 + 0.4 * tile) pose.prm.rock_top)
       with emission = scale (0.9 * rune_pulse pose.prm.rune_pulse tm * rune_paint tm p) pose.prm.rune_colour
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
-- size stop their shadow rays short of it. Which layers it involves:
--   casters             block it (local layers only: shadow rays are only
--                       traced through the sphere around the wizard);
--   receivers           are lit by it;
--   shadowed_receivers  of those, take its shadows.
type light = { pos: vec3, directional: bool
             , diffuse: vec3, specular: vec3
             , falloff: f32, range: f32, radius: f32
             , casters: i32, receivers: i32, shadowed_receivers: i32 }

-- The moon, the orb, and a weak light in each potion.
def n_lights : i32 = 5

def light_at (pose: pose) (g: glow) (k: i32) : light =
  if k == 0 then
    { pos = pose.moon, directional = true
    , diffuse = scale pose.prm.moon_strength pose.prm.moon_colour
    , specular = scale pose.prm.moon_strength (vec 0.7 0.75 1)
    , falloff = 0, range = f32.inf, radius = 0
    , casters = local_layers, receivers = layers_all, shadowed_receivers = layers_all }
  else if k == 1 then
    -- The orb's light flickers with the lightning, whitening in flashes.
    -- Its shadow rays stop short of it: the staff pokes into its underside,
    -- and a penumbra estimate taken right next to the light bands badly.
    -- The staff holds the orb on its tip, so its shadow rays towards the
    -- orb run along its own surface and graze every swelling and knot; a
    -- branch cannot really shadow a light at its own tip, so the staff
    -- layer takes no shadows from it.
    let c = scale (pose.prm.orb_light * g.level) (mix (vec 0.6 0.5 1) (vec 0.9 0.95 1) (0.6 * g.flash))
    in { pos = orb_pos, directional = false, diffuse = c, specular = c
       , falloff = 1.5, range = f32.inf, radius = orb_radius
       , casters = local_layers, receivers = layers_all, shadowed_receivers = layers_all & !layers_staff }
  else
    -- The potions: weak, unshadowed, reaching about half a unit and only
    -- lighting the wizard's things, each with a slight flicker of its own
    -- (whole multiples of the loop).
    let i = k - 2
    let flicker = 1 + 0.12 * f32.sin (f32.i32 (7 + 3 * i) * pose.tm + f32.i32 i * 2.1)
    let c = scale (pose.prm.potion_light * flicker) (glow_colour pose.prm (m_potion + i))
    in { pos = from_body_space pose (potion_centre pose.prm i), directional = false, diffuse = c, specular = scale 0.3 c
       , falloff = 1 / (0.12 * 0.12), range = 0.5, radius = 0.04
       , casters = 0, receivers = figure_layers, shadowed_receivers = 0 }

-- The lit colour of a surface hit at distance t along rd (fog included);
-- m is the hit's tagged material (with its layer).
def surface (pose: pose) (g: glow) (tagged: i32) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  let layer = hit_layer tagged
  let m = hit_material tagged
  let vis = pose.prm.layers -- only visible layers occlude, or show in AO
  in if m == m_water then water_surface pose p rd t else
  -- The terrain's normal comes from its height gradient, worked out
  -- analytically (one evaluation instead of four of the whole scene), and it
  -- gets no AO: at its scale the probes see nothing.
  let n = if m == m_terrain
          then let w = to_world pose.fl p
               in to_local_dir pose.fl (terrain_normal (land_of pose.prm) (terrain_detail p) w.x w.z)
          else normal (dist vis pose) p
  -- textures tilt the normal used for lighting; shadows and AO keep the
  -- geometric one
  let fp = t * pose.pix
  let ng = n
  let n = if has_bump m then bumped (bump_height pose fp m) p ng else ng
  let mat = material pose g fp m p n
  let occ = if m == m_terrain then 1 else ambient_occlusion (dist vis pose) p ng
  let po = p +^ scale 0.002 ng
  -- (shadow rays that pass well clear of the figure and rock are fully lit)
  let shadow (casters: i32) (l: vec3) (tmax: f32) =
    let casters = casters & vis & local_layers
    in if casters == 0 || shadow_misses occluder_centre occluder_radius po l tmax then 1
    -- (and they end shortly after leaving that sphere: nothing lies beyond,
    -- and the shadow loop's steps are capped at 0.25, so marching on to
    -- tmax only crawled through empty space)
    else soft_shadow (\tol q -> (local_scene casters pose tol q).0) po l
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
      in if atten < 0.01 || !(has L.receivers layer) then (lit, moon_shadow) else
      let sh = if !(has L.shadowed_receivers layer) || d < L.radius then 1
               else shadow L.casters l (if L.directional then 12 else d - L.radius - 0.15)
      let c = light mat n rd l sh (scale atten L.diffuse) (scale atten L.specular)
      in (lit +^ c, if k == 0 then sh else moon_shadow)
  let to_orb = orb_pos -^ p
  let od = length to_orb
  let ol = scale (1 / od) to_orb
  let fill = scale (occ * (0.5 + 0.5 * n.y) * pose.prm.fill) (mat.albedo *^ vec 0.2 0.22 0.45)
  let rim = if mat.rim then scale (occ * pow (1 - saturate (-(dot n rd))) 3 * pose.prm.rim) (vec 0.35 0.3 0.8)
            else splat 0
  -- Metals mirror their surroundings: the sky, and the glowing orb where
  -- the reflected ray points at it (it is right next to the gold cradle).
  let mirror =
    if !mat.metal then splat 0
    else let r = reflect rd n
         let cos_orb = dot r ol
         let disc = f32.cos (f32.asin (f32.min 1 (orb_radius / od)))
         let orb_seen = smoothstep (disc - 0.05) disc cos_orb + 0.25 * pow (saturate cos_orb) 6
         in mat.albedo *^ (scale 0.6 (sky pose (to_world_dir pose.fl r)) +^ scale (1.6 * g.level * orb_seen) (vec 0.75 0.9 1))
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
  let fog = 1 - f32.exp (-pose.prm.fog * t * t)
  in match pose.prm.view
     case 1 -> scale 0.5 (n +^ splat 1)
     case 2 -> mat.albedo
     case 3 -> splat occ
     case _ -> mix c (sky pose (to_world_dir pose.fl rd)) fog

---------------------------------------------------------------------
-- The glass orb

-- (whether the orb reflects, and its index of refraction, are uniforms:
-- orb_reflections and orb_ior in params.json)
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
def fresnel (ior: f32) (cosi: f32) : f32 =
  let r0 = ((1 - ior) / (1 + ior)) ** 2
  in r0 + (1 - r0) * pow (1 - saturate cosi) 5

-- The potions' glass, a thin wall rays pass through. Finds where the ray
-- (up to t_end) first meets it and returns what the wall adds there (a
-- Fresnel reflection of the sky, highlights from the moon and the orb, and a
-- faint rim) and what it lets through (Fresnel transmission, slightly
-- tinted). Only rays passing near the potions are traced.
def glass (pose: pose) (g: glow) (ro: vec3) (rd: vec3) (t_end: f32) : (vec3, vec3) =
  -- the potions' bounding sphere, in local space (body space leans)
  let c = from_body_space pose (potions_at pose.prm)
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
  let added = scale f (sky pose (to_world_dir pose.fl rr)) +^ highlights +^ scale (0.08 * pow (1 - cosi) 2) (vec 0.6 0.7 0.9)
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
      let vis = pose.prm.layers
      -- (a hidden orb is no glass at all)
      let (t_in, t_out) = if has vis layers_orb then orb_hit ro rd else (f32.inf, -f32.inf)
      let enters = !inside && t_in > 1e-3 && t_in < t_out
      let tmax = if inside then t_out else if enters then t_in else 60
      -- Two passes: the terrain alone over the whole segment, then the scene
      -- near the wizard up to the terrain hit or the edge of the sphere that
      -- holds it (the peaks reach into that sphere, so the terrain goes
      -- first). Neither pass evaluates the other's geometry at every step.
      let (t_terrain, m_land) = if inside || enters || !(has vis land_layers) then (f32.inf, 0) else march_terrain vis pose ro rd tmax
      let local_end = f32.max 0 (ray_sphere occluder_centre local_radius ro rd).1
      let local_max = f32.min tmax (f32.min t_terrain local_end)
      let (t, m, emitted, tr) = march (local_scene vis pose) (media pose g) ro rd local_max
      let (t, m) = if m < 0 && t_terrain < tmax then (t_terrain, m_land) else (t, m)
      -- the glows are above the cloud slab, so in front of the clouds
      let (cloud_rad, cloud_tr) = if has vis layers_clouds then clouds pose ro rd (if m >= 0 then t else tmax)
                                  else (splat 0, 1)
      -- the potions' glass, in front of whatever this segment reaches
      let (glass_added, glass_through) =
        if inside || !(has vis layers_gear) then (splat 0, splat 1) else glass pose g ro rd (if m >= 0 then t else local_max)
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
           let f = fresnel pose.prm.orb_ior (-(dot n rd))
           let refl = if refl.0 then refl else (true, p, reflect rd n, w.x * f)
           in (p, refract rd n (1 / pose.prm.orb_ior), scale (1 - f) (w *^ glass_tint), acc, true, false, refl)
         else if inside && t >= t_out then
           -- out of the glass
           let p = ro +^ scale t_out rd
           let n = normalise (p -^ orb_pos)
           in (p, refract rd (scale (-1) n) pose.prm.orb_ior, w *^ glass_tint, acc, false, false, refl)
         else (ro, rd, w, acc +^ (w *^ sky pose (to_world_dir pose.fl rd)), inside, true, refl)
  in (acc, refl)

def shade (prm: params) (dr: drive) (secs: f32) (pix: f32) (ro: vec3) (rd: vec3) : vec3 =
  -- worked out once per pixel, not per march step
  let pose = pose_at secs prm dr with pix = pix
  let g = glow_now pose.tm
  -- Path 0 is the camera ray; path 1 its reflection off the orb, if any.
  -- A loop, so that follow (and all the shading) is compiled only once.
  let paths = if prm.orb_reflections then 2 else 1
  let (c, _) =
    loop (c, refl) = (splat 0, (false, ro, rd, 0f32)) for k < paths do
      if k == 1 && !refl.0 then (c, refl) else
      let (o, d, w) = if k == 0 then (ro, rd, 1) else (refl.1, refl.2, refl.3)
      let (col, r) = follow pose g o d w
      in (c +^ col, if k == 0 then r else refl)
  in c
