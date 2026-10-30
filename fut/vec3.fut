-- Small 2D/3D vector library. Records rather than arrays so everything
-- stays in registers inside the per-pixel kernel.

type vec2 = {x: f32, y: f32}
type vec3 = {x: f32, y: f32, z: f32}

def vec (x: f32) (y: f32) (z: f32) : vec3 = {x, y, z}
def vec2 (x: f32) (y: f32) : vec2 = {x, y}
def splat (s: f32) : vec3 = {x = s, y = s, z = s}

def (a: vec3) +^ (b: vec3) : vec3 = {x = a.x + b.x, y = a.y + b.y, z = a.z + b.z}
def (a: vec3) -^ (b: vec3) : vec3 = {x = a.x - b.x, y = a.y - b.y, z = a.z - b.z}
def (a: vec3) *^ (b: vec3) : vec3 = {x = a.x * b.x, y = a.y * b.y, z = a.z * b.z}

def scale (s: f32) (v: vec3) : vec3 = {x = s * v.x, y = s * v.y, z = s * v.z}
def dot (a: vec3) (b: vec3) : f32 = a.x * b.x + a.y * b.y + a.z * b.z
def cross (a: vec3) (b: vec3) : vec3 =
  {x = a.y * b.z - a.z * b.y, y = a.z * b.x - a.x * b.z, z = a.x * b.y - a.y * b.x}
def length (v: vec3) : f32 = f32.sqrt (dot v v)
def normalise (v: vec3) : vec3 = scale (1 / length v) v

def vmap (f: f32 -> f32) (v: vec3) : vec3 = {x = f v.x, y = f v.y, z = f v.z}
def vabs (v: vec3) : vec3 = vmap f32.abs v
def vmaxs (v: vec3) (s: f32) : vec3 = vmap (f32.max s) v
def maxc (v: vec3) : f32 = f32.max v.x (f32.max v.y v.z)
def mix (a: vec3) (b: vec3) (t: f32) : vec3 = a +^ scale t (b -^ a)
def reflect (i: vec3) (n: vec3) : vec3 = i -^ scale (2 * dot n i) n

def length2 (v: vec2) : f32 = f32.sqrt (v.x * v.x + v.y * v.y)
def rot2 (a: f32) (p: vec2) : vec2 =
  let c = f32.cos a
  let s = f32.sin a
  in {x = c * p.x - s * p.y, y = s * p.x + c * p.y}

def clamp (lo: f32) (hi: f32) (x: f32) : f32 = f32.max lo (f32.min hi x)
def saturate (x: f32) : f32 = clamp 0 1 x
def smoothstep (e0: f32) (e1: f32) (x: f32) : f32 =
  let t = saturate ((x - e0) / (e1 - e0)) in t * t * (3 - 2 * t)
def fract (x: f32) : f32 = x - f32.floor x
def mix1 (a: f32) (b: f32) (t: f32) : f32 = a + (b - a) * t
def pow (x: f32) (y: f32) : f32 = x ** y
