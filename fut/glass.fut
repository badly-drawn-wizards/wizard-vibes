-- Glass of any shape: Fresnel reflection and refraction at surfaces given as
-- signed distance functions (negative inside). The orb (a sphere, met
-- analytically), the potions' flasks and his shield all go through here.

import "vec3"
import "sdf"
import "march"

-- Refraction of i at a surface with normal n facing against i, for the ratio
-- eta of refractive indices (from / to). Reflects on total internal reflection.
def refract (i: vec3) (n: vec3) (eta: f32) : vec3 =
  let cosi = -(dot n i)
  let k = 1 - eta * eta * (1 - cosi * cosi)
  in if k < 0 then reflect i n
     else scale eta i +^ scale (eta * cosi - f32.sqrt k) n

-- Schlick's approximation of the reflected fraction.
def fresnel (ior: f32) (cosi: f32) : f32 =
  let r0 = ((1 - ior) / (1 + ior)) ** 2
  in r0 + (1 - r0) * pow (1 - saturate cosi) 5

-- A ray along rd meeting a surface of normal n (either way round) into a
-- medium of index ior (from air; pass 1 / ior for the way out): the normal
-- turned to face the ray, the reflected fraction, and the reflected and
-- refracted directions.
type interface = {n: vec3, f: f32, reflected: vec3, refracted: vec3}

def fresnel_at (ior: f32) (rd: vec3) (n: vec3) : interface =
  let n = if dot n rd > 0 then scale (-1) n else n
  let cosi = saturate (-(dot n rd))
  -- (leaving a denser medium, Schlick needs the angle on the denser side)
  let f = if ior >= 1 then fresnel ior cosi
          else let s2 = (1 - cosi * cosi) / (ior * ior)
               in if s2 >= 1 then 1 else fresnel (1 / ior) (f32.sqrt (1 - s2))
  in {n, f, reflected = reflect rd n, refracted = refract rd n (1 / ior)}

-- Where the ray ro + t rd first meets glass `sdf` for t in [t0, t1], from
-- the side it starts on (outside if sdf > 0 there): (whether it does, t,
-- the surface normal there). Sphere-traced, in at most `steps` steps;
-- callers bound the stretch [t0, t1] tightly (e.g. by a bounding sphere).
def glass_hit (sdf: vec3 -> f32) (steps: i32) (ro: vec3) (rd: vec3) (t0: f32) (t1: f32) : (bool, f32, vec3) =
  let side = if sdf (ro +^ scale t0 rd) < 0 then -1f32 else 1
  let (t, _, hit) =
    loop (t, i, hit) = (t0, 0i32, false) while !hit && i < steps && t < t1 do
      let d = side * sdf (ro +^ scale t rd)
      in if d < 0.0004 * t + 1e-4 then (t, i + 1, true) else (t + 0.9 * d, i + 1, false)
  in if !hit then (false, t, vec 0 1 0)
     else let p = ro +^ scale t rd
          in (true, t, normal (\_ q -> sdf q) p)
