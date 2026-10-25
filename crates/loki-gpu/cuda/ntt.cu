// SPDX-License-Identifier: Apache-2.0
//
// P4 resident NTT / exact coset transform for BLS12-381 Fr.
//
// Algorithm (clean-room, from the textbook definition): iterative radix-2
// Cooley-Tukey decimation in time over a bit-reversed load (Cooley & Tukey,
// "An algorithm for the machine calculation of complex Fourier series",
// Math. Comp. 19, 1965; the iterative form as in Cormen et al.,
// "Introduction to Algorithms", ch. 30). Stage s combines pairs
// (i0, i0 + 2^s) with twiddle w^(j * 2^(K-1-s)). The first
// min(K, SHARED_LOG) stages operate inside aligned 2^SHARED_LOG blocks and are
// fused in shared memory; later stages are one global pass each.
//
// Field arithmetic: sppark 0.1.16 `mont_t` (vendored, Apache-2.0) through
// `ntt_field.cuh`. Storage stays canonical little-endian; conversion to and
// from Montgomery form happens inside this launch (load / finish).
//
// Everything is enqueued on the caller's stream; nothing here synchronizes,
// allocates or frees. `src` is only read. Publication policy is the Rust
// runtime's (`GpuRuntime::launch_transform_fresh`).

#include <cuda_runtime.h>

#include <cstdint>
#include <cstring>

#include "include/loki_gpu_ffi.h"
#include "ntt_field.cuh"

