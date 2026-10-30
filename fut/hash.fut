-- Integer hashes -> [0, 1).

def hash3 (x: i32) (y: i32) (z: i32) : f32 =
  let h = u32.i32 x * 73856093 ^ u32.i32 y * 19349663 ^ u32.i32 z * 83492791
  let h = (h ^ (h >> 16)) * 0x7feb352d
  let h = (h ^ (h >> 15)) * 0x846ca68b
  let h = h ^ (h >> 16)
  in f32.u32 (h >> 8) / 16777216

def hash2 (x: i32) (y: i32) : f32 = hash3 x y 1337
