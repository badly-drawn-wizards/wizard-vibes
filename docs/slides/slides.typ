// Techniques behind the Futhark wizard: a slideshow for programmers new to
// computer graphics. Build: typst compile docs/slides/slides.typ
// (no packages needed).

#let bg = rgb("#0b0a1c")
#let fg = rgb("#e4e0ff")
#let dim = rgb("#9a96c8")
#let accent = rgb("#c9a227")
#let glow = rgb("#7fd3ff")

#set page(width: 32cm, height: 18cm, margin: (x: 1.6cm, y: 1.2cm), fill: bg)
#set text(font: ("DejaVu Sans", "Liberation Sans"), size: 17pt, fill: fg)
#set par(leading: 0.6em)
#show raw: set text(font: ("DejaVu Sans Mono", "Liberation Mono"), size: 13pt)
#show raw.where(block: true): it => block(fill: rgb("#16142e"), inset: 12pt, radius: 6pt, width: 100%, it)
#show heading.where(level: 1): it => block(below: 0.7em, text(size: 30pt, fill: accent, weight: "bold", it.body))
#show strong: set text(fill: glow)
#set list(marker: text(fill: accent)[•], spacing: 0.75em)

#let slide(title, body) = {
  pagebreak(weak: true)
  heading(level: 1, title)
  body
}
#let note(body) = text(size: 15pt, fill: dim, body)
#let two(left, right, ratio: (1fr, 1fr)) = grid(columns: ratio, gutter: 1.2cm, left, right)
#let img(path, ..args) = image("img/" + path, ..args)

// ---------------------------------------------------------------------
// Title

#page(margin: 0pt, fill: bg)[
  #place(img("hero.png", width: 100%, height: 100%, fit: "cover"))
  #place(bottom + left, dx: 1.6cm, dy: -1.4cm, block(fill: rgb(11, 10, 28, 210), inset: 18pt, radius: 8pt)[
    #text(size: 38pt, weight: "bold", fill: accent)[A wizard, ray-marched]
    #v(-0.3em)
    #text(size: 20pt)[Rendering with distance functions, in Futhark, on WebGPU]
    #v(-0.2em)
    #text(size: 15pt, fill: dim)[Techniques for programmers who haven't done graphics]
  ])
]

#slide[What we built][
  #two(ratio: (1.1fr, 1fr))[
    - A wizard on a floating rock, flying over endless terrain with rivers, a sea, cottages, towers and pyramids
    - Every pixel is computed by a *pure function*: no meshes, no triangles, no game engine
    - Written in *Futhark*, a functional array language, compiled to *WebGPU* compute shaders
    - Live in the browser: hot reload, a parameter drawer, free flight, spells, a game controller
    - 1080p at 60 fps on a laptop GPU
  ][
    #img("cast.png", width: 100%)
    #note[Everything here, from the beard to the clouds, is maths evaluated per pixel.]
  ]
]

// ---------------------------------------------------------------------
// Part 1: geometry as functions

#slide[The usual way, and ours][
  #two[
    *Rasterisation* (games, most 3D)
    - Shapes are lists of triangles
    - The GPU projects each triangle onto the screen and fills its pixels
    - Fast, but every shape must first be modelled as a mesh
  ][
    *Ray marching* (this project)
    - Shapes are *functions* $d(p)$: "how far is point $p$ from the surface?"
    - For each pixel, shoot a ray from the camera and walk along it until it hits something
    - Shapes are a few lines of maths; blending, bending and repeating them is trivial
  ]
  #v(0.5em)
  #note[The idea is old (Hart, 1996, "Sphere tracing"); it became popular through the demoscene and Shadertoy.]
]

