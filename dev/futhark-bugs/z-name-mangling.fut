-- Minimal reproducer: a #[noinline] GPU function whose name contains a z.
-- `futhark webgpu` defines it as futrts_wizard_N but calls it as
-- futrts_wizzard_N (z escaped as zz at the call site only).
#[noinline]
def wizard (x: f32) : f32 = x * x + 1

#[noinline]
def outerfn (x: f32) : f32 = wizard x + wizard (x + 1)

entry main (xs: []f32) : []f32 = map outerfn xs

-- To check (Futhark 0.29.0 prerelease, webgpu branch 14e9ccbf):
--   futhark webgpu --library -o z z-name-mangling.fut
--   grep -o 'futrts_wiz[a-z]*_[0-9]*' z.c | sort | uniq -c
-- shows `fn futrts_wizard_N` defined but `futrts_wizzard_N(` called from
-- outerfn, so the browser rejects the shader ("unresolved call target").
-- Renaming `wizard` to anything without a 'z' makes it work.
