// V1 quotient numerator ABI skeleton. The ordered six-op bytecode interpreter
// is specified by the backend API but is not implemented in this translation
// unit. Unsupported must leave the output untouched and inspect no pointers.
#include "include/loki_gpu_ffi.h"

extern "C" int32_t loki_gpu_quotient_v1(const uint8_t *input,
                                        size_t input_bytes,
                                        uint8_t *out, size_t out_bytes,
                                        const uint8_t *program,
                                        size_t program_bytes) {
  (void)input;
  (void)input_bytes;
  (void)out;
  (void)out_bytes;
  (void)program;
  (void)program_bytes;
  return LOKI_GPU_UNSUPPORTED;
}