#slide[Signed distance functions][
  #two(ratio: (1.2fr, 1fr))[
    A *signed distance function* (SDF) returns the distance from $p$ to the nearest surface: positive outside, negative inside, zero on it.

    ```
    def sd_sphere p r = length p - r

    -- a segment a-b, thickened by r
    def sd_capsule p a b r =
      let pa = p - a
      let ba = b - a
      let h = clamp 0 1 (dot pa ba / dot ba ba)
      in length (pa - scale h ba) - r
    ```
    A scene is the *minimum* of its parts' distances: the nearest surface wins.
  ][
    #align(center)[
      #box(width: 11cm, height: 9cm, {
        place(dx: 3cm, dy: 2cm, circle(radius: 2.5cm, stroke: 2pt + accent))
        place(dx: 5.5cm, dy: 4.5cm, circle(radius: 3pt, fill: accent))
        place(dx: 9.2cm, dy: 1.2cm, circle(radius: 4pt, fill: glow))
        place(dx: 8.1cm, dy: 0.1cm, text(fill: glow, size: 15pt)[$p$])
        place(line(start: (9.2cm, 1.25cm), end: (7.35cm, 2.85cm), stroke: (paint: glow, thickness: 1.5pt, dash: "dashed")))
        place(dx: 8.6cm, dy: 2.2cm, text(fill: glow, size: 15pt)[$d(p)$])
        place(dx: 4.7cm, dy: 7.3cm, text(fill: dim, size: 14pt)[$d < 0$ inside])
      })
    ]
  ]
]

#slide[Sphere tracing][
  #two(ratio: (1.3fr, 1fr))[
    #align(center, box(width: 15cm, height: 8.5cm, clip: true, {
      // circles computed from the scene's distance function (a sphere and a
      // wall): each step's circle just touches the nearest surface
      let pts = ((0.6, 5.4, 4.63), (5.14, 4.53, 0.79), (5.91, 4.38, 0.48), (6.39, 4.29, 0.42), (6.8, 4.21, 0.47), (7.26, 4.12, 0.61), (7.87, 4.0, 0.93), (8.78, 3.83, 1.57), (10.32, 3.53, 2.89), (13.15, 2.99, 0.45), (13.59, 2.9, 0.01))
      place(dx: 4.4cm, dy: 0.6999999999999997cm, circle(radius: 1.6cm, fill: accent.transparentize(70%), stroke: 2pt + accent))
      place(dx: 13.6cm, dy: 0cm, rect(width: 2cm, height: 8.5cm, fill: accent.transparentize(70%), stroke: none))
      place(line(start: (13.6cm, 0cm), end: (13.6cm, 8.5cm), stroke: 2pt + accent))
      place(line(start: (0.6cm, 5.4cm), end: (13.59cm, 2.9cm), stroke: (paint: dim, thickness: 1pt, dash: "dotted")))
      for (x, y, r) in pts {
        place(dx: (x - r) * 1cm, dy: (y - r) * 1cm, circle(radius: r * 1cm, stroke: 1pt + glow.transparentize(45%)))
        place(dx: x * 1cm - 3pt, dy: y * 1cm - 3pt, circle(radius: 3pt, fill: glow))
      }
      place(dx: 0.1cm, dy: 5.8cm, text(fill: dim, size: 14pt)[camera])
      place(dx: 4.9cm, dy: 0.1cm, text(fill: accent, size: 14pt)[sphere])
      place(dx: 12.2cm, dy: 7.6cm, text(fill: accent, size: 14pt)[wall])
    }))
  ][
    The distance $d(p)$ is a *safe step*: nothing is closer than that, so we can jump that far along the ray without passing through anything.

    ```
    loop t = 0 while t < tmax do
      let d = scene (ro + t * rd)
      in if d < eps * t then hit t
         else t + d
    ```
    Big steps in empty space, tiny ones near surfaces. A few dozen steps per pixel usually suffice.
  ]
]

#slide[Combining shapes][
  #two[
    - *Union*: `min a b` (the nearer surface)
    - *Subtraction*: `max a (-b)` (carve b out of a)
    - *Smooth union*: blend two shapes with a fillet of radius k. This is how the robe flows into the shoulders and the arms into the robe:
    ```
    def smin k a b =
      let h = clamp 0 1 (0.5 + 0.5 * (b - a) / k)
      in mix b a h - k * h * (1 - h)
    ```
    - Each part also carries a *material id*, so the nearest one decides the colour
  ][
    - *Transform space, not shapes*: to move, rotate or bend a shape, transform the point before asking its SDF
      - the hat droops: `sd_cone (bend droop p)`
      - the head turns: `sd_head (rotate (-look) p)`
    - *Repetition*: fold space into a cell so one shape becomes many (`fract`, angular sectors). The orb's cradle prongs and the rune circle are folded this way
    - Distortions break the "safe step" promise, so we scale the distance down by the worst stretch
  ]
]

