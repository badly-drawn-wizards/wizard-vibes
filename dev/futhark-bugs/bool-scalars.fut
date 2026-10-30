-- Minimal reproducer: a bool computed on the host and passed to a kernel.
-- The webgpu runtime's gpu_launch_kernel mallocs the block of scalar
-- arguments without clearing it and copies the bool in as one byte, but WGSL
-- reads the slot as an i32, so the other three bytes are heap garbage: a
-- false can arrive as true. Depends on what the heap held, so the result
-- varies; it showed reliably in a kernel with three such flags.
entry main (flags: i32) (xs: []f32) : []f32 =
  let on = flags & 1 != 0 -- hoisted to the host, passed as a bool
  in map (\x -> if on then x else -x) xs

-- To check (webgpu branch 14e9ccbf): in the generated C,
--   grep -n 'void \*scalars = malloc' z.c
-- and the kernel wrapper passes `bool argN` with sizeof(argN) == 1.
--
-- Workaround: dev/emcc/emcc patches that malloc to calloc before compiling.
