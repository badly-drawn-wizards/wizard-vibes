-- The wizard scene: geometry, material ids, and the glowing media (orb cloud
-- and rune letters) that the march integrates.

import "vec3"
import "params"
import "config"
import "sdf"
import "hash"
import "anim"
import "world"
import "march"

-- Left and right are the wizard's own: he faces +z, so his left is +x.

-- Material ids.
def m_robe : i32 = 1
def m_skin : i32 = 2
def m_beard : i32 = 3
def m_hat : i32 = 4
def m_gold : i32 = 5
def m_eye : i32 = 6
def m_wood : i32 = 7
def m_ground : i32 = 9
def m_leather : i32 = 10
def m_tome : i32 = 11
def m_pages : i32 = 12
def m_potion : i32 = 20 -- 20, 21, 22: one per flask
def m_rock : i32 = 13
def m_terrain : i32 = 14
def m_staff : i32 = 15
def m_cork : i32 = 16
def m_brow : i32 = 17
def m_band : i32 = 18
def m_wall : i32 = 24 -- 24 .. 28: the structure parts (part_* in world.fut)
def m_thatch : i32 = 25
def m_tower : i32 = 26
def m_slate : i32 = 27
def m_sandstone : i32 = 28
def m_water : i32 = 29

---------------------------------------------------------------------
-- Wizard

-- The staff stands just outside the robe's hem and leans forward; the left
-- hand grips it at grip_height and the orb sits on its tip.
def staff_foot : vec3 = vec 0.6 0 0.25
def staff_top : vec3 = vec 0.52 2.0 0.46
def staff_dir : vec3 = normalise (staff_top -^ staff_foot)
def staff_radius : f32 = 0.028
def staff_at (y: f32) : vec3 = staff_foot +^ scale (y / staff_dir.y) staff_dir

-- The orb on its tip.
def orb_radius : f32 = 0.11
def orb_pos : vec3 = staff_top +^ scale 0.1 staff_dir

def grip_height : f32 = 1.15
-- The elbow's rest position; only used to place the fist and the wrist.
def left_elbow_rest : vec3 = vec 0.36 1.07 0.14
-- The left palm's centre, on the elbow's side of the staff, and the cuff
-- of the sleeve, set back from it so the hand sticks out (the arm's IK
-- reaches for the cuff).
def left_palm : vec3 =
  let g = staff_at grip_height
  let towards = left_elbow_rest -^ g
  in g +^ scale 0.045 (normalise (towards -^ scale (dot towards staff_dir) staff_dir))
def left_wrist : vec3 = left_palm -^ scale 0.1 (normalise (left_palm -^ left_elbow_rest))

-- Two-bone IK: for a shoulder s reaching for target t with bone lengths l1
-- and l2, returns (elbow, hand). The elbow bends towards `pole`; an
-- unreachable target is approached as far as the arm allows.
def two_bone (s: vec3) (t: vec3) (l1: f32) (l2: f32) (pole: vec3) : (vec3, vec3) =
  let d = t -^ s
  let dir = normalise d
  let reach = clamp (f32.abs (l1 - l2) + 1e-3) (l1 + l2 - 1e-3) (length d)
  let a = (l1 * l1 - l2 * l2 + reach * reach) / (2 * reach) -- along dir to the elbow
  let h = f32.sqrt (f32.max 0 (l1 * l1 - a * a)) -- out from dir to the elbow
  let side = normalise (pole -^ scale (dot pole dir) dir)
  in (s +^ scale a dir +^ scale h side, s +^ scale reach dir)

-- Breathing: three breaths per loop (4 s each); 0 exhaled, 1 inhaled.
def breath (tm: f32) : f32 = 0.5 - 0.5 * f32.cos (3 * tm)

-- Everything about the wizard that moves, worked out once per pixel. tm is
-- the loop angle (for periodic motion), secs the time in seconds.
type pose = { tm: f32, secs: f32, fl: flight
            , moon: vec3, moon_world: vec3 -- moon direction, local and in the world
            , prm: params -- the drawer's uniforms
            , breath: f32
            , lean: f32 -- sideways shear of the body above the hips, per unit height
            , head: vec3 -- offset of the head, beard and hat
            , look: f32 -- head turn (radians, towards +x)
            , point: f32 -- 0 idle .. 1 pointing with the right hand
            , pix: f32 -- angle one pixel spans (for texture filtering)
            , blink: f32 -- 0 eyes open .. 1 closed
            , runes: f32 -- the runes' brightness, relative to their glow parameters
            -- The staff, held out to cast: its rest frame (where the staff,
            -- orb, cradle and the hand holding it are modelled) turns by
            -- (staff_cos, staff_sin) about staff_axis through staff_pivot,
            -- then shifts by staff_shift (see staff_world and staff_rest).
            , staff_pivot: vec3, staff_axis: vec3, staff_cos: f32, staff_sin: f32, staff_shift: vec3
            , orb: vec3 -- the orb's centre, where the staff holds it
            , orb_tint: vec3 -- the orb's glow colour, by spell
            , bolt_tint: vec3 -- tints its arcs and its light, by spell
            , orb_boost: f32 -- its glow, flaring as a spell is cast
            , spell: i32 -- the spell in hand: what goes on in the orb (0 fire, 1 lightning, 2 wind)
            , l_shoulder: vec3, l_elbow: vec3, l_wrist: vec3
            -- the left hand's grip on the staff, the staff's axis there, and
            -- the palm (on the side facing the elbow)
            , l_grip: vec3, l_axis: vec3, l_palm: vec3
            , r_shoulder: vec3, r_elbow: vec3, r_wrist: vec3 }

