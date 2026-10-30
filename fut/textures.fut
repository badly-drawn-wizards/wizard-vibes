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
-- Bark comes in three scales, each fading only once a pixel covers it, so
-- the staff keeps a bark look at every distance rather than averaging to a
-- flat brown: fine plates (~0.005 across), broad plates and furrows
-- (~0.025), and lighter and darker patches along the length (never faded).

-- One scale of plates: (height, plate in [0, 1]; 0 in a furrow, faded to
-- 0.5 once too small to show). `across`/`along` are noise frequencies per
-- unit; `bands` sets how many plates per unit of noise.
def plates (fp: f32) (wavelength: f32) (across: f32) (along: f32) (bands: f32) (q: vec3) : f32 =
  let s = vec (q.x * across) (q.y * along) (q.z * across)
  let n = 0.65 * vnoise3 s + 0.35 * vnoise3 (scale 2.3 s)
  let band = fract (n * bands)
  in mix1 0.5 (0.5 - 0.5 * f32.cos (2 * f32.pi * band)) (keep fp wavelength)

-- (height, fine plate, broad plate, patch tone)
def bark (fp: f32) (p: vec3) : (f32, f32, f32, f32) =
  let q = staff_local p
  -- the noise changes by ~80 per unit across the staff at the fine scale
  -- and ~25 at the broad one; these amplitudes keep each bump's slope
  -- near 0.5
  let fine = plates fp 0.005 75 6 3 q
  let broad = plates fp 0.025 25 2.5 2 q
  let tone = vnoise3 (vec (q.x * 8) (q.y * 1.6 + 3.1) (q.z * 8))
  let cracks = vnoise3 (vec (q.x * 200) (q.y * 30) (q.z * 200)) * keep fp 0.003
  in (0.0008 * fine + 0.002 * broad + 0.0002 * cracks, fine, broad, tone)

def bark_albedo (fp: f32) (p: vec3) : vec3 =
  let (_, fine, broad, tone) = bark fp p
  let q = staff_local p
  -- Bark reads as narrow dark crevices between lighter plates: darken only
  -- near each plate's edge (plate values under ~0.2), at both scales.
  let crevice (v: f32) = smoothstep 0.2 0.0 v
  let wood = vec 0.4 0.28 0.16
  let base = scale ((1 - 0.85 * crevice broad) * (1 - 0.6 * crevice fine)) wood
  -- lighter and darker stretches along the length
  let base = scale (0.7 + 0.6 * tone) base
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
-- Hair: fine strands within each beard lock, faded by footprint, and the
-- locks' colours varying from white through cream to light grey.

def hair (pose: pose) (fp: f32) (p: vec3) : f32 =
  let l = beard_lock (beard_space pose p)
  -- six strands per lock; a lock is ~0.035 across, so ~0.006 per strand
  let strand = f32.abs (f32.sin (f32.pi * fract (l.phase * 6 + 0.3 * f32.sin (l.h * 20))))
  in 0.0003 * strand * keep fp 0.006

def hair_albedo (pose: pose) (p: vec3) : vec3 =
  let l = beard_lock (beard_space pose p)
  let tint = mix (vec 0.93 0.92 0.88) (vec 0.78 0.79 0.82) l.rnd
  in scale (0.62 + 0.38 * l.crown) tint

-- Direction the hair runs (down the beard), in the wizard's local space.
def hair_tangent : vec3 = normalise (beard_tip -^ beard_top)

---------------------------------------------------------------------
-- Cork: tan, with dark speckled pores (small pits in the bump).

def cork_pores (fp: f32) (p: vec3) : f32 =
  smoothstep 0.7 0.82 (vnoise3 (scale 420 p)) * keep fp 0.004

def cork (fp: f32) (p: vec3) : f32 = -0.0002 * cork_pores fp p

def cork_albedo (fp: f32) (p: vec3) : vec3 =
  let mottle = 0.85 + 0.3 * vnoise3 (scale 120 p)
  in scale (mottle * (1 - 0.45 * cork_pores fp p)) (vec 0.62 0.4 0.2)

---------------------------------------------------------------------

-- Height field of material m, or 0 where it has none.
def bump_height (pose: pose) (fp: f32) (m: i32) (p: vec3) : f32 =
  if m == m_staff then (bark fp p).0
  else if m == m_robe then cloth pose fp p
  else if m == m_leather then leather pose fp p
  else if m == m_cork then cork fp p
  else if m == m_beard then hair pose fp p
  else 0

def has_bump (m: i32) : bool = m == m_staff || m == m_robe || m == m_leather || m == m_beard || m == m_cork

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