#slide[Normals, shadows and ambient occlusion: all from d][
  #two[
    *Normal* = gradient of $d$, by finite differences. A tetrahedron of 4 samples instead of 6:
    ```
    n = normalise (Σ k · d(p + e·k))
        for k in 4 tetrahedron corners
    ```
    *Soft shadows*: march towards the light; how close the ray grazes other surfaces ($d / t$, smallest along the way) gives a penumbra for free.
  ][
    *Ambient occlusion*: sample $d$ a few times along the normal. If it is smaller than the distance travelled, something is nearby blocking the sky: darken.

    #v(0.5em)
    All three only need the same function $d$. No shadow maps, no light probes.
    #v(0.5em)
    #note[Lighting then is classic: diffuse (Lambert) + specular highlight (Blinn) + rim light + fill, per light: the moon, the orb, three potions.]
  ]
]

#slide[Textures without images][
  #two(ratio: (1fr, 1fr))[
    - Every pattern is procedural: *hashes* give repeatable randomness, *value noise* interpolates it, *fbm* sums octaves of it
    - Bark, cloth weave, leather stitching, beard locks, hat stars, bricks
    - *Bump mapping*: a height pattern tilts the normal (its gradient) without changing the geometry
    - *Footprint filtering*: a pixel at distance $t$ is ≈ $t ·$ (angle per pixel) wide; patterns finer than that fade out instead of flickering
  ][
    #img("cottage.png", width: 100%)
    #note[Bricks: cells with every other row offset, a hash per brick for its shade, mortar as a bump.]
  ]
]

#slide[Glass, glow and clouds][
  #two(ratio: (1fr, 1.1fr))[
    - *The orb* is solved analytically (ray–sphere), and the ray *refracted* in and out (Snell's law), with a *Fresnel* reflection
    - *Glows* are *media*: each march step adds light by a density, e.g. $e^(-d \/ "extent")$ near the orb; steps are capped so thin glows aren't skipped
    - *Lightning*: jagged polylines; distance to them gives a hot core and a halo
    - *Clouds*: a slab between two heights, integrated with a fixed sample budget apart from the march
  ][
    #img("orb.png", width: 100%)
  ]
]

// ---------------------------------------------------------------------
// Part 2: the world

#slide[Terrain: a heightfield, carefully][
  #two(ratio: (1fr, 1fr))[
    - Height $h(x, z)$ = sum of noise octaves (fbm), plus ridges $1 - |2f - 1|$
    - A heightfield is not a distance, but its slope is bounded, so $0.6 (y - h(x,z))$ is a safe underestimate
    - *Analytic normals*: the noise is differentiated alongside its value (chain rule), so one evaluation gives the slope instead of four
    - *Rivers* carve along a contour of another noise, with the gradient carried through the carving
    - *A cheap upper bound* (first octaves only) stands in for the full height while far above the ground
  ][
    #img("terrain.png", width: 100%)
    #note[The wizard stays at the origin; the world moves past him through a "flight frame" (position, heading, bank).]
  ]
]

#slide[Infinite buildings: hashing a grid][
  #two[
    - The world is divided into 30-unit cells; a hash of the cell's coordinates decides whether it has a building, which kind, where and how rotated
    - The building's distance is exact nearby, but marching every cell would be ruinous. Instead, *conservative bounds*:
      - above every roof: height above the tallest one
      - other cells' buildings sit inside their cells, so they are at least as far as this cell's border
  ][
    ```
    def structures w =
      let above = w.y - structure_top
      in if above > 0.3 then above else
      let far = cell_border w + margin
      in if !present (cell w) then far else
      ...
      in min (sd_structure kind q) far
    ```
    #note[Every value returned is a *lower bound* on the true distance, so stepping by it can never pass through a wall.]
  ]
]

#slide[Two marches and layers][
  - The scene mixes scales: millimetre fingers next to a 150-unit horizon. One march over everything would take tiny steps everywhere
  - So each camera ray is marched *twice*:
    + the *land* alone (terrain, water, buildings) over the whole ray, with its own tricks
    + the *local scene* (wizard, staff, rock, glows) only inside a sphere around him, up to where the land was hit
  - Every part belongs to a *layer* (a bit in a mask): the same scene functions serve the camera, the shadows (which parts cast) and the debug toggles in the drawer
  - *Bounding volumes* everywhere: far from a capsule around the figure, return the capsule's distance and skip all the parts inside
]

