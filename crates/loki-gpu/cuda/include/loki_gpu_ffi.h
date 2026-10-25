#ifndef LOKI_GPU_FFI_H
#define LOKI_GPU_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Stable integer status codes shared with Rust. Never use a C enum in the ABI. */
#define LOKI_GPU_OK ((int32_t)0)
#define LOKI_GPU_UNSUPPORTED ((int32_t)1)
#define LOKI_GPU_INVALID_ARGUMENT ((int32_t)2)
#define LOKI_GPU_RUNTIME_ERROR ((int32_t)3)

/*
 * Synchronous v1 wire ABI for the compileable stubs. Inputs and outputs are
 * byte arrays whose exact encodings are defined by their Rust operation
 * modules, not Rust/C++ enum or struct memory layouts. In particular, a future
 * quotient program encoding must use a versioned fixed-width opcode/operand
 * format and validate lengths and tags before parsing. A status of UNSUPPORTED
 * guarantees that output memory is untouched.
 * Production resident and asynchronous calls require a separate handle/stream
 * ABI; these functions do not claim to provide residency or overlap.
 */
int32_t loki_gpu_msm_v1(const uint8_t *scalars, size_t scalar_bytes,
                        const uint8_t *bases, size_t base_bytes,
                        uint8_t *out, size_t out_bytes);
int32_t loki_gpu_ntt_v1(const uint8_t *input, size_t input_bytes,
                        uint8_t *out, size_t out_bytes,
                        const uint8_t *params, size_t params_bytes);
int32_t loki_gpu_quotient_v1(const uint8_t *input, size_t input_bytes,
                             uint8_t *out, size_t out_bytes,
                             const uint8_t *program, size_t program_bytes);

#ifdef __cplusplus
}
#endif

#endif /* LOKI_GPU_FFI_H */

