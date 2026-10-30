// (inserted by dev/emcc/emcc)
// Compute pipelines are created asynchronously, waiting the way the runtime
// waits for other WebGPU futures (emscripten_sleep, i.e. back to the event
// loop): Chromium compiles a pipeline made by the synchronous call on its
// GPU process's main thread, stalling every frame on the page meanwhile,
// including a build that is still drawing while another loads. Should the
// asynchronous creation fail, the synchronous call is made, so the error is
// reported as before.
typedef struct wgpu_pipeline_wait {
  bool released;
  WGPUComputePipeline pipeline;
} wgpu_pipeline_wait;

static void wgpu_pipeline_callback(WGPUCreatePipelineAsyncStatus status, WGPUComputePipeline pipeline,
                                   WGPUStringView message, void *userdata1, void *userdata2) {
  (void)message;
  (void)userdata2;
  wgpu_pipeline_wait *w = (wgpu_pipeline_wait *)userdata1;
  w->pipeline = status == WGPUCreatePipelineAsyncStatus_Success ? pipeline : NULL;
  w->released = true;
}

static WGPUComputePipeline wgpu_create_compute_pipeline_waiting(WGPUDevice device, const WGPUComputePipelineDescriptor *desc) {
  wgpu_pipeline_wait w = { .released = false, .pipeline = NULL };
  WGPUCreateComputePipelineAsyncCallbackInfo cb = {
    .mode = WGPUCallbackMode_AllowSpontaneous,
    .callback = wgpu_pipeline_callback,
    .userdata1 = (void *)&w,
    .userdata2 = NULL,
  };
  wgpuDeviceCreateComputePipelineAsync(device, desc, cb);
  while (!w.released) {
    emscripten_sleep(0);
  }
  return w.pipeline ? w.pipeline : wgpuDeviceCreateComputePipeline(device, desc);
}

