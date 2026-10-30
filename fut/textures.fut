-- Surface textures: bark on the staff, cloth on the robe, leather on the
-- belt. Each material has a height field (in world units, along the normal)
-- that tilts the shading normal (bump mapping), and colour variation.
--
-- Fine detail aliases once a pixel covers more than a fraction of a
-- wavelength, so every fine term is faded by the pixel's footprint on the
-- surface, fp (distance times the angle one pixel spans).

import "vec3"
import "sdf"
import "world"
import "scene"

-- 1 while the footprint is under a quarter wavelength, 0 beyond a half.
def keep (fp: f32) (wavelength: f32) : f32 = smoothstep (0.5 * wavelength) (0.25 * wavelength) fp

---------------------------------------------------------------------
-- Bark: plates separated by furrows, running along the staff. The plates
-- are contour bands of a two-octave 3D noise stretched along the axis (so
-- the contours run lengthwise, with no seam around the staff); finer cracks
-- on top. (Plain ridged value noise sits near its maximum almost
-- everywhere, so it shows hardly any furrows.)

-- (height, plate in [0, 1]: 0 in a furrow, 1 on a plate's crown)
def bark (fp: f32) (p: vec3) : (f32, f32) =
  let q = staff_local p
  let s = vec (q.x * 75) (q.y * 6) (q.z * 75)
  let n = 0.65 * vnoise3 s + 0.35 * vnoise3 (scale 2.3 s)
  -- about 30-50 plates round the staff; their noise changes by ~80 per
  -- unit across it, so a band is ~0.005 wide and this amplitude keeps the
  -- bump's slope near 0.5
  let band = fract (n * 3)
  let plate = (0.5 - 0.5 * f32.cos (2 * f32.pi * band)) * keep fp 0.005
  let cracks = vnoise3 (vec (s.x * 2.7) (s.y * 5) (s.z * 2.7)) * keep fp 0.003
  in (0.0008 * plate + 0.0002 * cracks, plate)

def bark_albedo (fp: f32) (p: vec3) : vec3 =
  let (_, plate) = bark fp p
  let q = staff_local p
  -- far away (plate faded to 0) the colour averages to a mid brown
  let k = keep fp 0.005
  let t = mix1 0.55 (smoothstep 0.1 0.7 plate) k
  let base = mix (vec 0.06 0.04 0.025) (vec 0.42 0.29 0.17) t
  -- a few grey-green patches of lichen
  let lichen = smoothstep 0.8 0.86 (vnoise3 (vec (q.x * 60) (q.y * 9) (q.z * 60)))
  in mix base (vec 0.3 0.33 0.25) (0.6 * lichen)

---------------------------------------------------------------------
-- Cloth: soft vertical folds, deeper towards the hem, and (close up) a
-- basket weave. Measured in the robe's own space, so it leans with him.

def cloth_coords (pose: pose) (p: vec3) : (f32, f32, f32) = -- (angle, arc length, height)
  let b = body_space pose p
  let a = f32.atan2 b.z b.x
  in (a, a * length2 (vec2 b.x b.z), b.y)

def fold (a: f32) (y: f32) : f32 =
  f32.sin (a * 13 + 0.7 * f32.sin (a * 5 + y * 2)) * (0.25 + 0.75 * saturate (1 - y / 1.1))

def cloth (pose: pose) (fp: f32) (p: vec3) : f32 =
  let (a, u, y) = cloth_coords pose p
  -- basket weave: in alternate cells the warp (running up) or the weft
  -- (running round) is on top, each a rounded thread
  let weave_w = 0.008
  let cu = f32.floor (u / weave_w)
  let cy = f32.floor (y / weave_w)
  let warp_on_top = (i32.f32 cu + i32.f32 cy) % 2 == 0
  let thread (x: f32) = f32.sin (f32.pi * fract (x / weave_w))
  let weave = if warp_on_top then thread u else thread y
  in 0.012 * fold a y + 0.00025 * weave * keep fp (2 * weave_w)

def cloth_albedo (pose: pose) (p: vec3) : vec3 =
  let (a, _, y) = cloth_coords pose p
  let mottle = 0.9 + 0.2 * vnoise3 (scale 25 p)
  -- the creases of the folds catch less light and look deeper in colour
  in scale (mottle * (0.82 + 0.18 * (0.5 + 0.5 * fold a y))) (vec 0.07 0.05 0.33)

---------------------------------------------------------------------
-- Leather: a pebbled grain and, on the belt, a row of stitches along each
-- edge, with the edges worn darker.

-- (arc length round the body, height above the belt's centre, on the belt?)
def belt_coords (pose: pose) (p: vec3) : (f32, f32, bool) =
  let b = body_space pose p
  let dy = b.y - belt_y
  let r = length2 (vec2 b.x b.z)
  in (f32.atan2 b.x b.z * robe_radius belt_y, dy, f32.abs dy < 0.035 && f32.abs (r - robe_radius b.y) < 0.03)

-- How much of a stitch is at this point, in [0, 1].
def stitch (u: f32) (dy: f32) : f32 =
  let across = f32.min (f32.abs (dy - 0.017)) (f32.abs (dy + 0.017))
  let phase = fract (u / 0.011)
  let along = if phase < 0.65 then f32.sqrt (f32.sin (f32.pi * phase / 0.65)) else 0
  in along * smoothstep 0.0026 0.0008 across

def leather (pose: pose) (fp: f32) (p: vec3) : f32 =
  let (u, dy, on_belt) = belt_coords pose p
  let g = vnoise3 (scale 85 p)
  let grain = 0.0006 * g * g * keep fp 0.012
  let st = if on_belt then 0.0011 * stitch u dy * keep fp 0.011 else 0
  in grain + st

def leather_albedo (pose: pose) (fp: f32) (p: vec3) : vec3 =
  let (u, dy, on_belt) = belt_coords pose p
  let g = vnoise3 (scale 85 p)
  let base = scale (0.75 + 0.35 * mix1 0.6 g (keep fp 0.012)) (vec 0.17 0.095 0.05)
  in if !on_belt then base
     else let worn = mix1 1 0.6 (smoothstep 0.018 0.03 (f32.abs dy))
          let s = stitch u dy * keep fp 0.011
          in mix (scale worn base) (vec 0.55 0.45 0.3) s

---------------------------------------------------------------------

-- Height field of material m, or 0 where it has none.
def bump_height (pose: pose) (fp: f32) (m: i32) (p: vec3) : f32 =
  if m == m_staff then (bark fp p).0
  else if m == m_robe then cloth pose fp p
  else if m == m_leather then leather pose fp p
  else 0

def has_bump (m: i32) : bool = m == m_staff || m == m_robe || m == m_leather

-- Tilts normal n by the gradient of the height field h across the surface.
-- The gradient comes from four taps on a tetrahedron, small enough for the
-- finest features (the stitches are 0.0016 wide).
def bumped (h: vec3 -> f32) (p: vec3) (n: vec3) : vec3 =
  let e = 0.0004
  let g = loop g = splat 0 for i < 4 do
            let k = if i == 0 then vec 1 (-1) (-1)
                    else if i == 1 then vec (-1) (-1) 1
                    else if i == 2 then vec (-1) 1 (-1)
                    else vec 1 1 1
            in g +^ scale (h (p +^ scale e k)) k
  let g = scale (1 / (4 * e)) g
  let tangential = g -^ scale (dot g n) n
  in normalise (n -^ tangential)
