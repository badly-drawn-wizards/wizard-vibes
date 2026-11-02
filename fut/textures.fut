-- Surface textures: bark on the staff, cloth on the robe, leather on the
-- belt. Each material has a height field (in world units, along the normal)
-- that tilts the shading normal (bump mapping), and colour variation.
--
-- Fine detail aliases once a pixel covers more than a fraction of a
-- wavelength, so every fine term is faded by the pixel's footprint on the
-- surface, fp (distance times the angle one pixel spans).

import "vec3"
import "hash"
import "params"
import "config"
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
#[noinline]
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

def bark_albedo (prm: params) (fp: f32) (p: vec3) : vec3 =
  let (_, fine, broad, tone) = bark fp p
  let q = staff_local p
  -- Bark reads as narrow dark crevices between lighter plates: darken only
  -- near each plate's edge (plate values under ~0.2), at both scales.
  let crevice (v: f32) = smoothstep 0.2 0.0 v
  let wood = wood_colour
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

-- (the patterns below are real GPU functions taking the point in their own
-- space, which the function after each works out from the pose: the bump
-- mapping evaluates them four times a pixel)
#[noinline]
def cloth_pattern (fp: f32) (a: f32) (u: f32) (y: f32) : f32 =
  -- basket weave: in alternate cells the warp (running up) or the weft
  -- (running round) is on top, each a rounded thread
  let weave_w = 0.008
  let cu = f32.floor (u / weave_w)
  let cy = f32.floor (y / weave_w)
  let warp_on_top = (i32.f32 cu + i32.f32 cy) % 2 == 0
  let thread (x: f32) = f32.sin (f32.pi * fract (x / weave_w))
  let weave = if warp_on_top then thread u else thread y
  in 0.012 * fold a y + 0.00025 * weave * keep fp (2 * weave_w)

def cloth (pose: pose) (fp: f32) (p: vec3) : f32 =
  let (a, u, y) = cloth_coords pose p
  in cloth_pattern fp a u y

def cloth_albedo (pose: pose) (p: vec3) : vec3 =
  let (a, _, y) = cloth_coords pose p
  let mottle = 0.9 + 0.2 * vnoise3 (scale 25 p)
  -- the creases of the folds catch less light and look deeper in colour
  in scale (mottle * (0.82 + 0.18 * (0.5 + 0.5 * fold a y))) pose.prm.robe_colour

---------------------------------------------------------------------
-- Leather: a pebbled grain and, on the belt, a row of stitches along each
-- edge, with the edges worn darker.

