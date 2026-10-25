#include "include/loki_gpu_ffi.h"

/*
 * Clean, compileable placeholder for the NTT entry point. The Rust operation
 * module currently returns Unsupported before reaching FFI; keeping this
 * export makes the optional CUDA crate layout link-complete without claiming
 * an implemented transform. In particular, UNSUPPORTED leaves `out` untouched.
 */
extern "C" int32_t loki_gpu_ntt_v1(const uint8_t *input, size_t input_bytes,
                                  uint8_t *out, size_t out_bytes,
                                  const uint8_t *params, size_t params_bytes) {
  (void)input;
  (void)input_bytes;
  (void)out;
  (void)out_bytes;
  (void)params;
  (void)params_bytes;
  return LOKI_GPU_UNSUPPORTED;
}
