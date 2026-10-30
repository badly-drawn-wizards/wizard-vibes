// Whether this is the static build (dev/build-static.ts), which esbuild
// compiles with STATIC defined as true: no dev server behind it, so no hot
// reload, and the compile-time parameters are fixed at their defaults.
declare const STATIC: boolean;
export const IS_STATIC: boolean = typeof STATIC !== "undefined" && STATIC;