-- (arc length round the body, height above the belt's centre, on the belt?)
def belt_coords (pose: pose) (p: vec3) : (f32, f32, bool) =
  let b = body_space pose p
  let prm = pose.prm
  let dy = b.y - prm.belt_height
  let r = length2 (vec2 b.x b.z)
  in (f32.atan2 b.x b.z * robe_radius (gear_of prm) prm.belt_height, dy, f32.abs dy < 0.035 && f32.abs (r - robe_radius (gear_of prm) b.y) < 0.03)

-- How much of a stitch is at this point, in [0, 1].
def stitch (u: f32) (dy: f32) : f32 =
  let across = f32.min (f32.abs (dy - 0.017)) (f32.abs (dy + 0.017))
  let phase = fract (u / 0.011)
  let along = if phase < 0.65 then f32.sqrt (f32.sin (f32.pi * phase / 0.65)) else 0
  in along * smoothstep 0.0026 0.0008 across

#[noinline]
def leather_pattern (fp: f32) (u: f32) (dy: f32) (on_belt: bool) (p: vec3) : f32 =
  let g = vnoise3 (scale 85 p)
  let grain = 0.0006 * g * g * keep fp 0.012
  let st = if on_belt then 0.0011 * stitch u dy * keep fp 0.011 else 0
  in grain + st

def leather (pose: pose) (fp: f32) (p: vec3) : f32 =
  let (u, dy, on_belt) = belt_coords pose p
  in leather_pattern fp u dy on_belt p

def leather_albedo (pose: pose) (fp: f32) (p: vec3) : vec3 =
  let (u, dy, on_belt) = belt_coords pose p
  let g = vnoise3 (scale 85 p)
  let base = scale (0.75 + 0.35 * mix1 0.6 g (keep fp 0.012)) leather_colour
  in if !on_belt then base
     else let worn = mix1 1 0.6 (smoothstep 0.018 0.03 (f32.abs dy))
          let s = stitch u dy * keep fp 0.011
          in mix (scale worn base) (vec 0.55 0.45 0.3) s

---------------------------------------------------------------------
-- Hair: fine strands within each beard lock, faded by footprint, and the
-- locks' colours varying from white through cream to light grey.

#[noinline]
def hair_pattern (fp: f32) (b: vec3) : f32 =
  let l = beard_lock b
  -- six strands per lock; a lock is ~0.035 across, so ~0.006 per strand
  let strand = f32.abs (f32.sin (f32.pi * fract (l.phase * 6 + 0.3 * f32.sin (l.h * 20))))
  in 0.0003 * strand * keep fp 0.006

def hair (pose: pose) (fp: f32) (p: vec3) : f32 = hair_pattern fp (beard_space pose p)

def hair_albedo (pose: pose) (p: vec3) : vec3 =
  let l = beard_lock (beard_space pose p)
  let tint = pose.prm.beard_tint *^ mix (vec 0.93 0.92 0.88) (vec 0.78 0.79 0.82) l.rnd
  in scale (0.62 + 0.38 * l.crown) tint

-- Direction the hair runs (down the beard), in the wizard's local space.
def hair_tangent : vec3 = normalise (beard_tip -^ beard_top)

---------------------------------------------------------------------
-- Eyes: sclera, an icy blue iris with a darker rim, and a pupil, laid out
-- by the angle from each eye's gaze. The eyes glance further than the head
-- turns when he points.

def eye_albedo (pose: pose) (p: vec3) : vec3 =
  let q = head_space pose p
  let side = if q.x < 0 then -1 else 1f32
  let c = {x = side * eye_centre.x, y = eye_centre.y, z = eye_centre.z}
  let dir = normalise (q -^ c)
  let gaze = normalise (yaw (0.8 * pose.look) (vec 0 (-0.08) 1))
  let cosa = dot dir gaze
  let iris = smoothstep 0.86 0.875 cosa
  let pupil = smoothstep 0.955 0.965 cosa
  let rim = 1 - 0.5 * smoothstep 0.92 0.88 cosa -- darker towards the iris's edge
  let sclera = vec 0.9 0.88 0.85
  let blue = scale rim iris_colour
  in mix (mix sclera blue iris) (splat 0.02) pupil

---------------------------------------------------------------------
-- Hat stars: five-pointed stars stitched onto the cone, standing slightly
-- proud of it.

-- Signed distance to a five-pointed star with outer radius r and inner
-- radius rf * r, pointing up (after Inigo Quilez).
def sd_star5 (x: f32) (y: f32) (r: f32) (rf: f32) : f32 =
  let (k1x, k1y) = (0.809016994, -0.587785252)
  let (k2x, k2y) = (-k1x, k1y)
  let x = f32.abs x
  let d1 = 2 * f32.max (k1x * x + k1y * y) 0
  let (x, y) = (x - d1 * k1x, y - d1 * k1y)
  let d2 = 2 * f32.max (k2x * x + k2y * y) 0
  let (x, y) = (x - d2 * k2x, y - d2 * k2y)
  let x = f32.abs x
  let y = y - r
  let (bax, bay) = (rf * -k1y, rf * k1x - 1)
  let h = clamp 0 r ((x * bax + y * bay) / (bax * bax + bay * bay))
  let dx = x - bax * h
  let dy = y - bay * h
  in f32.sqrt (dx * dx + dy * dy) * f32.sgn (y * bax - x * bay)

-- (coverage in [0, 1], height above the cloth). The pattern lives in the
-- same bent space as the cone's SDF, so it follows the drooping tip. Rows of
-- square cells run up the cone, each row with as many cells as fit around
-- it, and the star is measured in arc length, so stars keep their shape as
-- the cone narrows. Each star has its own size and tilt; its edge is
-- antialiased by the pixel footprint fp, and its bevel fades out with it.
-- (bp: in the bent hat's space; y0: the band's top)
#[noinline]
def hat_star_pattern (fp: f32) (y0: f32) (bp: vec3) : (f32, f32) =
  let cell = 0.085
  let y0 = y0 + 0.012 -- start above the band
  let v = (bp.y - y0) / cell
  let cv = f32.floor v
  let row_radius = hat_radius (y0 + (cv + 0.5) * cell)
  let cells = f32.max 1 (f32.round (2 * f32.pi * row_radius / cell))
  let u = (f32.atan2 bp.z bp.x / (2 * f32.pi) + 0.5) * cells
  let cu = f32.floor u
  let iu = i32.f32 cu
  let iv = i32.f32 cv
  in if bp.y < y0 || row_radius < 0.035 || hash2 iu iv >= 0.55 then (0, 0) else
  let ox = 0.32 + 0.36 * hash2 (iu + 17) iv
  let oy = 0.32 + 0.36 * hash2 iu (iv + 31)
  -- arc length per u unit at this point, in cell units
  let stretch = 2 * f32.pi * length2 (vec2 bp.x bp.z) / cells / cell
  let tilt = 0.5 * (hash2 (iu + 3) (iv + 5) - 0.5)
  let size = 0.22 + 0.1 * hash2 (iu + 11) (iv + 7)
  let r = rot2 tilt (vec2 ((u - cu - ox) * stretch) (v - cv - oy))
  let d = sd_star5 r.x r.y size 0.45 -- in cell units
  let aa = f32.max 0.02 (fp / cell)
  let cover = smoothstep aa (-aa) d
  let height = 0.0025 * smoothstep 0.01 (-0.06) d * keep fp 0.01
  in (cover, height)

def hat_stars (pose: pose) (fp: f32) (p: vec3) : (f32, f32) =
  hat_star_pattern fp (band_top pose.prm) (bend pose.prm.hat_droop (head_space pose p -^ hat_base))

---------------------------------------------------------------------
-- Cork: tan, with dark speckled pores (small pits in the bump).

#[noinline]
def cork_pores (fp: f32) (p: vec3) : f32 =
  smoothstep 0.7 0.82 (vnoise3 (scale 420 p)) * keep fp 0.004

def cork (fp: f32) (p: vec3) : f32 = -0.0002 * cork_pores fp p

def cork_albedo (fp: f32) (p: vec3) : vec3 =
  let mottle = 0.85 + 0.3 * vnoise3 (scale 120 p)
  in scale (mottle * (1 - 0.45 * cork_pores fp p)) (vec 0.62 0.4 0.2)

---------------------------------------------------------------------
-- The rock's top: flagstones laid in rings round the centre, each a little
-- different in shade, with mossy grooves between them.

-- (distance to the stone's edge, the stone's hash) at a point of the top
def flagstone (p: vec3) : (f32, f32) =
  let ring = 0.32
  let r = length2 (vec2 p.x p.z)
  let ri = f32.floor (r / ring)
  let n = f32.max 2 (f32.floor (2 * f32.pi * (ri + 0.5) * ring / 0.42))
  let a = (f32.atan2 p.z p.x / (2 * f32.pi) + 0.5) * n + 0.7 * hash2 (i32.f32 ri) 5
  let ai = f32.floor a
  let (fr, fa) = (r / ring - ri, a - ai)
  let edge = f32.min (f32.min fr (1 - fr) * ring) (f32.min fa (1 - fa) * 2 * f32.pi * f32.max r 0.05 / n)
  in (edge, hash2 (i32.f32 ri * 7 + 3) (i32.f32 (ai - n * f32.floor (ai / n))))

def flagstone_height (fp: f32) (p: vec3) : f32 =
  let (edge, _) = flagstone p
  in (-0.008 * smoothstep 0.025 0.004 edge + 0.0015 * vnoise3 (scale 25 p)) * keep fp 0.03

---------------------------------------------------------------------
-- The ice and the carpet (see platform.fut).

-- Clear blue ice, deeper and darker towards the point, with pale cracks.
def ice_albedo (base: vec3) (fp: f32) (p: vec3) : vec3 =
  let deep = smoothstep 0 (-2.5) p.y
  let crack = smoothstep 0.035 0 (f32.abs (vnoise3 (scale 2.5 p) - 0.5)) * keep fp 0.08
  in mix (mix base (base *^ vec 0.3 0.45 0.7) deep) (vec 0.8 0.9 1) (0.7 * crack)

-- Its top: frost, patchy and sparkling a little.
def frost_albedo (base: vec3) (fp: f32) (p: vec3) : vec3 =
  let patch = vnoise3 (scale 3 p)
  let sparkle = smoothstep 0.82 0.9 (vnoise3 (scale 40 p)) * keep fp 0.05
  in mix (mix base (vec 0.75 0.85 0.95) (0.4 + 0.5 * patch)) (vec 1 1 1) sparkle

-- A woven rug: a field of `field` with a lattice of darker lines and a
-- dark blue medallion in the middle, inside a border of `trim` with a
-- zigzag, edged with cream lines; the tassels cream. In the carpet's
-- frame (at the platform's origin, x across, z along).
def carpet_albedo (field: vec3) (trim: vec3) (fp: f32) (p: vec3) : vec3 =
  let cream = vec 0.85 0.75 0.55
  let navy = vec 0.04 0.06 0.2
  let (ax, az) = (f32.abs p.x, f32.abs p.z)
  in if az >= 1.6 then cream else
  -- (lines widen to the pixel's footprint, fading as they do)
  let w = f32.max 0.012 fp
  let fade = 0.012 / w
  let line (d: f32) = fade * smoothstep w (0.5 * w) (f32.abs d)
  let e = f32.min (1.15 - ax) (1.6 - az)
  -- the border: along whichever edge is nearer, a zigzag
  let along = if 1.15 - ax < 1.6 - az then p.z else p.x
  let zig = 0.07 + 0.06 * f32.abs (2 * fract (along * 5) - 1)
  let border = smoothstep (0.2 + 0.5 * w) (0.2 - 0.5 * w) e
  let band = mix trim field (smoothstep (zig - 0.5 * w) (zig + 0.5 * w) e * smoothstep (zig + 0.05) (zig + 0.05 - w) e)
  -- the field: a lattice and the medallion
  let lattice = f32.max (line (fract ((p.x + p.z) * 3) - 0.5)) (line (fract ((p.x - p.z) * 3) - 0.5))
  let dm = ax / 0.75 + az / 1.1
  let c = mix field (scale 0.55 field) lattice
  let c = mix c navy (smoothstep (0.55 + 0.5 * w) (0.55 - 0.5 * w) dm)
  let c = mix c trim (smoothstep (0.2 + 0.5 * w) (0.2 - 0.5 * w) dm)
  let c = mix c cream (f32.max (line (dm - 0.55)) (line (dm - 0.6)))
  let c = mix c band border
  let c = mix c cream (f32.max (line (e - 0.03)) (line (e - 0.2)))
  -- the pile
  in scale (0.85 + 0.3 * vnoise3 (scale 30 p) * keep fp 0.04) c

---------------------------------------------------------------------
-- The terrain up close: rough rock, and grass in tufts of blades. The
-- normal of the land itself comes from its height (see surface); this
-- tilts it by detail too fine for that, fading as the footprint grows.

-- (in world coordinates)
def grass_blades (fp: f32) (w: vec3) : f32 =
  -- tufts, and the blades in them: narrow ridges across a warped field
  let tuft = vnoise2 (w.x * 3.1) (w.z * 3.1)
  let warp = 2.5 * vnoise2 (w.x * 9) (w.z * 9)
  let blade = 1 - f32.abs (2 * fract (w.x * 41 + w.z * 13 + warp) - 1)
  let cross = 1 - f32.abs (2 * fract (w.z * 37 - w.x * 11 + 1.7 * warp) - 1)
  in tuft * (blade * blade * keep fp 0.024 + cross * cross * keep fp 0.027)

-- (2D noise, cheap enough for nearly every pixel of the frame, four times
-- over; the grass only where it shows)
def terrain_bump (fp: f32) (w: vec3) : f32 =
  0.03 * vnoise2 (w.x * 4 + 0.4 * w.y) (w.z * 4 - 0.4 * w.y) * keep fp 0.25
  + (if fp < 0.014 then 0.006 * grass_blades fp w else 0)

---------------------------------------------------------------------

-- Height field of material m, or 0 where it has none.
def bump_height (pose: pose) (fp: f32) (m: i32) (p: vec3) : f32 =
  if m == m_staff then (bark fp (staff_rest pose p)).0
  else if m == m_robe then cloth pose fp p
  else if m == m_leather then leather pose fp p
  else if m == m_cork then cork fp p
  else if m == m_hat then (hat_stars pose fp p).1
  else if m == m_beard then hair pose fp p
  else if m == m_ground then (if pose.prm.platform == platform_rock then flagstone_height fp p else 0)
  else if m == m_terrain then terrain_bump fp (to_world pose.fl p)
  else 0

def has_bump (m: i32) : bool =
  m == m_staff || m == m_robe || m == m_leather || m == m_beard || m == m_cork || m == m_hat || m == m_ground || m == m_terrain

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