// ---------------------------------------------------------------------
// Part 3: Futhark and the browser

#slide[Futhark: a functional language for GPUs][
  #two[
    - Pure, statically typed, array-oriented (like a small ML or Haskell)
    - Programs are written with `map`, `reduce`, `scan`, `filter`, `tabulate`; the compiler turns them into GPU kernels
    - Records and tuples compile to registers: a `vec3` is just three floats
    - A frame is one expression:
    ```
    tabulate (w * h) (\i ->
      let (x, y) = (i % w, i / w)
      in pack (pixel prm x y))
    ```
  ][
    - Backends: C, multicore, CUDA, OpenCL, and (on a development branch) *WebGPU*, generating WGSL shaders plus a WebAssembly host
    - The same source also compiles to a *CPU program*: byte-exact reference renders, used to check that refactors change nothing
    - Functions are inlined by default; `#[noinline]` keeps one as a real GPU function. This turned out to matter a lot (later)
  ]
]

#slide[From Futhark to the screen, without the CPU][
  #two[
    + Futhark's WebGPU backend writes the frame into a GPU *storage buffer* of packed pixels
    + The harness (TypeScript) gets that buffer's handle out of the WebAssembly module
    + One `copyBufferToTexture` puts it into the canvas
    #v(0.4em)
    No round trip through the CPU, no extra drawing pass.
    #note[Futhark creates its own GPU device; the harness captures it by wrapping `requestDevice` during initialisation.]
  ][
    - *Uniforms*: ~80 parameters (colours, sizes, glow strengths) from a generated drawer, passed as a Futhark *record*. The drawer, the record type and its defaults are all generated from one JSON schema
    - *Compile-time parameters* (step counts) rebuild the shader: the dev server rebuilds on save, content-addressed by the hash of every input, and hot-swaps the build into the open page
  ]
]

#slide[Interactive, while staying pure][
  #two[
    - All state lives in TypeScript: camera, free flight (velocity, drag, banking), spells in flight, cooldowns
    - Each frame it is passed in as plain data: a *drive* record (where he is, how he accelerates, the staff gesture) and a flat array of spells
    - The renderer stays a pure function of (time, parameters, state)
    - *Inverse kinematics* (two-bone, closed form) places elbows so the hands reach the staff and the pointing target
  ][
    - *Collisions* need the scene's distance on the CPU side. A second tiny entry point, `probe`, evaluates it at a few points (spheres around the rock, each spell)
    - Its 100-odd bytes are copied out and read back *asynchronously*, a frame or two late, so rendering never waits on it
    - Spells are glows computed in closed form per pixel (distance from the ray to a segment), so they cost no extra marching
  ]
]

#slide[A WebGL fallback from the same program][
  - WebGL has no compute shaders, and Futhark only generates compute kernels
  - But the per-pixel work sits in two GPU functions that take only scalars. So each build gets a *second entry point*: a fragment shader that calls them, fed from a uniform block
  - *naga* (the WGSL translator from the Rust wgpu project) turns it into GLSL for WebGL2
  - The page picks WebGPU, or falls back if the browser has none, or its adapter fails, without looping or giving up silently
  - Shader compiles block the page in WebGL; compiling first in a throwaway *Worker* fills the browser's shader cache, so the real compile then takes milliseconds
]

// ---------------------------------------------------------------------
// Part 4: making it fast

#slide[Where time goes][
  #two[
    - Measured by *ablation*: build variants with one feature switched off, and time them
    - Plus GPU *timestamp queries* around every kernel
    - At the start of the last round (22.4 ms a frame):
      - land march and its shading: ~11 ms
      - local march (wizard, glows): ~5 ms
      - shadows ~3, clouds ~2, AO ~0.5
  ][
    Biggest wins, roughly in order:
    - looser surface tolerance on distant terrain, matched to pixel size
    - bounding groups of parts (face, hat, beard)
    - over-relaxed sphere tracing (step 1.4× and back off on overshoot)
    - not re-evaluating buildings at every terrain step: a bound found earlier stays valid, minus the distance travelled
    - shadows from a simplified stand-in figure
  ]
]

