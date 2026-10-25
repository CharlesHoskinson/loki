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
#define LOKI_GPU_RUNTIME_ERROR ((int32_t)3)  /* non-sticky CUDA failure */
#define LOKI_GPU_OUT_OF_MEMORY ((int32_t)4)  /* non-sticky allocation failure */
#define LOKI_GPU_NOT_READY ((int32_t)5)      /* event/stream still pending */
#define LOKI_GPU_STALE_HANDLE ((int32_t)6)   /* handle from an older context epoch */
#define LOKI_GPU_CONTEXT_LOST ((int32_t)7)   /* sticky error: context unusable */
#define LOKI_GPU_ABI_MISMATCH ((int32_t)8)   /* handle/struct ABI version differs */
#define LOKI_GPU_NO_DEVICE ((int32_t)9)      /* no usable device or driver */

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

/* ------------------------------------------------------------------------
 * Versioned runtime ABI (P2). Every struct is fixed-width with explicit
 * padding; sizes and offsets are reported by loki_gpu_rt_abi_layout_v1 and
 * checked on both sides. Handles are opaque to Rust: `raw` holds driver bits
 * and is only interpreted by this translation unit, after the handle's
 * abi_version, kind and context_epoch have been validated. A handle minted
 * before loki_gpu_rt_reset_v1 fails with LOKI_GPU_STALE_HANDLE.
 * ------------------------------------------------------------------------ */

#define LOKI_GPU_RT_ABI_VERSION ((uint32_t)1)

#define LOKI_GPU_HANDLE_DEVICE_MEMORY ((uint32_t)1)
#define LOKI_GPU_HANDLE_HOST_PINNED ((uint32_t)2)
#define LOKI_GPU_HANDLE_HOST_PAGEABLE ((uint32_t)3)
#define LOKI_GPU_HANDLE_STREAM ((uint32_t)4)
#define LOKI_GPU_HANDLE_EVENT ((uint32_t)5)

typedef struct loki_gpu_handle_v1 {
  uint32_t abi_version;   /* LOKI_GPU_RT_ABI_VERSION */
  uint32_t kind;          /* LOKI_GPU_HANDLE_* */
  uint64_t context_epoch; /* native context generation that minted it */
  uint64_t raw;           /* opaque driver bits; never interpreted by Rust */
  uint64_t bytes;         /* extent for memory kinds, 0 otherwise */
} loki_gpu_handle_v1;

/* Explicit status detail. `code` repeats the returned status. */
typedef struct loki_gpu_status_v1 {
  int32_t code;
  int32_t cuda_error; /* raw cudaError_t, 0 if none */
  uint32_t sticky;    /* 1 when the context must be treated as poisoned */
  uint32_t reserved;  /* zero */
} loki_gpu_status_v1;

typedef struct loki_gpu_device_info_v1 {
  uint32_t abi_version;
  int32_t ordinal;
  int32_t cc_major;
  int32_t cc_minor;
  uint64_t total_bytes;
  uint64_t free_bytes;
  uint64_t context_epoch;
  int32_t runtime_version;
  int32_t driver_version;
  int32_t multiprocessors;
  int32_t async_engines;
  char name[64]; /* NUL-terminated */
} loki_gpu_device_info_v1;

typedef struct loki_gpu_abi_layout_v1 {
  uint32_t abi_version;
  uint32_t handle_size;
  uint32_t handle_align;
  uint32_t handle_offset_context_epoch;
  uint32_t handle_offset_raw;
  uint32_t handle_offset_bytes;
  uint32_t status_size;
  uint32_t status_offset_sticky;
  uint32_t device_info_size;
  uint32_t device_info_offset_total_bytes;
  uint32_t device_info_offset_context_epoch;
  uint32_t device_info_offset_name;
  uint32_t layout_size;
  uint32_t reserved; /* zero */
} loki_gpu_abi_layout_v1;