namespace {

constexpr uint32_t kLimbs = 8;           // 32-bit limbs per element
constexpr uint32_t kElemBytes = 32;
constexpr uint32_t kConstRoot = 0;       // const slots [0, 32)
constexpr uint32_t kConstCoset = 32;     // const slots [32, 64)
constexpr uint32_t kConstScale = 64;     // const slot 64
constexpr uint32_t kConstUsed = 65;
constexpr uint32_t kSharedLog = 10;      // 1024 elements = 32 KiB shared
constexpr unsigned kThreads = 256;
constexpr uint32_t kKnownFlags =
    LOKI_GPU_NTT_FLAG_INPUT_BITREV | LOKI_GPU_NTT_FLAG_OUTPUT_BITREV |
    LOKI_GPU_NTT_FLAG_PRE_COSET | LOKI_GPU_NTT_FLAG_POST_COSET;

static_assert(LOKI_GPU_NTT_CONST_SLOTS >= kConstUsed, "const slots");

// r, little-endian bytes, for host-side canonical checks of the constants.
const uint8_t kModulusLe[32] = {
    0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xfe, 0x5b, 0xfe,
    0xff, 0x02, 0xa4, 0xbd, 0x53, 0x05, 0xd8, 0xa1, 0x09, 0x08, 0xd8,
    0x39, 0x33, 0x48, 0x7d, 0x9d, 0x29, 0x53, 0xa7, 0xed, 0x73};

bool canonical(const uint8_t *le) {
  for (int i = 31; i >= 0; --i) {
    if (le[i] != kModulusLe[i]) return le[i] < kModulusLe[i];
  }
  return false;  // equal to r
}

struct Constants {
  uint32_t v[kConstUsed][kLimbs];  // canonical little-endian limbs
};
static_assert(sizeof(Constants) == kConstUsed * kElemBytes, "constants");

__device__ __forceinline__ uint32_t bitrev(uint32_t x, uint32_t log) {
  return log == 0 ? 0u : (__brev(x) >> (32u - log));
}

#if defined(__CUDA_ARCH__)
// Product of table[j] over the set bits j of e (table in Montgomery form).
__device__ __forceinline__ fr_t pow_bits(const uint32_t *table, uint32_t e) {
  fr_t acc = fr_t::one();
  for (uint32_t j = 0; e != 0; ++j, e >>= 1) {
    if (e & 1u) acc *= fr_t(table + kLimbs * j);
  }
  return acc;
}

__device__ __forceinline__ void butterfly(fr_t &u, fr_t &v, const fr_t &w) {
  v *= w;
  fr_t t = u;
  u = t + v;
  v = t - v;
}
#endif

// Canonical constants -> Montgomery const slots.
__global__ void consts_kernel(uint32_t *out, Constants c) {
#if defined(__CUDA_ARCH__)
  uint32_t i = threadIdx.x;
  if (i < kConstUsed) fr_from_canonical(c.v[i]).store(out + kLimbs * i);
#endif
}

// tw[t] = w^t for t < half_n.
__global__ void twiddle_kernel(uint32_t *tw, const uint32_t *consts,
                               uint32_t half_n) {
#if defined(__CUDA_ARCH__)
  uint32_t t = blockIdx.x * blockDim.x + threadIdx.x;
  if (t >= half_n) return;
  pow_bits(consts + kLimbs * kConstRoot, t).store(tw + (size_t)kLimbs * t);
#endif
}

// work[q] = x[i] * (g^i if pre-coset), i = q (bit-reversed input) or
// bitrev(q); zero when i >= src_len (padding).
__global__ void load_kernel(uint32_t *work, const uint32_t *src,
                            const uint32_t *consts, uint32_t log_n, uint32_t n,
                            uint32_t src_len, uint32_t flags) {
#if defined(__CUDA_ARCH__)
  uint32_t q = blockIdx.x * blockDim.x + threadIdx.x;
  if (q >= n) return;
  uint32_t i = (flags & LOKI_GPU_NTT_FLAG_INPUT_BITREV) ? q : bitrev(q, log_n);
  uint32_t *out = work + (size_t)kLimbs * q;
  if (i >= src_len) {
    for (uint32_t l = 0; l < kLimbs; ++l) out[l] = 0;
    return;
  }
  fr_t x = fr_from_canonical(src + (size_t)kLimbs * i);
  if (flags & LOKI_GPU_NTT_FLAG_PRE_COSET)
    x *= pow_bits(consts + kLimbs * kConstCoset, i);
  x.store(out);
#endif
}

// Stages [0, stages) inside aligned blocks of 2^stages elements.
__global__ void shared_stages_kernel(uint32_t *a, const uint32_t *tw,
                                     uint32_t log_n, uint32_t stages) {
#if defined(__CUDA_ARCH__)
  extern __shared__ uint32_t sh[];
  const uint32_t block = 1u << stages;
  uint32_t *base = a + (size_t)kLimbs * ((size_t)blockIdx.x * block);
  for (uint32_t e = threadIdx.x; e < block * kLimbs; e += blockDim.x)
    sh[e] = base[e];
  __syncthreads();
  for (uint32_t s = 0; s < stages; ++s) {
    const uint32_t half = 1u << s;
    for (uint32_t t = threadIdx.x; t < block / 2; t += blockDim.x) {
      uint32_t j = t & (half - 1);
      uint32_t i0 = ((t >> s) << (s + 1)) | j;
      uint32_t i1 = i0 + half;
      fr_t w(tw + (size_t)kLimbs * ((size_t)j << (log_n - 1 - s)));
      fr_t u(sh + kLimbs * i0), v(sh + kLimbs * i1);
      butterfly(u, v, w);
      u.store(sh + kLimbs * i0);
      v.store(sh + kLimbs * i1);
    }
    __syncthreads();
  }
  for (uint32_t e = threadIdx.x; e < block * kLimbs; e += blockDim.x)
    base[e] = sh[e];
#endif
}

// One global radix-2 stage s.
__global__ void stage_kernel(uint32_t *a, const uint32_t *tw, uint32_t log_n,
                             uint32_t s) {
#if defined(__CUDA_ARCH__)
  uint32_t t = blockIdx.x * blockDim.x + threadIdx.x;
  if (t >= (1u << (log_n - 1))) return;
  const uint32_t half = 1u << s;
  uint32_t j = t & (half - 1);
  uint32_t i0 = ((t >> s) << (s + 1)) | j;
  uint32_t i1 = i0 + half;
  fr_t w(tw + (size_t)kLimbs * ((size_t)j << (log_n - 1 - s)));
  fr_t u(a + (size_t)kLimbs * i0), v(a + (size_t)kLimbs * i1);
  butterfly(u, v, w);
  u.store(a + (size_t)kLimbs * i0);
  v.store(a + (size_t)kLimbs * i1);
#endif
}

// dst[p] = canonical(work[p or bitrev(p)] * scale * (g^p if post-coset)),
// p < out_len. Safe in place (dst == work): natural order touches only slot
// p; bit-reversed output swaps each pair from one thread.
__global__ void finish_kernel(uint32_t *dst, const uint32_t *work,
                              const uint32_t *consts, uint32_t log_n,
                              uint32_t out_len, uint32_t flags) {
#if defined(__CUDA_ARCH__)
  uint32_t p = blockIdx.x * blockDim.x + threadIdx.x;
  if (p >= out_len) return;
  const fr_t scale(consts + kLimbs * kConstScale);
  if (flags & LOKI_GPU_NTT_FLAG_OUTPUT_BITREV) {
    uint32_t q = bitrev(p, log_n);
    if (p > q) return;
    fr_t x(work + (size_t)kLimbs * p), y(work + (size_t)kLimbs * q);
    fr_store_canonical(y * scale, dst + (size_t)kLimbs * p);
    if (p != q) fr_store_canonical(x * scale, dst + (size_t)kLimbs * q);
    return;
  }
  fr_t x(work + (size_t)kLimbs * p);
  x *= scale;
  if (flags & LOKI_GPU_NTT_FLAG_POST_COSET)
    x *= pow_bits(consts + kLimbs * kConstCoset, p);
  fr_store_canonical(x, dst + (size_t)kLimbs * p);
#endif
}

unsigned blocks_for(uint64_t threads) {
  return static_cast<unsigned>((threads + kThreads - 1) / kThreads);
}

// Validated parameters; 0 on malformed input.
uint64_t scratch_elems(const loki_gpu_ntt_params_v1 *p) {
  if (p == nullptr || p->ntt_log > LOKI_GPU_NTT_MAX_LOG) return 0;
  uint64_t n = 1ull << p->ntt_log;
  uint64_t work = p->output_len < n ? n : 0;
  return LOKI_GPU_NTT_CONST_SLOTS + n / 2 + work;
}

int32_t validate_params(const loki_gpu_ntt_params_v1 *p) {
  if (p == nullptr) return LOKI_GPU_INVALID_ARGUMENT;
  if (p->abi_version != LOKI_GPU_NTT_ABI_VERSION ||
      p->struct_size != sizeof(loki_gpu_ntt_params_v1))
    return LOKI_GPU_ABI_MISMATCH;
  if (p->ntt_log > LOKI_GPU_NTT_MAX_LOG) return LOKI_GPU_UNSUPPORTED;
  if ((p->flags & ~kKnownFlags) != 0) return LOKI_GPU_INVALID_ARGUMENT;
  const uint64_t n = 1ull << p->ntt_log;
  if (p->source_len == 0 || p->source_len > n || p->output_len == 0 ||
      p->output_len > n)
    return LOKI_GPU_INVALID_ARGUMENT;
  const bool pre = p->flags & LOKI_GPU_NTT_FLAG_PRE_COSET;
  const bool post = p->flags & LOKI_GPU_NTT_FLAG_POST_COSET;
  const bool in_rev = p->flags & LOKI_GPU_NTT_FLAG_INPUT_BITREV;
  const bool out_rev = p->flags & LOKI_GPU_NTT_FLAG_OUTPUT_BITREV;
  if (pre && post) return LOKI_GPU_INVALID_ARGUMENT;
  if ((pre || post) && (in_rev || out_rev)) return LOKI_GPU_INVALID_ARGUMENT;
  // Padding only for a forward coset; truncation only for an inverse coset.
  if (p->source_len < n && !pre) return LOKI_GPU_INVALID_ARGUMENT;
  if (p->output_len < n && !post) return LOKI_GPU_INVALID_ARGUMENT;
  if ((p->source_len & (p->source_len - 1)) != 0 ||
      (p->output_len & (p->output_len - 1)) != 0)
    return LOKI_GPU_INVALID_ARGUMENT;
  for (uint32_t j = 0; j < 32; ++j) {
    if (!canonical(p->root_powers[j]) || !canonical(p->coset_powers[j]))
      return LOKI_GPU_INVALID_ARGUMENT;
  }
  if (!canonical(p->scale)) return LOKI_GPU_INVALID_ARGUMENT;
  return LOKI_GPU_OK;
}

template <typename T>
T *raw_ptr(const loki_gpu_handle_v1 *h) {
  return reinterpret_cast<T *>(static_cast<uintptr_t>(h->raw));
}

int32_t report_plain(loki_gpu_status_v1 *status, int32_t code) {
  if (status != nullptr) {
    status->code = code;
    status->cuda_error = 0;
    status->sticky = 0;
    status->reserved = 0;
  }
  return code;
}

}  // namespace

