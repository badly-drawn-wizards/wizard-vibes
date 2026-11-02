-- The enemies' states, from the harness (web/enemies.ts), one float each:
-- 16 times its generation (how many enemies have had its place before it)
-- plus its phase: under 8, the seconds it has still to appear; 8 and over,
-- 8 plus the seconds since it was defeated. 0 is one there from the start.
-- (Apart from enemies.fut, so the pose can carry them.)

type foes = (f32, f32, f32, f32, f32, f32)

def foe_state (fs: foes) (i: i32) : f32 =
  if i == 0 then fs.0 else if i == 1 then fs.1 else if i == 2 then fs.2
  else if i == 3 then fs.3 else if i == 4 then fs.4 else fs.5
