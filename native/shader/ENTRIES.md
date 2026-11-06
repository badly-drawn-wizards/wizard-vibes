# The shader crate's entry points and host API

All GPU entry points are compute shaders with workgroup size 256 (x), in one
SPIR-V module (naga's WGSL of it for the web), sharing one descriptor set
(group 0) whose bindings each have a fixed number, type and access mode in
every entry point, so a single bind group layout serves all of them:

| binding | WGSL type                 | access     | holds |
|---------|---------------------------|------------|-------|
| 0       | uniform `Uniforms`        | uniform    | the generated module's block (params `p_*`, drive `d_*`, args `a_*`) |
| 1       | `array<u32>`              | read_write | out: the frame's pixels, BGRA8 packed as native_main packs them; `a_w * a_h`, rows `a_w` long, columns `>= a_vw` 0xff000000 |
| 2       | `array<vec4<f32>>`        | read_write | colour: per image pixel (`a_vw * a_h`, pixel (x, y) at `y * a_vw + x`) its finished colour (rgb, after tone mapping) and in w the distance its ray sees (inf: the sky) |
| 3       | `array<f32>`              | read       | spells: `spell_stride` (10) floats per slot (fut/spells.fut's layout), `a_n_spells` slots used (at most `max_spells`; clamped to the buffer's length) |
| 4       | `array<u32>`              | read_write | edges: `[0]` how many edge pixels edge_mark found, then their indices into colour; `1 + a_vw * a_h` u32s for the worst case (fewer is fine, see capacity) |
| 5       | `array<vec4<f32>>`        | read_write | rays: one colour per extra ray, 4 per edge pixel (w unused) |
| 6       | `array<u32>`              | read_write | args: 6 u32s, indirect dispatch sizes: `[0..3]` edge_rays, `[3..6]` edge_resolve (usage STORAGE \| INDIRECT) |
| 7       | `array<f32>`              | read_write | music: 3 floats a frame: the mix with the soundboard (left, right, before the limit) and the room's send; the host finishes it (music::finish_chunk: the room, then the limit) |
| 8       | `array<f32>`              | read_write | dry: 4 floats a frame: the mix's left, right (before the limit), the harpsichord alone, the room's send |
| 9       | `array<f32>`              | read_write | modes: the soundboard's modes' outputs, `modes * a_m_n` (mode m's at `m * a_m_n`) |
| 10      | `array<f32>`              | read_write | the soundboard's state: two slots of `modes * 2` floats, kept from chunk to chunk |

Bindings must stay contiguous: the SPIR-V run as it is (wgpu's
passthrough) silently did nothing in entry points using bindings past a
gap (10-12 with 8 and 9 unused).

Every storage binding but 3 is read_write in the layout
(`Storage { read_only: false }`) even where an entry point only reads it;
3 is `Storage { read_only: true }`. Bind all eight in every bind group (an
entry point uses only some; small dummies are fine for the rest, e.g. for
the music when no frame buffers exist yet). Binding 4 is atomic in edge_mark
(naga makes it `array<atomic<u32>>` there; it is the same buffer).

## Entry points

| entry point    | threads (x)                | reads                    | writes |
|----------------|----------------------------|--------------------------|--------|
| `native_main`  | `a_w * a_h`                | 0                        | 1 (the old client's: no spells, no AA; unchanged) |
| `shade`        | `a_w * a_h`                | 0, 3                     | 2; 1 if `p_aa` is not 1 or 2 (pixels and padding); `edges[0] = 0` (4) if `p_aa == 2` |
| `fxaa`         | `a_w * a_h`                | 0, 2                     | 1 (FXAA of colour, packed, and padding) |
| `edge_mark`    | `a_vw * a_h`               | 0, 2, 4, 5 (lengths)     | 4 (atomic count, edge pixel indices) |
| `edge_args`    | 1 (one workgroup)          | 4, 5                     | 6 |
| `edge_rays`    | `4 * edges` (indirect `args[0..3]`) | 0, 3, 4         | 5 |
| `edge_resolve` | `edges` (indirect `args[3..6]`)     | 2, 4, 5         | 2 (edge pixels' colour: the mean of their 5 samples) |
| `pack_pixels`  | `a_w * a_h`                | 0, 2                     | 1 (colour packed, and padding) |
| `music0` .. `music4` | `a_m_n`              | 0                        | 8 |
| `soundboard`   | `modes` (64)               | 0, 8, 10 (slot (a_m_chunk - 1) % 2; none for chunk 0) | 9, 10 (slot a_m_chunk % 2) |
| `music_mix`    | `a_m_n`                    | 0, 8, 9                  | 7 |

Dispatch `ceil(threads / 256)` workgroups; every entry point ignores
threads beyond its count, so rounding up (or dispatching more) is safe.

Uniform args used: `a_vw` (image width), `a_h`, `a_w` (padded row length,
a multiple of 64), `a_tm`, `a_yaw`, `a_pitch`, `a_dst` (camera, as
native_main), `a_n_spells`; `a_m_t0` (loop time of the first sample,
wrapped into [0, period) on the host in double precision), `a_m_rate`,
`a_m_n`; params `p_aa` (0 off, 1 FXAA, 2 edge supersampling) and
`p_aa_threshold`.

## Order (one frame; all in one compute pass is fine: wgpu orders dispatches)

- `p_aa == 0` (or anything but 1, 2): `shade`. Done: out is the frame.
- `p_aa == 1`: `shade`, `fxaa`.
- `p_aa == 2`: `shade`, `edge_mark`, `edge_args`, `edge_rays`,
  `edge_resolve`, `pack_pixels`.
  - edge_rays and edge_resolve can be dispatched indirectly from args
    (offsets 0 and 12 bytes), or directly for the most edge pixels the
    buffers hold (`4 * cap` and `cap` threads; they read the count from
    edges[0] and stop there). Indirect: wgpu forbids a buffer bound as
    storage in the current bind group from being the indirect buffer, so set
    a second bind group (same layout, binding 6 a small dummy buffer) before
    the two indirect dispatches.
  - Capacity: the edge pixels handled are at most
    `cap = min(len(edges) - 1, len(rays) / 4)`. At 1080p the worst case
    (every pixel an edge) needs 133 MB of rays; a smaller rays buffer is
    fine: edge pixels found beyond its capacity are left with their one
    sample (which ones is arbitrary: edge_mark appends in no fixed order).
    Typical frames have 1-5% edge pixels.
- Music (independent of the frame), a chunk at a time and in order
  (`a_m_chunk` counting them from 0: the soundboard's state carries over):
  `music<song>` (the song's own kernel), `soundboard`, `music_mix`, then read back binding 7 and finish
  it on the host with `music::finish_chunk` (the room reverb, room.rs, whose
  state carries over there; sequential through its delay lines, it took 30
  ms a half-second chunk as GPU threads).

## Host API (the same crate, built as a normal Rust dependency)

- Collision probe (collide.rs, fut/collide.fut):
  `collide::probe(prm: &Params, dr: &Drive, secs: f32, pts: &[f32], out: &mut [f32])`
  writes `collide::probe_len(pts.len() / 3)` = `4 * (6 + pts.len() / 3)`
  floats: per sphere of `collision_spheres(prm.platform)`, then per point
  (x, y, z in pts) as from a sphere of radius 0, `[distance from its
  surface to the land (negative: inside, at most 1e6), normal x, y, z]`, as
  probe_frame. Also `land_probe(prm, w) -> (f32, Vec3)` and
  `probe_one(prm, w, r) -> [f32; 4]`.
- Music (music.rs, fut/music.fut; not in the prelude):
  `music::period` (f32, the loop's length: 64 s), `music::music_at(rate, t)
  -> (f32, f32)`, `music::sound_value(t0, rate, j)` (value j of a chunk) and,
  host only, `music::sound_chunk(t0, rate, n) -> Vec<f32>`.
- Sound effects (sfx.rs, fut/sfx.fut; host only, not in the SPIR-V):
  `sfx::sfx_count` (i32, 14), `sfx::sfx_length(k) -> f32` (seconds),
  `sfx::sfx_samples(k, rate, n: usize) -> Vec<f32>` (n stereo samples,
  interleaved; the app asks for `ceil(sfx_length(k) * rate)`). About
  5-75 ms each at 48 kHz in a release build.
- Spells: `spell_stride` (10), the kinds `spell_fire` .. `spell_mark`
  (spells.rs), `max_spells` (the generated module, from params.json).
