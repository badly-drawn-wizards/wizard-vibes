-- The enemies' states, from the harness (web/enemies.ts). Per enemy, one
-- float: 16 times its generation (how many enemies have had its place
-- before it) plus its phase: under 8, the seconds it has still to appear;
-- 8 and over, 8 plus the seconds since it was defeated (0 is one there
-- from the start). And the arena: the point in the world they circle.
-- (Apart from enemies.fut, so the pose can carry them.)

import "vec3"

type foes = {states: (f32, f32, f32, f32, f32, f32), arena: vec3}

def foe_state (fs: foes) (i: i32) : f32 =
  let s = fs.states
  in if i == 0 then s.0 else if i == 1 then s.1 else if i == 2 then s.2
     else if i == 3 then s.3 else if i == 4 then s.4 else s.5