-- Free flight, driven from the harness (web/flight.ts): with `free` set, the
-- wizard is where it says instead of on the flight path. (ax, ay, az) is the
-- way he accelerates in his own frame, as long as how hard (0..1): he points
-- that way, the more the harder. climb is his (smoothed) vertical speed: the
-- runes brighten as he rises and dim as he sinks.
--
-- Casting (on the path too): staff (0..1) is how far he holds the staff out
-- towards (sx, sy, sz), the way he casts in his own frame; flash is the time
-- since the last cast (for the thrust and the orb's flare); spell is the
-- spell in hand (0 fire, 1 lightning, 2 wind), which colours the orb.
type drive = {free: bool, x: f32, y: f32, z: f32, heading: f32, turn: f32, ax: f32, ay: f32, az: f32, climb: f32,
              staff: f32, sx: f32, sy: f32, sz: f32, flash: f32, spell: f32}

def on_path : drive = {free = false, x = 0, y = 0, z = 0, heading = 0, turn = 0, ax = 0, ay = 0, az = 0, climb = 0,
                       staff = 0, sx = 0, sy = 0, sz = 1, flash = 10, spell = 1}

def flight_of (prm: params) (dr: drive) (secs: f32) : flight =
  if !dr.free then flight_at prm.bank secs
  else {pos = vec dr.x dr.y dr.z, heading = dr.heading, turn = dr.turn, bank = clamp (-0.35) 0.35 (prm.bank * dr.turn)}

-- Rotation of v by angle (cos c, sin s) about the unit axis k.
def rotate_about (k: vec3) (c: f32) (s: f32) (v: vec3) : vec3 =
  scale c v +^ scale s (cross k v) +^ scale ((1 - c) * dot k v) k

-- The staff's rest frame to his, and back.
def staff_world (pose: pose) (q: vec3) : vec3 =
  pose.staff_pivot +^ rotate_about pose.staff_axis pose.staff_cos pose.staff_sin (q -^ pose.staff_pivot) +^ pose.staff_shift
def staff_rest (pose: pose) (p: vec3) : vec3 =
  pose.staff_pivot +^ rotate_about pose.staff_axis pose.staff_cos (-pose.staff_sin) (p -^ pose.staff_shift -^ pose.staff_pivot)

def pose_at (secs: f32) (prm: params) (dr: drive) : pose =
  let tm = loop_angle secs
  let fl = flight_of prm dr secs
  let b = breath tm
  -- Turning towards local +x makes fl.turn positive. He leans into the turn,
  -- and in sharper turns points the way with his right hand and looks there.
  -- (in free flight he points the way he accelerates instead)
  let accel = vec dr.ax dr.ay dr.az
  let side = if dr.free then f32.sgn dr.ax else f32.sgn fl.turn
  let point = if dr.free then saturate (length accel) else smoothstep 0.03 0.12 (f32.abs fl.turn)
  let lean = clamp (-0.15) 0.15 (prm.lean * fl.turn)
  let shift (y: f32) = vec (lean * f32.max 0 (y - 0.45)) 0 0
  -- inhaling lifts the shoulders; the head rises and nods forward a little
  let bd = b * prm.breath
  let lift = vec 0 (0.012 * bd) (0.004 * bd) +^ shift 1.33
  let head = vec 0 (0.012 * bd) (0.007 * bd) +^ shift 1.6
  let l_shoulder = vec 0.2 1.33 0 +^ lift
  let r_shoulder = vec (-0.2) 1.33 0 +^ lift
  -- Casting: the staff turns about his grip to lead with the orb (raised a
  -- little above the aim) and is pushed forward, with a thrust on each cast.
  let aim = normalise (vec dr.sx dr.sy dr.sz +^ vec 0 0 1e-4)
  let want = normalise (aim +^ vec 0 0.9 0)
  let k = cross staff_dir want
  let staff_axis = if length k > 1e-4 then normalise k else vec 1 0 0
  let angle = 0.85 * dr.staff * f32.acos (clamp (-1) 1 (dot staff_dir want))
  let kick = dr.staff * f32.exp (-12 * dr.flash)
  let staff_pivot = staff_at grip_height
  let staff_shift = scale (0.16 * dr.staff + 0.07 * kick) (normalise (vec aim.x 0 aim.z +^ vec 0 0 1e-4))
                    +^ vec 0 (0.08 * dr.staff) 0
  let (staff_cos, staff_sin) = (f32.cos angle, f32.sin angle)
  let held (q: vec3) = staff_pivot +^ rotate_about staff_axis staff_cos staff_sin (q -^ staff_pivot) +^ staff_shift
  -- The left hand stays on the staff, closed round it with the palm facing
  -- the elbow, wherever the staff and the elbow are: the arm reaches for the
  -- staff, then the hand is built round the staff's grip facing the elbow
  -- that gives, and the arm reaches again for its wrist.
  let l_grip = held (staff_at grip_height)
  let l_axis = rotate_about staff_axis staff_cos staff_sin staff_dir
  let hand_at (elbow: vec3) =
    let towards = elbow -^ l_grip
    let palm = l_grip +^ scale 0.045 (normalise (towards -^ scale (dot towards l_axis) l_axis))
    in (palm, palm -^ scale 0.1 (normalise (palm -^ elbow)))
  let pole = vec 0.5 (-1) (-0.5)
  let (e0, _) = two_bone l_shoulder (held left_wrist) 0.34 0.29 pole
  let (l_elbow, l_wrist) = two_bone l_shoulder (hand_at e0).1 0.34 0.29 pole
  let l_palm = (hand_at l_elbow).0
  -- the orb takes the colour of the spell in hand, and flares as it is cast
  let spell = i32.f32 (f32.round dr.spell)
  let orb_tint = if spell == 0 then vec 1 0.42 0.12 else if spell == 2 then vec 0.5 1 0.75 else prm.orb_glow_colour
  let bolt_tint = if spell == 0 then vec 1.5 0.65 0.25 else if spell == 2 then vec 0.75 1.25 0.95 else splat 1
  let orb_boost = 1 + 2 * f32.exp (-6 * dr.flash)
  -- the right hand drifts idly in front of the belly, or points into the turn
  let idle = vec (0.025 * f32.sin tm) (0.018 * f32.sin (2 * tm) + 0.01 * b) (0.02 * f32.cos tm)
  let rest = vec (-0.25) 1.02 0.27 +^ idle +^ shift 1.02
  -- (pointing to his left means reaching across the body, so aim higher and more
  -- forward to keep the arm clear of the beard)
  let aim_dir =
    if dr.free then
      -- never behind him (he can't reach), and reaching across, higher and
      -- further forward, like the turns
      let a = normalise (accel +^ vec 0 0 1e-4)
      let across = f32.max 0 a.x
      in vec a.x (a.y + 0.3 + 0.25 * across) (f32.max 0.35 a.z + 0.4 * across)
    else if side < 0 then vec (-0.85) 0.3 0.75 else vec 0.5 0.55 0.9
  let aim = r_shoulder +^ scale 0.62 (normalise aim_dir)
  let (r_elbow, r_wrist) = two_bone r_shoulder (mix rest aim point) 0.31 0.27 (vec (-0.6) (-1) (-0.5))
  let moon_world = moon_dir prm.moon_azimuth prm.moon_elevation
  in { tm, secs, fl, prm, moon = to_local_dir fl moon_world, moon_world
     , breath = b, lean, head, look = 0.45 * point * side * (if dr.free then f32.min 1 (2 * f32.abs dr.ax) else 1)
     , point, pix = 0.0008
     -- doubling for every 4 units/s of climb
     , runes = if dr.free then clamp 0.1 3 (2 ** (dr.climb / 4)) else 1
     -- three quick blinks per loop (every 4 s), each about 0.2 s long
     , blink = let d = (fract (3 * tm / (2 * f32.pi) + 0.15) - 0.5) * 28 in f32.exp (-(d * d))
     , l_shoulder, l_elbow, l_wrist, l_grip, l_axis, l_palm, r_shoulder, r_elbow, r_wrist
     , staff_pivot, staff_axis, staff_cos, staff_sin, staff_shift, orb = held orb_pos, orb_tint, bolt_tint, orb_boost, spell }


def sd_orb (p: vec3) : f32 = sd_sphere (p -^ orb_pos) orb_radius

-- The gold cradle holding the orb: a collar on the staff tip and prongs that
-- curve up around the glass with a helical twist. Angles are polar angles
-- from the orb's bottom, along the staff's axis.
def prongs : f32 = 5
def prong_start : f32 = 0.45
def prong_end : f32 = 1.37 -- 78 degrees: 40% of the way up the orb
def prong_twist : f32 = 1.2 -- radians around the orb per radian climbed
def prong_radius : f32 = 0.013

def sd_cradle_exact (p: vec3) : f32 =
  -- a frame around the staff's axis
  let ax = staff_dir
  let e1 = normalise (cross ax (vec 0 0 1))
  let e2 = cross ax e1
  let v = p -^ orb_pos
  let rho = length v
  let theta = f32.acos (clamp (-1) 1 (-(dot v ax) / f32.max rho 1e-6))
  let phi = f32.atan2 (dot v e2) (dot v e1)
  -- prongs: tubes on a sphere just outside the glass, folded into one sector
  let r = orb_radius + 0.6 * prong_radius
  let th = clamp prong_start prong_end theta
  let f = (th - prong_start) / (prong_end - prong_start)
  let sector = 2 * f32.pi / prongs
  let dphi = (fract ((phi - prong_twist * th) / sector + 0.5) - 0.5) * sector
  let radial = rho - r
  let across = r * f32.sin th * dphi
  let along = r * (theta - th)
  -- the twist stretches space by at most this much, so divide it out
  let stretch = f32.sqrt (1 + (prong_twist * f32.sin th) ** 2)
  let prong = (f32.sqrt (radial * radial + across * across + along * along) - prong_radius * (1 - 0.4 * f)) / stretch
  let tip = (f32.sqrt (radial * radial + across * across + (r * (theta - prong_end)) ** 2) - 0.014) / stretch
  -- collar: a ring where the prongs start, on a cup flaring from the staff
  let ring_centre = orb_pos -^ scale (r * f32.cos prong_start) ax
  let ring_r = r * f32.sin prong_start
  let w = p -^ ring_centre
  let h = dot w ax
  let ring = length2 (vec2 (length (w -^ scale h ax) - ring_r) h) - 0.014
  let cup = sd_round_cone_ab p (staff_top -^ scale 0.05 ax) ring_centre staff_radius (0.85 * ring_r)
  in f32.min (smin 0.02 ring cup) (smin 0.01 prong tip)

-- (the whole cradle fits in a sphere of radius 0.15 round this point)
def sd_cradle (p: vec3) : f32 =
  sphere_bounded (orb_pos -^ scale 0.04 staff_dir) 0.15 0.15 sd_cradle_exact p

def sd_robe (pose: pose) (p: vec3) : f32 =
  let a = f32.atan2 p.z p.x
  let hem = 0.02 * f32.sin (9 * a + 2 * pose.tm) * saturate (1 - p.y / 0.8)
  let cone = sd_capped_cone p 1.35 pose.prm.robe_hem 0.17 - 0.03 + hem
  -- the chest fills out with each breath
  let b = pose.breath
  let shoulders = sd_ellipsoid (p -^ vec 0 (1.3 + 0.01 * b) (0.004 * b)) (vec (0.29 + 0.008 * b) 0.14 (0.2 + 0.01 * b))
  in smin 0.12 cone shoulders

-- An arm: upper arm and a forearm whose sleeve flares towards the cuff,
-- left open: a narrower cone is cut out of its end, so the cuff is a tube
-- the hand comes out of.
def sd_arm (shoulder: vec3) (elbow: vec3) (wrist: vec3) (p: vec3) : f32 =
  let d = normalise (wrist -^ elbow)
  let fore = sd_round_cone_ab p elbow wrist 0.064 0.078
  -- (the opening lies within 0.13 of the cuff; beyond 0.2 it cannot affect
  -- the sleeve, so it is only evaluated near the cuff)
  let sleeve = if length (p -^ wrist) > 0.2 then fore
               else smax 0.008 fore (-(sd_round_cone_ab p (wrist -^ scale 0.07 d) (wrist +^ scale 0.06 d) 0.05 0.062))
  in smin 0.04 (sd_capsule p shoulder elbow 0.075) sleeve

-- Where the right hand's palm is: out of the sleeve, along the forearm.
def right_palm (pose: pose) : vec3 =
  pose.r_wrist +^ scale 0.085 (normalise (pose.r_wrist -^ pose.r_elbow))

def sd_arms (pose: pose) (p: vec3) : f32 =
  f32.min (sd_arm pose.l_shoulder pose.l_elbow pose.l_wrist p)
          (sd_arm pose.r_shoulder pose.r_elbow pose.r_wrist p)

-- Cartoon hands: a thumb and three fingers.

-- A wrist: skin from inside the sleeve's cuff out to the palm.
def sd_wrist (cuff: vec3) (palm: vec3) (p: vec3) : f32 =
  let d = normalise (palm -^ cuff)
  in sd_capsule p (cuff -^ scale 0.03 d) (palm -^ scale 0.02 d) 0.03

-- The left hand, closed round the staff. A palm on
-- the elbow's side, and digits made of short straight segments with
-- knuckles, like the pointing hand's: three fingers wrapping round the
-- staff below the thumb, which wraps the other way above them.
-- (built round the staff wherever it is held, see pose_at)
def sd_left_hand (pose: pose) (p: vec3) : f32 =
  let g = pose.l_grip
  let staff_dir = pose.l_axis
  let u = normalise (pose.l_palm -^ g) -- to the palm
  let v = cross u staff_dir -- the way the fingers wrap
  let q = p -^ g
  let palm = sd_ellipsoid (vec (dot q u - 0.045) (dot q v) (dot q staff_dir)) (vec 0.026 0.036 0.05)
  -- a digit at height y along the staff: segments between points on a
  -- circle of radius 0.048 round it, at the given angles from the palm
  let around (y: f32) (phi: f32) =
    g +^ scale y staff_dir +^ scale (0.048 * f32.cos phi) u +^ scale (0.048 * f32.sin phi) v
  -- (angles are scalars, not an array: the scene is a non-inlined GPU
  -- function, which the WebGPU backend cannot generate with arrays)
  let digit (y0: f32) (y1: f32) (a0: f32) (a1: f32) (a2: f32) (a3: f32) (r: f32) =
    let pt (k: f32) (a: f32) = around (mix1 y0 y1 (k / 3)) a
    let p0 = pt 0 a0
    let p1 = pt 1 a1
    let p2 = pt 2 a2
    let p3 = pt 3 a3
    in f32.min (sd_capsule p p0 p1 r) (f32.min (sd_capsule p p1 p2 (r - 0.001)) (sd_capsule p p2 p3 (r - 0.002)))
  let fingers = digit (-0.03) (-0.03) 0.2 1.3 2.4 3.35 0.0155
                |> f32.min (digit 0 0 0.2 1.35 2.5 3.45 0.016)
                |> f32.min (digit 0.03 0.03 0.2 1.3 2.4 3.3 0.0155)
  let thumb = digit 0.05 0.062 0.4 (-0.5) (-1.35) (-1.9) 0.016
  in smin 0.012 (smin 0.02 palm (sd_wrist pose.l_wrist pose.l_palm p)) (f32.min fingers thumb)

-- The right hand, which points: a palm in a frame along
-- the forearm, three fingers of two straight segments side by side, and a
-- thumb. At rest the fingers are nearly straight with a slight bend; as he
-- points into a turn the index finger straightens and the others close.
def sd_right_hand (pose: pose) (p: vec3) : f32 =
  let f = normalise (pose.r_wrist -^ pose.r_elbow) -- along the forearm
  let side = normalise (cross f (vec 0 1 0)) -- away from the thumb
  let up = cross side f -- the back of the hand
  let centre = right_palm pose
  let q = p -^ centre
  let lq = vec (dot q side) (dot q up) (dot q f)
  let palm = sd_ellipsoid lq (vec 0.036 0.026 0.045)
  -- a finger across the palm, two segments of lengths l1 and l2, bent by
  -- c1 and c1 + c2 radians towards the palm
  let finger (across: f32) (c1: f32) (c2: f32) (l1: f32) (l2: f32) =
    let dir (c: f32) = scale (f32.cos c) f -^ scale (f32.sin c) up
    let base = centre +^ scale 0.035 f +^ scale across side
    let knuckle = base +^ scale l1 (dir c1)
    let tip = knuckle +^ scale l2 (dir (c1 + c2))
    in f32.min (sd_capsule p base knuckle 0.015) (sd_capsule p knuckle tip 0.0145)
  let pt = pose.point
  let index = finger (-0.025) (mix1 0.15 0.0 pt) (mix1 0.2 0.0 pt) 0.032 0.028
  let middle = finger 0 (mix1 0.2 1.1 pt) (mix1 0.25 1.3 pt) 0.034 0.028
  let ring = finger 0.025 (mix1 0.25 1.1 pt) (mix1 0.3 1.3 pt) 0.03 0.025
  -- the thumb: from the inner side of the palm, along the index finger
  let tb = centre -^ scale 0.03 side +^ scale 0.005 f
  let tdir = normalise (scale 0.85 f -^ scale 0.4 side -^ scale (0.2 + 0.25 * pt) up)
  let thumb = sd_capsule p tb (tb +^ scale 0.042 tdir) 0.015
  let wrist = sd_wrist pose.r_wrist centre p
  in smin 0.012 (smin 0.02 palm wrist) (f32.min (f32.min index middle) (f32.min ring thumb))

-- (each hand, with its wrist, fits in a sphere of radius 0.14)
def sd_hands (pose: pose) (p: vec3) : f32 =
  f32.min (sphere_bounded pose.l_palm 0.14 0.15 (sd_left_hand pose) p)
          (sphere_bounded (right_palm pose) 0.14 0.15 (sd_right_hand pose) p)

-- The head, beard and hat move together: offset by the pose, and turned
-- about the neck to look where he points.
def neck : vec3 = vec 0 1.45 0

def head_space (pose: pose) (p: vec3) : vec3 =
  neck +^ yaw (-pose.look) (p -^ pose.head -^ neck)

-- The body above the hips leans sideways into turns (a shear; the hem stays
-- put). The lean is at most 0.15, so distances stretch by under 2%.
def body_space (pose: pose) (p: vec3) : vec3 =
  p -^ vec (pose.lean * f32.max 0 (p.y - 0.45)) 0 0

def sd_head (pose: pose) (p: vec3) : f32 =
  let p = head_space pose p
  let head = sd_sphere (p -^ vec 0 1.6 0.02) 0.17
  let nose = sd_ellipsoid (p -^ vec 0 1.58 0.19) (vec 0.035 0.05 0.05)
  in smin 0.03 head nose

-- The eyes: eyeballs set into the face (their iris and pupil are shading,
-- see eye_albedo), with upper lids that close to blink.
-- (the head's surface is at z ~ 0.173 here, so the eye stands ~0.011 proud)
def eye_centre : vec3 = vec 0.062 1.632 0.159 -- the left eye's (+x); mirrored
def eye_radius : f32 = 0.025

-- head space, mirrored onto the left side
def mirror_x (p: vec3) : vec3 = {x = f32.abs p.x, y = p.y, z = p.z}

def sd_eyes (pose: pose) (p: vec3) : f32 =
  sd_sphere (mirror_x (head_space pose p) -^ eye_centre) eye_radius

-- The upper lids: skin over the top of each eyeball, down to a lid line
-- that drops to below the eye's equator to blink.
def sd_eyelids (pose: pose) (p: vec3) : f32 =
  let q = mirror_x (head_space pose p) -^ eye_centre
  let line = mix1 (0.6 * eye_radius) (-1.05 * eye_radius) pose.blink
  in f32.max (sd_sphere q (eye_radius + 0.003)) (line - q.y)

-- The eyebrows: bushy arcs above the eyes, from inner to outer end. When he
-- points they knit into a determined look: the inner ends drop and draw
-- together, the outer ends lift.
def sd_brows (pose: pose) (p: vec3) : f32 =
  let q = mirror_x (head_space pose p)
  let k = pose.point
  let inner = vec (0.03 - 0.008 * k) (1.664 - 0.012 * k) 0.174
  let mid = vec 0.064 (1.676 + 0.002 * k) 0.172
  let outer = vec 0.1 (1.668 + 0.008 * k) 0.145
  in smin 0.006 (sd_round_cone_ab q inner mid 0.012 0.01) (sd_round_cone_ab q mid outer 0.01 0.006)

-- The beard hangs from the cheeks to a forked tip resting on the chest.
def beard_top : vec3 = vec 0 1.52 0.11
def beard_tip : vec3 = vec 0 0.98 0.27

-- Beard space: swaying with the loop, and squashed sideways so the beard is
-- wider than deep (dividing x only contracts space, so distances stay safe).
def beard_space (pose: pose) (p: vec3) : vec3 =
  let p = head_space pose p
  let tm = pose.tm
  let sway = 0.03 * f32.sin (2 * tm) * saturate ((1.5 - p.y) / 0.5)
  in {x = (p.x - sway) / 1.45, y = p.y, z = p.z}

-- The beard's locks, at a point in beard space. Around the beard's axis
-- the angle is divided into about 18 locks; noise warps the divisions so
-- locks vary in width and wander as they hang. Shared by the shape and the
-- material, so shading follows the locks.
type lock = { h: f32 -- along the beard, 0 at the cheeks .. 1 at the tip
            , phase: f32 -- across the locks: lock k spans [k, k + 1)
            , crown: f32 -- 0 in the groove between locks .. 1 on a lock's crown
            , rnd: f32 } -- per-lock random value

def beard_lock (q: vec3) : lock =
  let ba = beard_tip -^ beard_top
  let h = saturate (dot (q -^ beard_top) ba / dot ba ba)
  let rv = q -^ (beard_top +^ scale h ba)
  let phi = f32.atan2 rv.x rv.z
  let warp = 1.6 * vnoise2 (h * 3) (phi * 1.5 + 7) + 0.4 * f32.sin (h * 9 + phi * 2)
  let phase = phi * 18 / (2 * f32.pi) + warp
  let id = i32.f32 (f32.floor phase)
  in {h, phase, crown = f32.sin (f32.pi * fract phase) ** 0.7, rnd = hash2 id 51}

def sd_beard (pose: pose) (p: vec3) : f32 =
  let q = beard_space pose p
  let p = head_space pose p
  -- fork the lower half into two points
  let fork = 0.035 * smoothstep 1.3 0.98 q.y
  let fq = {x = f32.abs q.x - fork, y = q.y, z = q.z}
  let body = sd_round_cone_ab fq beard_top beard_tip 0.13 0.018
  -- grooves between the locks, deeper for some locks than others, and each
  -- lock ending at its own length, so the tip is ragged rather than cut
  let l = beard_lock q
  let groove = 0.008 * (1 - l.h * 0.5) * (1 - l.crown) * (0.6 + 0.8 * l.rnd)
  let len = length (beard_tip -^ beard_top)
  let cut = (l.h - (0.86 + 0.14 * l.rnd)) * len
  let body = smax 0.02 (body + groove) cut
  let jaw = sd_ellipsoid (p -^ vec 0 1.53 0.05) (vec 0.155 0.1 0.13)
  let moustache =
    let m = {x = f32.abs p.x, y = p.y, z = p.z}
    in sd_round_cone_ab m (vec 0.01 1.548 0.178) (vec 0.105 1.49 0.13) 0.024 0.01
  -- (the warped locks stretch the field a little; scale to stay safe)
  in 0.85 * smin 0.03 (smin 0.06 body jaw) moustache

def hat_base : vec3 = vec 0 1.74 0
def hat_height : f32 = 0.72
def hat_radius (y: f32) : f32 = 0.2 - (0.2 - 0.012) * saturate (y / hat_height)

def sd_hat (pose: pose) (p: vec3) : f32 =
  let hp = head_space pose p -^ hat_base
  let bp = bend pose.prm.hat_droop hp
  let cone = sd_capped_cone bp hat_height (hat_radius 0) (hat_radius hat_height)
  -- a flat disc with a slim rolled (welt) edge
  let brim = f32.min (sd_torus hp 0.25 0.012) (sd_cylinder hp 0.255 0.01)
  in smin 0.04 cone brim

-- A flat gold band round the hat just above the brim: a short frustum
-- following the cone's slope, standing a few millimetres proud of it, with
-- softened edges. (The hat's bend is negligible this low.)
def band_bottom : f32 = 0.018 -- above the hat's base
def band_top (prm: params) : f32 = band_bottom + prm.band_height

def sd_band (pose: pose) (p: vec3) : f32 =
  let y0 = band_bottom
  let y1 = band_top pose.prm
  let proud = 0.006
  let q = head_space pose p -^ (hat_base +^ vec 0 y0 0)
  in sd_capped_cone q (y1 - y0) (hat_radius y0 + proud - 0.002) (hat_radius y1 + proud - 0.002) - 0.002

-- A frame around the staff: (along the axis from the foot, and two unit
-- vectors across it).
def staff_frame : (vec3, vec3) =
  let e1 = normalise (cross staff_dir (vec 0 0 1))
  in (e1, cross staff_dir e1)

-- Coordinates in the staff's frame: x, z across the axis, y along it.
def staff_local (p: vec3) : vec3 =
  let (e1, e2) = staff_frame
  let v = p -^ staff_foot
  in vec (dot v e1) (dot v staff_dir) (dot v e2)

def staff_length : f32 = length (staff_top -^ staff_foot)

-- Knots: swellings at a few heights, each on one side of the branch.

-- A branch rather than a pole: the radius swells and narrows along the
-- length, thickening a little towards the foot, with a few knots. (The
-- field is stretched by under 2%, so it stays safe to trace.)
def sd_staff (p: vec3) : f32 =
  let q = staff_local p
  let y = clamp 0 staff_length q.y
  let r = staff_radius * (1.12 - 0.12 * y / staff_length + 0.07 * f32.sin (y * 6.3 + 1) + 0.04 * f32.sin (y * 17.1))
  let shaft = length (q -^ vec 0 y 0) - r
  let knot (ky: f32) (ka: f32) =
    let c = vec (staff_radius * 0.75 * f32.cos ka) ky (staff_radius * 0.75 * f32.sin ka)
    in sd_ellipsoid (q -^ c) (vec 0.022 0.035 0.022)
  let knots = f32.min (knot 0.55 0.6) (f32.min (knot 1.32 2.6) (knot 1.72 4.4))
  in smin 0.015 shaft knots

---------------------------------------------------------------------
-- Belt, buckle, potions and tome

-- Radius of the robe's cone at height y (before the hem ripple).
-- The robe's surface radius at height y (the -0.03 in sd_robe rounds it
-- outward), and the belt's height: everything at the waist is placed from
-- these, so it follows the drawer's robe_hem and belt_height.
def robe_radius (prm: params) (y: f32) : f32 = prm.robe_hem - (prm.robe_hem - 0.17) * y / 1.35 + 0.03

-- Outward horizontal direction at an angle round the body (0 = front, +x at 90 degrees).
def around (a: f32) : vec3 = vec (f32.sin a) 0 (f32.cos a)
def deg (d: f32) : f32 = d * f32.pi / 180

def sd_belt (prm: params) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let q = vec2 (r - (robe_radius prm p.y + 0.008)) (p.y - prm.belt_height)
  -- a rounded rectangle in the (radius, height) plane, swept round the body
  let d = vec2 (f32.abs q.x - 0.008) (f32.abs q.y - 0.024)
  in length2 (vec2 (f32.max d.x 0) (f32.max d.y 0)) + f32.min (f32.max d.x d.y) 0 - 0.006

-- A gold pentagram in a ring, on the front of the belt. Every stroke is a
-- round tube, so it reads as cast metal.
def buckle_centre (prm: params) : vec3 = vec 0 prm.belt_height (robe_radius prm prm.belt_height + 0.03)

def sd_buckle (prm: params) (p: vec3) : f32 =
  let q = p -^ buckle_centre prm
  let rad = 0.05
  let corner (k: i32) =
    let a = f32.pi / 2 + 2 * f32.pi * f32.i32 k / 5
    in vec (rad * f32.cos a) (rad * f32.sin a) 0
  let star = loop d = 1e9f32 for k < 5 do
               f32.min d (sd_capsule q (corner k) (corner ((k + 2) % 5)) 0.006)
  let ring = length2 (vec2 (length2 (vec2 q.x q.y) - (rad + 0.008)) q.z) - 0.007
  in f32.min star ring

-- A flat strap (0.022 wide, 0.005 thick) from a to b, its face along n.
def sd_strap (p: vec3) (a: vec3) (b: vec3) (n: vec3) : f32 =
  let ab = b -^ a
  let len = length ab
  let d = scale (1 / len) ab
  let w = normalise (cross d n)
  let nn = cross w d
  let v = p -^ mix a b 0.5
  in sd_box (vec (dot v d) (dot v w) (dot v nn)) (vec (0.5 * len + 0.002) 0.01 0.0015) - 0.001

-- Three potions hanging from the belt on the left hip: two round flasks
-- either side of a tall bottle, each with a short neck and a cork. The glass
-- is not part of the marched scene (rays pass through it; see potion_glass
-- and glass in shading.fut); the liquid inside is, and glows.
def potion_angle (i: i32) : f32 = deg (70 + 24 * f32.i32 i)

-- Each potion hangs below the belt from a strap round its neck.
def potion_neck_top (prm: params) (i: i32) : vec3 =
  let y = prm.belt_height - 0.06
  in scale (robe_radius prm y + 0.022) (around (potion_angle i)) +^ vec 0 y 0

def potion_neck_radius (i: i32) : f32 = if i == 1 then 0.012 else 0.013

-- The centre of potion i's body, in body space.
def potion_centre (prm: params) (i: i32) : vec3 =
  let body_top = potion_neck_top prm i -^ vec 0 0.03 0
  in if i == 1 then body_top -^ vec 0 0.055 0 else body_top -^ vec 0 0.032 0

-- (outer glass surface, cork, liquid surface height) of potion i
def potion_parts (prm: params) (i: i32) (p: vec3) : (f32, f32, f32) =
  let neck_top = potion_neck_top prm i
  let body_top = neck_top -^ vec 0 0.03 0
  let c = potion_centre prm i
  let (glass, level) =
    if i == 1 then
      -- tall bottle: a rounded cylinder under the neck, four-fifths full
      ( smin 0.01 (sd_capsule p (c -^ vec 0 0.03 0) (c +^ vec 0 0.025 0) 0.03)
                     (sd_capsule p body_top neck_top 0.012)
         , c.y + 0.03 )
    else
      -- round flask, two-thirds full
      let r = 0.04
      in (smin 0.015 (sd_sphere (p -^ c) r) (sd_capsule p body_top neck_top 0.013), c.y + 0.25 * r)
  -- an open mouth (the neck's capsule would otherwise dome over the cork)
  -- with a rolled lip round it
  let nr = potion_neck_radius i
  let glass = f32.max glass (p.y - (neck_top.y - 0.002))
  let glass = smin 0.002 glass (sd_torus (p -^ (neck_top -^ vec 0 0.002 0)) nr 0.0035)
  -- the cork: pushed into the neck (filling it inside the glass wall) and
  -- standing a little proud of the lip
  let cork = sd_cylinder (p -^ (neck_top -^ vec 0 0.004 0)) (nr - 0.0045) 0.011 - 0.0015
  in (glass, cork, level)

-- The leather that holds potion i: a band round its neck, and a strap from
-- the band up under the belt.
def potion_strap (prm: params) (i: i32) (p: vec3) : f32 =
  let neck_top = potion_neck_top prm i
  let nr = potion_neck_radius i
  let band_y = neck_top.y - 0.011
  let q = p -^ vec neck_top.x band_y neck_top.z
  let rq = vec2 (length2 (vec2 q.x q.z) - (nr + 0.0035)) q.y
  let band = length2 (vec2 (f32.max (f32.abs rq.x - 0.0015) 0) (f32.max (f32.abs rq.y - 0.0045) 0)) - 0.001
  let a = potion_angle i
  let out = around a
  -- from the band's side facing the body, up against the robe and on up
  -- underneath the belt (the belt's band spans robe_radius - 0.006 to
  -- + 0.022, so a strap at + 0.006 is hidden inside it)
  let start = vec neck_top.x band_y neck_top.z -^ scale (nr + 0.004) out
  let below = scale (robe_radius prm (prm.belt_height - 0.04) + 0.006) out +^ vec 0 (prm.belt_height - 0.04) 0
  let tucked = scale (robe_radius prm prm.belt_height + 0.006) out +^ vec 0 prm.belt_height 0
  let strap = sd_strap p start below out |> f32.min (sd_strap p below tucked out)
  in f32.min band strap

-- The liquid fills the inside of the glass (0.004 thick) up to its level.
def potion_liquid (prm: params) (i: i32) (p: vec3) : f32 =
  let (glass, _, level) = potion_parts prm i p
  in smax 0.004 (glass + 0.004) (p.y - level)

def potion (prm: params) (i: i32) (p: vec3) : hit =
  let (_, cork, _) = potion_parts prm i p
  in (potion_liquid prm i p, m_potion + i) |> umin (cork, m_cork) |> umin (potion_strap prm i p, m_leather)

-- Where the potions hang, in body space, and a sphere holding all three.
def potions_at (prm: params) : vec3 =
  let y = prm.belt_height - 0.08
  in scale (robe_radius prm y + 0.05) (around (potion_angle 1)) +^ vec 0 y 0
def potions_radius : f32 = 0.26

-- From body space (which leans into turns) back to the figure's local space.
def from_body_space (pose: pose) (q: vec3) : vec3 =
  q +^ vec (pose.lean * f32.max 0 (q.y - 0.45)) 0 0

-- The potions' glass: distance to the nearest outer glass surface, in the
-- figure's local space.
def potion_glass (pose: pose) (p: vec3) : f32 =
  let prm = pose.prm
  in sphere_bounded (potions_at prm) potions_radius 0.05
                 (\q -> f32.min (potion_parts prm 0 q).0 (f32.min (potion_parts prm 1 q).0 (potion_parts prm 2 q).0))
                 (body_space pose p)

-- The nearest potion's liquid: (distance, material id), for the glow.
def nearest_liquid (pose: pose) (p: vec3) : hit =
  let prm = pose.prm
  in sphere_bounded_hit (potions_at prm) potions_radius 0.1 m_potion
    (\q -> (potion_liquid prm 0 q, m_potion) |> umin (potion_liquid prm 1 q, m_potion + 1) |> umin (potion_liquid prm 2 q, m_potion + 2))
    (body_space pose p)

-- A tome hanging on the right hip, lying against the robe's slope.
def tome (prm: params) (p: vec3) : hit =
  let a = deg (-100)
  let y = prm.belt_height - 0.12
  let out = normalise (around a +^ vec 0 0.28 0) -- the robe's cone slopes inwards
  let up = normalise (cross (cross out (vec 0 1 0)) out)
  let side = cross up out
  let c = scale (robe_radius prm y + 0.035) (around a) +^ vec 0 y 0
  let v = p -^ c
  let q = vec (dot v side) (dot v up) (dot v out) -- width, height, thickness
  let box (cx: f32) (cz: f32) (b: vec3) (r: f32) = sd_box (q -^ vec cx 0 cz) (b -^ splat r) - r
  let covers = f32.min (box 0 0.027 (vec 0.08 0.1 0.006) 0.003) (box 0 (-0.027) (vec 0.08 0.1 0.006) 0.003)
  let spine = box (-0.077) 0 (vec 0.008 0.1 0.03) 0.006
  let pages = box 0.004 0 (vec 0.074 0.094 0.022) 0.002
  let clasp = box 0.08 0 (vec 0.006 0.016 0.031) 0.003
  -- a strap from low on the front cover, up and over the top of the belt
  let on_cover (u: f32) = c +^ scale u up +^ scale 0.035 out
  let belt_front = scale (robe_radius prm prm.belt_height + 0.019) (around a) +^ vec 0 (prm.belt_height + 0.016) 0
  let belt_top = scale (robe_radius prm prm.belt_height + 0.002) (around a) +^ vec 0 (prm.belt_height + 0.027) 0
  let strap = sd_strap p (on_cover (-0.04)) (on_cover 0.1) out
              |> f32.min (sd_strap p (on_cover 0.1) belt_front out)
              |> f32.min (sd_strap p belt_front belt_top out)
  in (f32.min covers spine, m_tome) |> umin (pages, m_pages) |> umin (clasp, m_gold) |> umin (strap, m_leather)

def waist_gear (prm: params) (p: vec3) : hit =
  let ty = prm.belt_height - 0.1
  let tome_at = scale (robe_radius prm ty + 0.035) (around (deg (-100))) +^ vec 0 ty 0
  in (sd_belt prm p, m_leather)
     |> umin (sphere_bounded_hit (buckle_centre prm) 0.07 0.15 m_gold (\q -> (sd_buckle prm q, m_gold)) p)
     |> umin (sphere_bounded_hit (potions_at prm) potions_radius 0.15 m_leather (\q -> potion prm 0 q |> umin (potion prm 1 q) |> umin (potion prm 2 q)) p)
     |> umin (sphere_bounded_hit tome_at 0.19 0.15 m_tome (tome prm) p)

---------------------------------------------------------------------
-- Rune circle: 14 glyphs on a circle of radius 1.125, turning once per loop.

def rune_radius : f32 = 1.125

-- 2D distance in the ground plane to the nearest rune stroke.
def glyph_sdf (tm: f32) (x: f32) (z: f32) : f32 =
  let c = rune_radius
  let r = length2 (vec2 x z)
  -- Far from the band, a cheap lower bound. It is only used where the glow
  -- density is negligible, so the bound's looseness never shows.
  in if f32.abs (r - c) > 0.45 then f32.abs (r - c) - 0.12
  else
    let n = 14f32
    let sector = 2 * f32.pi / n
    let q = rot2 tm (vec2 x z)
    let a = f32.atan2 q.y q.x + f32.pi
    let idx = i32.f32 (f32.floor (a / sector))
    let lp = rep_angle n {x = q.x, y = 0, z = q.y}
    let h1 = hash2 idx 3
    let h2 = hash2 idx 7
    let seg (ax: f32) (az: f32) (bx: f32) (bz: f32) =
      sd_capsule lp (vec (c + ax) 0 az) (vec (c + bx) 0 bz) 0.008
    let g1 = seg (-0.08) (-0.05 + 0.1 * h1) 0.08 (0.05 - 0.1 * h1)
    let g2 = seg (-0.08 + 0.16 * h2) (-0.06) (0.08 - 0.16 * h1) 0.06
    let g3 = if h2 > 0.5 then seg 0 (-0.07) 0 0.07 else sd_torus (lp -^ vec c 0 0) 0.05 0.004
    in f32.min g1 (f32.min g2 g3)

-- Approximate 3D distance to the nearest letter: the in-plane distance and
-- the height above the ground, combined by Pythagoras.
def glyph_dist (tm: f32) (p: vec3) : f32 =
  if p.y > 0.35 then p.y -- high above the ground: skip the strokes
  else let d = f32.max 0 (glyph_sdf tm p.x p.z)
       in f32.sqrt (d * d + p.y * p.y)

-- Emission painted on the ground (before brightness): two rings and the glyphs.
def rune_paint (tm: f32) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let ring w c = 1 - smoothstep (w * 0.5) w (f32.abs (r - c))
  let rings = f32.max (ring 0.018 0.95) (ring 0.012 1.3)
  let glyph = 1 - smoothstep 0.0 0.008 (glyph_sdf tm p.x p.z)
  in f32.max rings glyph

---------------------------------------------------------------------
-- Whole scene

-- Every wizard part lies inside this capsule, so its distance is a lower
-- bound on the wizard's. Beyond the requested tolerance, return the bound and
-- skip evaluating the parts (see the distance contract in march.fut).

---------------------------------------------------------------------
-- Layers. Every part of the scene belongs to one of the layers_* (generated
-- from the "layers" parameter in params.json), and the scene functions take
-- a mask of the layers to include. A hit's material id carries its layer in
-- bits 8 and up (material ids are below 256).

def tag (layer: i32) (h: hit) : hit = (h.0, h.1 | (layer << 8))
def hit_material (m: i32) : i32 = m & 255
def hit_layer (m: i32) : i32 = m >> 8
def has (mask: i32) (layer: i32) : bool = mask & layer != 0
def nothing : hit = (1e9, 0) -- (finite: shadow maths squares distances)

-- The parts of the figure in the layers of `mask`.
def wizard (mask: i32) (pose: pose) (p: vec3) : hit =
  let figure =
    if !(has mask layers_figure) then nothing
    else (smin 0.06 (sd_robe pose (body_space pose p)) (sd_arms pose p), m_robe)
         |> umin (sd_hands pose p, m_skin)
         |> umin (sd_head pose p, m_skin)
         |> umin (sd_eyes pose p, m_eye)
         |> umin (sd_eyelids pose p, m_skin)
         |> umin (sd_brows pose p, m_brow)
         |> umin (sd_beard pose p, m_beard)
         |> umin (sd_hat pose p, m_hat)
         |> umin (sd_band pose p, m_band)
         |> tag layers_figure
  let gear = if !(has mask layers_gear) then nothing
             else waist_gear pose.prm (body_space pose p) |> tag layers_gear
  in figure |> umin gear

-- The staff and its cradle, which move together when he holds the staff
-- out (at q, in the staff's rest frame).
def held_staff (mask: i32) (q: vec3) : hit =
  if !(has mask layers_staff) then nothing
  else (sd_staff q, m_staff) |> umin (sd_cradle q, m_gold) |> tag layers_staff

def staff_bound (q: vec3) : f32 = sd_capsule q staff_foot orb_pos 0.25

-- (with room for the left arm reaching out with the staff)
def wizard_bound (p: vec3) : f32 = sd_capsule p (vec 0 0.3 0) (vec 0 2.3 0) 1.1

def figure_layers : i32 = layers_figure | layers_staff | layers_gear

#[noinline] -- (see scene; no 'z' in the name)
def bounded_figure (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  let b = wizard_bound p
  let q = staff_rest pose p
  let sb = staff_bound q
  in if !(has mask figure_layers) then nothing
     else (if b > tol then (b, m_robe) else wizard mask pose p)
          |> umin (if sb > tol then (sb, m_staff) else held_staff mask q)

-- The orb is glass, refracted analytically by the shading (see shading.fut),
-- so it is not part of the marched scene; what is inside it is drawn by the
-- media (see media.fut and lightning.fut).
---------------------------------------------------------------------
-- The floating rock the wizard stands on: an inverted cone with a flat top
-- (the magic circle's outer ring, at radius 1.3, fits inside), roughened
-- below by noise that fades out towards the rim.

def rock_radius : f32 = 1.6
def rock_depth : f32 = 2.8

def rock_exact (p: vec3) : hit =
  let cone = sd_capped_cone (p -^ vec 0 (-rock_depth) 0) rock_depth 0.3 rock_radius
  -- (the weight is 0 at and above the top face, where the wizard is, so the
  -- noise is only evaluated below it)
  let rough = if p.y >= 0 then 0
              else saturate (-p.y * 2.5) * (0.22 * vnoise3 (scale 2.2 p) + 0.1 * vnoise3 (scale 5.3 p) - 0.16)
  -- the noise steepens the field; scale it back to keep tracing safe
  in (0.6 * (cone + rough), if p.y > -0.03 then m_ground else m_rock)

-- (the rock fits in a sphere of radius 2.3; its noise needs a wider margin)
def rock (p: vec3) : hit = sphere_bounded_hit (vec 0 (-1.2) 0) 2.3 0.3 m_rock rock_exact p |> tag layers_rock

-- The terrain far below, sampled through the flight frame. No peak is above
-- terrain_max, so well above it the height over terrain_max is a cheap lower
-- bound (it stays >= 1.5 here, so it can never register as a hit).
--
-- Terrain detail (octaves) is deliberately not reduced with distance. A
-- distance-based fade is centred on the wizard, who moves through the world,
-- so the ground changed shape as he flew past it: the terrain visibly swam.
-- Measured against a high-quality reference over consecutive flight frames,
-- the fade caused about three times the flicker of the cloud sampling, and
-- pushing it out to 30-60 units only halved that while costing the same as
-- no fade at all (17.2 ms at 1080p either way).
def terrain_detail (_p: vec3) : f32 = 5

def land_of (prm: params) : land = {sea = prm.sea_level, river = prm.river_width}

-- The world below, in the layers of `mask`: the ground, the water (sea and
-- rivers, a plane at sea level) and the structures.
def land_layers : i32 = layers_terrain | layers_water | layers_structures

def terrain (mask: i32) (pose: pose) (p: vec3) : hit =
  let prm = pose.prm
  let l = land_of prm
  let w = to_world pose.fl p
  let detail = terrain_detail p
  let ground =
    if !(has mask layers_terrain) then nothing
    else if w.y > terrain_max + 1.5 then (w.y - terrain_max, m_terrain) |> tag layers_terrain
    else
      -- the cheap bound clears most points near the wizard, his rock and
      -- the clouds; only close to the ground is the full terrain needed
      -- (rivers only lower the ground, so it stays an upper bound)
      let gap = w.y - terrain_upper w.x w.z
      in (if gap > 0.4 then 0.6 * gap else terrain_dist l detail w, m_terrain) |> tag layers_terrain
  let water = if has mask layers_water then (w.y - prm.sea_level, m_water) |> tag layers_water else nothing
  let built = if !(has mask layers_structures) then nothing
              else let (d, part) = structures l prm.structure_density w
                   in (d, m_wall + part) |> tag layers_structures
  in ground |> umin water |> umin built

-- #[noinline]: Futhark inlines everything by default, and the scene is used
-- by the march, normals, AO and both shadow loops, which made the shader
-- several megabytes (Firefox's compiler and runtime suffered most). The
-- figure, the scene, the occluders and the media are compiled once as real
-- GPU functions instead. The WebGPU backend's limits here:
-- - such functions may not use arrays;
-- - a GPU function whose name contains a 'z' cannot be called from another
--   GPU function: the call site escapes the name ('zz') but the definition
--   does not, so the call has no target. Hence bounded_figure, not
--   bounded_wizard. Fixed upstream; the pinned compiler predates the fix,
--   so keep the name until flake.lock is updated. See
--   dev/futhark-bugs/z-name-mangling.fut.
-- The scene near the wizard, in the layers of `mask`: the figure, its staff
-- and gear, and the rock. (Primary rays march this and the terrain in
-- separate passes, see follow in shading.fut.)
#[noinline]
def local_scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  bounded_figure mask pose tol p |> umin (if has mask layers_rock then rock p else nothing)

def local_layers : i32 = figure_layers | layers_rock

-- Everything in local_scene lies in this sphere (radius 3.3), and so do the
-- glowing media (the orb's cloud and tendrils, the rune circle; 3.6).
def occluder_centre : vec3 = vec 0 (-0.3) 0
def occluder_radius : f32 = 3.3
def local_radius : f32 = 3.6

#[noinline]
def scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  local_scene mask pose tol p |> umin (if has mask land_layers then terrain mask pose p else nothing)

def dist (mask: i32) (pose: pose) (tol: f32) (p: vec3) : f32 = (scene mask pose tol p).0

-- The terrain pass: sphere-traces the land alone (the layers of `mask` among
-- land_layers) along ro + t rd, for t in [0, tmax], returning the hit's
-- distance and tagged material, or infinity. A ray heading upwards
-- from above the highest peak cannot hit it at all, and one heading down
-- jumps straight to that height before stepping.
#[noinline]
def march_terrain (mask: i32) (pose: pose) (ro: vec3) (rd: vec3) (tmax: f32) : (f32, i32) =
  let wo = to_world pose.fl ro
  let wd = to_world_dir pose.fl rd
  let top = land_max + 1.5
  in if wd.y >= 0 && wo.y > land_max then (f32.inf, 0) else
  let t0 = if wo.y > top then (wo.y - top) / -wd.y else 0
  let (t, _, hit, m) =
    loop (t, i, hit, m) = (f32.max 0.02 t0, 0i32, false, 0i32)
    while !hit && i < terrain_steps && t < tmax do
      let (d, mat) = terrain mask pose (ro +^ scale t rd)
      in if d < 0.0004 * t then (t, i + 1, true, mat) else (t + d * 0.85, i + 1, false, m)
  in if hit then (t, m) else (f32.inf, 0)