#if defined(__cplusplus)
static_assert(sizeof(loki_gpu_handle_v1) == 32, "handle ABI");
static_assert(sizeof(loki_gpu_status_v1) == 16, "status ABI");
static_assert(sizeof(loki_gpu_device_info_v1) == 120, "device info ABI");
static_assert(sizeof(loki_gpu_abi_layout_v1) == 56, "layout ABI");
#endif

/* All functions below validate every pointer and handle before touching
 * memory, return a LOKI_GPU_* status, and fill `status` when non-null.
 * Copies and launches are asynchronous on the given non-blocking stream;
 * callers own the source/destination lifetimes until an event recorded
 * after the operation has been synchronized. */
uint32_t loki_gpu_rt_abi_version(void);
int32_t loki_gpu_rt_abi_layout_v1(loki_gpu_abi_layout_v1 *out);
int32_t loki_gpu_rt_probe_v1(int32_t ordinal, loki_gpu_device_info_v1 *out,
                             loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_reset_v1(int32_t ordinal, loki_gpu_device_info_v1 *out,
                             loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_stream_create_v1(loki_gpu_handle_v1 *out,
                                     loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_stream_destroy_v1(const loki_gpu_handle_v1 *stream,
                                      loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_event_create_v1(uint32_t timing, loki_gpu_handle_v1 *out,
                                    loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_event_destroy_v1(const loki_gpu_handle_v1 *event,
                                     loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_device_alloc_v1(uint64_t bytes, loki_gpu_handle_v1 *out,
                                    loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_device_free_v1(const loki_gpu_handle_v1 *memory,
                                   loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_host_alloc_v1(uint64_t bytes, uint32_t pinned,
                                  loki_gpu_handle_v1 *out,
                                  loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_host_free_v1(const loki_gpu_handle_v1 *buffer,
                                 loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_memcpy_h2d_v1(const loki_gpu_handle_v1 *dst,
                                  uint64_t dst_offset,
                                  const loki_gpu_handle_v1 *src_host,
                                  uint64_t src_offset, uint64_t len,
                                  const loki_gpu_handle_v1 *stream,
                                  loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_memcpy_d2h_v1(const loki_gpu_handle_v1 *dst_host,
                                  uint64_t dst_offset,
                                  const loki_gpu_handle_v1 *src,
                                  uint64_t src_offset, uint64_t len,
                                  const loki_gpu_handle_v1 *stream,
                                  loki_gpu_status_v1 *status);
/* Runtime self-test kernel: dst[0..bytes) = src[0..bytes). */
int32_t loki_gpu_rt_launch_copy_v1(const loki_gpu_handle_v1 *dst,
                                   const loki_gpu_handle_v1 *src,
                                   uint64_t bytes,
                                   const loki_gpu_handle_v1 *stream,
                                   loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_event_record_v1(const loki_gpu_handle_v1 *event,
                                    const loki_gpu_handle_v1 *stream,
                                    loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_stream_wait_event_v1(const loki_gpu_handle_v1 *stream,
                                         const loki_gpu_handle_v1 *event,
                                         loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_event_synchronize_v1(const loki_gpu_handle_v1 *event,
                                         loki_gpu_status_v1 *status);
/* Returns LOKI_GPU_OK when complete, LOKI_GPU_NOT_READY when pending. */
int32_t loki_gpu_rt_event_query_v1(const loki_gpu_handle_v1 *event,
                                   loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_event_elapsed_ms_v1(const loki_gpu_handle_v1 *start,
                                        const loki_gpu_handle_v1 *end,
                                        float *ms, loki_gpu_status_v1 *status);
int32_t loki_gpu_rt_stream_synchronize_v1(const loki_gpu_handle_v1 *stream,
                                          loki_gpu_status_v1 *status);

/* ------------------------------------------------------------------------
 * P4 resident transform ABI. One launch runs the whole pipeline described in
 * `loki-gpu/src/runtime.rs` (`TransformLaunch`) asynchronously on `stream`:
 * constants -> twiddles -> load (canonical->Montgomery, optional bit
 * reversal, coset powers, zero padding) -> radix-2 DIT stages -> finish
 * (scale, inverse coset powers, optional bit reversal, Montgomery->fully
 * reduced canonical). `src` is only read. Scalars and constants are 32-byte
 * canonical little-endian values below r. The scratch layout is
 * [LOKI_GPU_NTT_CONST_SLOTS consts][N/2 twiddles][N work, only when
 * output_len < N]; it must be exactly that size (scratch may be NULL only
 * when the size is zero, i.e. never in v1 since consts are always used).
 * ------------------------------------------------------------------------ */

#define LOKI_GPU_NTT_ABI_VERSION ((uint32_t)1)
#define LOKI_GPU_NTT_MAX_LOG ((uint32_t)27)
#define LOKI_GPU_NTT_CONST_SLOTS ((uint32_t)72)
#define LOKI_GPU_NTT_FLAG_INPUT_BITREV ((uint32_t)1)
#define LOKI_GPU_NTT_FLAG_OUTPUT_BITREV ((uint32_t)2)
#define LOKI_GPU_NTT_FLAG_PRE_COSET ((uint32_t)4)
#define LOKI_GPU_NTT_FLAG_POST_COSET ((uint32_t)8)

typedef struct loki_gpu_ntt_params_v1 {
  uint32_t abi_version;          /* LOKI_GPU_NTT_ABI_VERSION */
  uint32_t struct_size;          /* sizeof(loki_gpu_ntt_params_v1) */
  uint32_t ntt_log;              /* N = 2^ntt_log, <= LOKI_GPU_NTT_MAX_LOG */
  uint32_t flags;                /* LOKI_GPU_NTT_FLAG_* */
  uint64_t source_len;           /* elements read from src, 1..=N */
  uint64_t output_len;           /* elements written to dst, 1..=N */
  uint8_t root_powers[32][32];   /* w^(2^j), j < ntt_log; w already inverted
                                    for inverse transforms */
  uint8_t coset_powers[32][32];  /* g^(2^j), j < ntt_log, when a coset flag
                                    is set */
  uint8_t scale[32];             /* output multiplier (N^-1 or 1) */
} loki_gpu_ntt_params_v1;

#if defined(__cplusplus)
static_assert(sizeof(loki_gpu_ntt_params_v1) == 2112, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, abi_version) == 0, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, struct_size) == 4, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, ntt_log) == 8, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, flags) == 12, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, source_len) == 16, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, output_len) == 24, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, root_powers) == 32, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, coset_powers) == 1056, "ntt params ABI");
static_assert(offsetof(loki_gpu_ntt_params_v1, scale) == 2080, "ntt params ABI");
static_assert(alignof(loki_gpu_ntt_params_v1) == 8, "ntt params ABI");
#endif

/* Scratch bytes required by `params` (0 on malformed params). */
uint64_t loki_gpu_ntt_scratch_bytes_v1(const loki_gpu_ntt_params_v1 *params);

int32_t loki_gpu_ntt_launch_v1(const loki_gpu_handle_v1 *dst,
                               const loki_gpu_handle_v1 *src,
                               const loki_gpu_handle_v1 *scratch,
                               const loki_gpu_ntt_params_v1 *params,
                               const loki_gpu_handle_v1 *stream,
                               loki_gpu_status_v1 *status);

/* Shared validation for other translation units (P4): the runtime's own
 * handle check (version, kind, nonzero, current epoch) and CUDA error
 * classification/report. Neither dereferences a handle's raw bits. */
int32_t loki_gpu_rt_check_handle_v1(const loki_gpu_handle_v1 *handle,
                                    uint32_t kind);
int32_t loki_gpu_rt_report_cuda_v1(int32_t cuda_error,
                                   loki_gpu_status_v1 *status);
/* Make the probed device current on the calling host thread; returns
 * LOKI_GPU_OK or an already reported failure. */
int32_t loki_gpu_rt_ensure_device_v1(loki_gpu_status_v1 *status);

#ifdef __cplusplus
}
#endif

#endif /* LOKI_GPU_FFI_H */