#slide[Trading pixels for milliseconds][
  #two(ratio: (1fr, 1.1fr))[
    - A pixel at distance $t$ is $≈ 0.0008 t$ wide: a surface found within $0.0015 t$ is within two pixels' depth, so silhouettes don't move
    - Content trade-offs, measured and reversible: one fewer terrain octave, 16 cloud samples, a 150-unit horizon
    - Quality presets are separate builds; *Auto* watches the frame rate and swaps builds while the old one keeps drawing
  ][
    #align(center, img("epscmp.png", height: 11cm))
    #note[Hit tolerance 0.0004 t (top) vs 0.0015 t (bottom): 1.7 ms faster, visually the same.]
  ]
]

#slide[Shader size: the hidden cost][
  #two[
    - Inlining everything made a 2.4 MB shader; Chrome compiles each *pipeline* (kernel) at load, slowly, blocking
    - Naive `#[noinline]` made it *worse*: GPU functions get every record field as a parameter, and the pose has ~200
    - What worked: *project* each function's inputs into a small record; fewer kernels holding the shading; asynchronous pipeline creation
    - 2.4 → 1.6 MB; time to first frame about halved
  ][
    ```
    type sky_in = { horizon, zenith, moon,
                    moon_size, ..., secs }

    #[noinline]
    def sky_colour (sk: sky_in) (rd: vec3) = ...

    def sky pose rd = sky_colour (sky_in_of pose) rd
    ```
    #note[The wrapper keeps every call site unchanged; only the GPU function's signature shrinks.]
  ]
]

#slide[Anti-aliasing as a second pass][
  #img("aa-cmp.png", width: 100%)
  #two[
    - *FXAA*: look at each pixel's neighbours; where contrast suggests an edge, blend across it. Cheap (0.4 ms), slightly soft
  ][
    - *Edge supersampling*: find pixels whose hit distance jumps or whose contrast is high, compact them into a list (`filter`), and trace 4 more rays only for those. Sharp, ~11 ms
  ]
]

#slide[Benchmarking honestly on a laptop][
  #two[
    - An *A/A test* (two identical builds) shows the noise: frame time varies by ~0.25 ms between rounds
    - The laptop runs ~1 ms faster cold, drifts for a minute, then holds at its power limit. Fans at 100% changed nothing: power-bound, not heat-bound
    - So: warm up first, then interleave builds *A B B A* so drift hits each equally, and report a mean difference with a 95% confidence interval
  ][
    #table(columns: (1fr, 1fr), stroke: 0.5pt + dim, inset: 8pt,
      [*Difference to detect*], [*Rounds needed*],
      [0.5 ms], [~3],
      [0.25 ms], [~9],
      [0.1 ms], [~55])
    #note[95% confidence, 80% power, paired rounds of 12 frames at 1080p. Without this, "optimisations" of 0.2 ms are coin flips.]
  ]
]

#slide[Bugs found along the way][
  - *Bools passed to kernels were sometimes true when false*: the runtime copied 1 byte into a 4-byte slot of uninitialised memory. Fixed by patching `malloc` to `calloc` in the generated C
  - *Function names with a 'z'* broke GPU calls: the name escaping differed between definition and call site (fixed upstream)
  - *`x ** 2` with negative x* is NaN in WGSL: one shockwave ring turned the screen white
  - *Empty arrays* bound as too-small buffers; *Asyncify's 4 KB stack* overflowed on a mid-frame readback
  - *Chrome compiling pipelines on its GPU process's main thread*, freezing other canvases for seconds
  #v(0.4em)
  #note[Most were found by rendering on the CPU and the GPU and comparing, or by bisecting with build variants.]
]

#slide[Takeaways][
  - *Distance functions* make geometry, shadows, AO and collisions all fall out of one function
  - *Correct bounds* are the core of ray-marching performance: every shortcut must return a value that is never larger than the true distance
  - A *pure renderer* with state passed in as data is easy to test: byte-exact CPU references catch regressions
  - On the GPU, *code size* and *kernel count* matter as much as arithmetic
  - *Measure* before believing: ablations, timestamps, A/A tests, confidence intervals
  #v(1em)
  #note[Source: github.com/badly-drawn-wizards/wizard-vibes · Live: badly-drawn-wizards.github.io/wizard-vibes]
]
