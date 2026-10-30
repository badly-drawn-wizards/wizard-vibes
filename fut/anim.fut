-- Animation timing. The animation loops every `loop_seconds`. Inside the
-- renderer `tm` is the loop angle in [0, 2pi), and every animated term uses a
-- whole-number multiple of it, so the last frame of the loop matches the first.

import "vec3"

def loop_seconds : f32 = 12
def loop_angle (seconds: f32) : f32 = 2 * f32.pi * fract (seconds / loop_seconds)
