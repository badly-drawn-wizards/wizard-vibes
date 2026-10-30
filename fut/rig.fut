-- The wizard's rig: the staff's place, the arms' IK, and the pose (everything
-- about him that moves, worked out once per pixel from the time, the
-- uniforms and the harness's drive record).

import "vec3"
import "params"
import "anim"
import "world"

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
            -- rigidly: rotated to columns (staff_x, staff_y, staff_z) and
            -- moved by staff_t (see staff_world and staff_rest).
            , staff_x: vec3, staff_y: vec3, staff_z: vec3, staff_t: vec3
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
  pose.staff_t +^ scale q.x pose.staff_x +^ scale q.y pose.staff_y +^ scale q.z pose.staff_z
def staff_rest (pose: pose) (p: vec3) : vec3 =
  let v = p -^ pose.staff_t in vec (dot v pose.staff_x) (dot v pose.staff_y) (dot v pose.staff_z)

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
  -- The staff leans with him, half as far, tilting about its foot (a shear
  -- of lean per unit height is a tilt of atan lean).
  let tilt = -(f32.atan (0.5 * lean))
  let leaned (q: vec3) = staff_foot +^ rotate_about (vec 0 0 1) (f32.cos tilt) (f32.sin tilt) (q -^ staff_foot)
  let leaned_dir = rotate_about (vec 0 0 1) (f32.cos tilt) (f32.sin tilt) staff_dir
  -- Casting: the staff turns about his grip to lead with the orb (raised a
  -- little above the aim) and is pushed forward, with a thrust on each cast.
  let aim = normalise (vec dr.sx dr.sy dr.sz +^ vec 0 0 1e-4)
  let want = normalise (aim +^ vec 0 0.9 0)
  let k = cross leaned_dir want
  let staff_axis = if length k > 1e-4 then normalise k else vec 1 0 0
  let angle = 0.85 * dr.staff * f32.acos (clamp (-1) 1 (dot leaned_dir want))
  let kick = dr.staff * f32.exp (-12 * dr.flash)
  let staff_pivot = leaned (staff_at grip_height)
  -- Aiming to his right (local -x), he brings the staff across in front of
  -- his chest, the further the more he aims that way.
  let across = dr.staff * smoothstep 0 0.7 (-aim.x)
  let staff_shift = scale (0.16 * dr.staff + 0.07 * kick) (normalise (vec aim.x 0 aim.z +^ vec 0 0 1e-4))
                    +^ vec (-0.6 * across) (0.08 * dr.staff + 0.05 * across) (0.22 * across)
  let (staff_cos, staff_sin) = (f32.cos angle, f32.sin angle)
  let held (q: vec3) = staff_pivot +^ rotate_about staff_axis staff_cos staff_sin (leaned q -^ staff_pivot) +^ staff_shift
  -- (that, as a rotation and offset)
  let staff_t = held (splat 0)
  let column (e: vec3) = held e -^ staff_t
  -- The left hand stays on the staff, closed round it with the palm facing
  -- the elbow, wherever the staff and the elbow are: the arm reaches for the
  -- staff, then the hand is built round the staff's grip facing the elbow
  -- that gives, and the arm reaches again for its wrist.
  let l_grip = held (staff_at grip_height)
  let l_axis = rotate_about staff_axis staff_cos staff_sin leaned_dir
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
     , staff_x = column (vec 1 0 0), staff_y = column (vec 0 1 0), staff_z = column (vec 0 0 1), staff_t
     , orb = held orb_pos, orb_tint, bolt_tint, orb_boost, spell }
