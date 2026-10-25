// V1 MSM ABI skeleton. The production segmented-bucket implementation is
// specified in design/MSM.md and is not implemented in this translation unit.
#include "include/loki_gpu_ffi.h"

extern "C" int32_t loki_gpu_msm_v1(const uint8_t *scalars,
                                   size_t scalar_bytes,
                                   const uint8_t *bases,
                                   size_t base_bytes,
                                   uint8_t *out,
                                   size_t out_bytes) {
  // Unsupported must be side-effect free. Do not inspect any caller pointer
  // or publish a partial result from the skeleton.
  (void)scalars;
  (void)scalar_bytes;
  (void)bases;
  (void)base_bytes;
  (void)out;
  (void)out_bytes;
  return LOKI_GPU_UNSUPPORTED;
}
