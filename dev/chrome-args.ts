// Chromium flags for headless WebGPU on Linux. Without --ignore-gpu-blocklist
// and Vulkan ANGLE, Chromium may fall back to a software adapter that lacks
// shader-f16, which Futhark's WebGPU backend needs.
export const CHROME_ARGS = ["--enable-unsafe-webgpu", "--enable-features=Vulkan,WebGPU", "--ignore-gpu-blocklist", "--use-angle=vulkan"];
