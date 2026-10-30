// Appended inside the emscripten module closure (--post-js) so the harness
// can turn a WGPUBuffer handle (wasm pointer) into the JS GPUBuffer.
Module["wgpuGetJsObject"] = (ptr) => WebGPU.getJsObject(ptr);