extern "C" uint64_t loki_gpu_ntt_scratch_bytes_v1(
    const loki_gpu_ntt_params_v1 *params) {
  if (validate_params(params) != LOKI_GPU_OK) return 0;
  return scratch_elems(params) * kElemBytes;
}

extern "C" int32_t loki_gpu_ntt_launch_v1(const loki_gpu_handle_v1 *dst,
                                          const loki_gpu_handle_v1 *src,
                                          const loki_gpu_handle_v1 *scratch,
                                          const loki_gpu_ntt_params_v1 *params,
                                          const loki_gpu_handle_v1 *stream,
                                          loki_gpu_status_v1 *status) {
  // ---- Validate everything before enqueueing anything.
  int32_t c = validate_params(params);
  if (c != LOKI_GPU_OK) return report_plain(status, c);
  c = loki_gpu_rt_check_handle_v1(dst, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK)
    c = loki_gpu_rt_check_handle_v1(src, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK)
    c = loki_gpu_rt_check_handle_v1(scratch, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK)
    c = loki_gpu_rt_check_handle_v1(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report_plain(status, c);
  const uint32_t log_n = params->ntt_log;
  const uint64_t n = 1ull << log_n;
  if (dst->bytes != params->output_len * kElemBytes ||
      src->bytes != params->source_len * kElemBytes ||
      scratch->bytes != scratch_elems(params) * kElemBytes)
    return report_plain(status, LOKI_GPU_INVALID_ARGUMENT);
  if (dst->raw == src->raw || dst->raw == scratch->raw ||
      src->raw == scratch->raw)
    return report_plain(status, LOKI_GPU_INVALID_ARGUMENT);
  // The CUDA current device is per host thread: re-select it before any
  // device work (the runtime's entry-point convention).
  if (int32_t d = loki_gpu_rt_ensure_device_v1(status)) return d;

  Constants consts_host;
  std::memcpy(consts_host.v[kConstRoot], params->root_powers, 32 * kElemBytes);
  std::memcpy(consts_host.v[kConstCoset], params->coset_powers,
              32 * kElemBytes);
  std::memcpy(consts_host.v[kConstScale], params->scale, kElemBytes);

  cudaStream_t s = raw_ptr<CUstream_st>(stream);
  uint32_t *d_dst = raw_ptr<uint32_t>(dst);
  const uint32_t *d_src = raw_ptr<const uint32_t>(src);
  uint32_t *d_consts = raw_ptr<uint32_t>(scratch);
  uint32_t *d_tw = d_consts + (size_t)kLimbs * LOKI_GPU_NTT_CONST_SLOTS;
  uint32_t *d_work = params->output_len < n ? d_tw + (size_t)kLimbs * (n / 2)
                                            : d_dst;
  const uint32_t flags = params->flags;
  const uint32_t src_len = static_cast<uint32_t>(params->source_len);
  const uint32_t out_len = static_cast<uint32_t>(params->output_len);

  // ---- Enqueue. A launch error is reported after earlier kernels may have
  // been queued; the runtime drains the stream before freeing anything.
#define LOKI_NTT_LAUNCHED()                                       \
  do {                                                            \
    cudaError_t e_ = cudaGetLastError();                          \
    if (e_ != cudaSuccess)                                        \
      return loki_gpu_rt_report_cuda_v1(static_cast<int32_t>(e_), \
                                        status);                  \
  } while (0)

  consts_kernel<<<1, 96, 0, s>>>(d_consts, consts_host);
  LOKI_NTT_LAUNCHED();
  if (log_n > 0) {
    twiddle_kernel<<<blocks_for(n / 2), kThreads, 0, s>>>(
        d_tw, d_consts, static_cast<uint32_t>(n / 2));
    LOKI_NTT_LAUNCHED();
  }
  load_kernel<<<blocks_for(n), kThreads, 0, s>>>(
      d_work, d_src, d_consts, log_n, static_cast<uint32_t>(n), src_len,
      flags);
  LOKI_NTT_LAUNCHED();
  if (log_n > 0) {
    const uint32_t fused = log_n < kSharedLog ? log_n : kSharedLog;
    const uint32_t block = 1u << fused;
    const unsigned threads = block / 2 < 512 ? block / 2 : 512;
    shared_stages_kernel<<<static_cast<unsigned>(n >> fused), threads,
                           block * kElemBytes, s>>>(d_work, d_tw, log_n, fused);
    LOKI_NTT_LAUNCHED();
    for (uint32_t stage = fused; stage < log_n; ++stage) {
      stage_kernel<<<blocks_for(n / 2), kThreads, 0, s>>>(d_work, d_tw, log_n,
                                                          stage);
      LOKI_NTT_LAUNCHED();
    }
  }
  finish_kernel<<<blocks_for(out_len), kThreads, 0, s>>>(
      d_dst, d_work, d_consts, log_n, out_len, flags);
  LOKI_NTT_LAUNCHED();
#undef LOKI_NTT_LAUNCHED
  return report_plain(status, LOKI_GPU_OK);
}

/*
 * Legacy synchronous v1 byte-wire stub. Superseded by the resident launch
 * above; it still returns UNSUPPORTED without touching `out`.
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
