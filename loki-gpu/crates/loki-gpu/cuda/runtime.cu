// SPDX-License-Identifier: Apache-2.0
//
// P2 native runtime ABI: device probe/reset, non-blocking streams, events,
// device and host (pinned or pageable) allocations, asynchronous copies and a
// byte-copy self-test kernel. Written from the CUDA Runtime API reference;
// no third-party source is copied.
//
// Every entry point validates its pointers and handles before touching any
// memory, classifies CUDA errors into transient versus sticky (context-
// poisoning) statuses, and never frees or synchronizes on behalf of the
// caller. Lifetime and publication policy live in the Rust coordinator.

#include <cuda_runtime.h>

#include <atomic>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <limits>

#include "include/loki_gpu_ffi.h"

namespace {

// Native context generation. 0 = never probed. Incremented on every reset;
// handles minted under another epoch are rejected as stale.
std::atomic<uint64_t> g_epoch{0};
// Device selected by the last successful probe/reset; -1 = none. The CUDA
// current device is per host thread, so every device-touching entry point
// re-selects it (ensure_device) instead of trusting the caller's thread.
std::atomic<int32_t> g_ordinal{-1};

bool is_sticky(cudaError_t e) {
  switch (e) {
    case cudaErrorIllegalAddress:
    case cudaErrorLaunchTimeout:
    case cudaErrorHardwareStackError:
    case cudaErrorIllegalInstruction:
    case cudaErrorMisalignedAddress:
    case cudaErrorInvalidAddressSpace:
    case cudaErrorInvalidPc:
    case cudaErrorLaunchFailure:
    case cudaErrorAssert:
    case cudaErrorECCUncorrectable:
    case cudaErrorContextIsDestroyed:
    case cudaErrorDeviceUninitialized:
    case cudaErrorUnknown:
      return true;
    default:
      return false;
  }
}

int32_t classify(cudaError_t e) {
  if (e == cudaSuccess) return LOKI_GPU_OK;
  if (e == cudaErrorNotReady) return LOKI_GPU_NOT_READY;
  if (e == cudaErrorMemoryAllocation) return LOKI_GPU_OUT_OF_MEMORY;
  if (e == cudaErrorNoDevice || e == cudaErrorInsufficientDriver ||
      e == cudaErrorInvalidDevice || e == cudaErrorSystemDriverMismatch)
    return LOKI_GPU_NO_DEVICE;
  if (is_sticky(e)) return LOKI_GPU_CONTEXT_LOST;
  return LOKI_GPU_RUNTIME_ERROR;
}

int32_t report(loki_gpu_status_v1 *status, int32_t code, cudaError_t e) {
  if (status != nullptr) {
    status->code = code;
    status->cuda_error = static_cast<int32_t>(e);
    status->sticky = (code == LOKI_GPU_CONTEXT_LOST) ? 1u : 0u;
    status->reserved = 0;
  }
  return code;
}

int32_t report_cuda(loki_gpu_status_v1 *status, cudaError_t e) {
  if (e != cudaSuccess && e != cudaErrorNotReady && !is_sticky(e)) {
    // Clear a non-sticky error so it is not reported again by a later,
    // unrelated call. Sticky errors cannot be cleared.
    (void)cudaGetLastError();
  }
  return report(status, classify(e), e);
}

// Make the selected device current on the calling host thread. Returns
// LOKI_GPU_OK (0) or an already reported failure code.
int32_t ensure_device(loki_gpu_status_v1 *status) {
  int32_t ordinal = g_ordinal.load();
  if (g_epoch.load() == 0 || ordinal < 0)
    return report(status, LOKI_GPU_NO_DEVICE, cudaSuccess);
  int current = -1;
  cudaError_t e = cudaGetDevice(&current);
  if (e == cudaSuccess && current == ordinal) return LOKI_GPU_OK;
  e = cudaSetDevice(ordinal);
  if (e != cudaSuccess) return report_cuda(status, e);
  return LOKI_GPU_OK;
}

int32_t check(const loki_gpu_handle_v1 *h, uint32_t kind) {
  if (h == nullptr) return LOKI_GPU_INVALID_ARGUMENT;
  if (h->abi_version != LOKI_GPU_RT_ABI_VERSION) return LOKI_GPU_ABI_MISMATCH;
  if (h->kind != kind) return LOKI_GPU_INVALID_ARGUMENT;
  if (h->raw == 0) return LOKI_GPU_INVALID_ARGUMENT;
  uint64_t epoch = g_epoch.load();
  if (epoch == 0 || h->context_epoch != epoch) return LOKI_GPU_STALE_HANDLE;
  return LOKI_GPU_OK;
}

int32_t check_host(const loki_gpu_handle_v1 *h) {
  if (h == nullptr) return LOKI_GPU_INVALID_ARGUMENT;
  if (h->kind == LOKI_GPU_HANDLE_HOST_PINNED)
    return check(h, LOKI_GPU_HANDLE_HOST_PINNED);
  return check(h, LOKI_GPU_HANDLE_HOST_PAGEABLE);
}

// `offset + len <= extent` without overflow.
bool in_bounds(uint64_t offset, uint64_t len, uint64_t extent) {
  return offset <= extent && len <= extent - offset;
}

void mint(loki_gpu_handle_v1 *out, uint32_t kind, uint64_t raw,
          uint64_t bytes) {
  out->abi_version = LOKI_GPU_RT_ABI_VERSION;
  out->kind = kind;
  out->context_epoch = g_epoch.load();
  out->raw = raw;
  out->bytes = bytes;
}

template <typename T>
T as_ptr(const loki_gpu_handle_v1 *h) {
  return reinterpret_cast<T>(static_cast<uintptr_t>(h->raw));
}

int32_t fill_info(int32_t ordinal, loki_gpu_device_info_v1 *out,
                  loki_gpu_status_v1 *status) {
  cudaDeviceProp prop;
  cudaError_t e = cudaGetDeviceProperties(&prop, ordinal);
  if (e != cudaSuccess) return report_cuda(status, e);
  size_t free_bytes = 0, total_bytes = 0;
  e = cudaMemGetInfo(&free_bytes, &total_bytes);
  if (e != cudaSuccess) return report_cuda(status, e);
  int runtime_version = 0, driver_version = 0;
  (void)cudaRuntimeGetVersion(&runtime_version);
  (void)cudaDriverGetVersion(&driver_version);

  std::memset(out, 0, sizeof(*out));
  out->abi_version = LOKI_GPU_RT_ABI_VERSION;
  out->ordinal = ordinal;
  out->cc_major = prop.major;
  out->cc_minor = prop.minor;
  out->total_bytes = total_bytes;
  out->free_bytes = free_bytes;
  out->context_epoch = g_epoch.load();
  out->runtime_version = runtime_version;
  out->driver_version = driver_version;
  out->multiprocessors = prop.multiProcessorCount;
  out->async_engines = prop.asyncEngineCount;
  std::strncpy(out->name, prop.name, sizeof(out->name) - 1);
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

__global__ void copy_bytes_kernel(uint8_t *__restrict__ dst,
                                  const uint8_t *__restrict__ src,
                                  uint64_t bytes) {
  const uint64_t stride =
      static_cast<uint64_t>(blockDim.x) * static_cast<uint64_t>(gridDim.x);
  // 16-byte vector body when both pointers are 16-byte aligned.
  const bool vec = ((reinterpret_cast<uintptr_t>(dst) |
                     reinterpret_cast<uintptr_t>(src)) & 15u) == 0;
  uint64_t i = static_cast<uint64_t>(blockIdx.x) * blockDim.x + threadIdx.x;
  uint64_t head = 0;
  if (vec) {
    const uint64_t words = bytes / 16;
    const uint4 *s4 = reinterpret_cast<const uint4 *>(src);
    uint4 *d4 = reinterpret_cast<uint4 *>(dst);
    for (uint64_t w = i; w < words; w += stride) d4[w] = s4[w];
    head = words * 16;
  }
  for (uint64_t b = head + i; b < bytes; b += stride) dst[b] = src[b];
}

}  // namespace

extern "C" uint32_t loki_gpu_rt_abi_version(void) {
  return LOKI_GPU_RT_ABI_VERSION;
}

extern "C" int32_t loki_gpu_rt_abi_layout_v1(loki_gpu_abi_layout_v1 *out) {
  if (out == nullptr) return LOKI_GPU_INVALID_ARGUMENT;
  out->abi_version = LOKI_GPU_RT_ABI_VERSION;
  out->handle_size = sizeof(loki_gpu_handle_v1);
  out->handle_align = alignof(loki_gpu_handle_v1);
  out->handle_offset_context_epoch = offsetof(loki_gpu_handle_v1, context_epoch);
  out->handle_offset_raw = offsetof(loki_gpu_handle_v1, raw);
  out->handle_offset_bytes = offsetof(loki_gpu_handle_v1, bytes);
  out->status_size = sizeof(loki_gpu_status_v1);
  out->status_offset_sticky = offsetof(loki_gpu_status_v1, sticky);
  out->device_info_size = sizeof(loki_gpu_device_info_v1);
  out->device_info_offset_total_bytes =
      offsetof(loki_gpu_device_info_v1, total_bytes);
  out->device_info_offset_context_epoch =
      offsetof(loki_gpu_device_info_v1, context_epoch);
  out->device_info_offset_name = offsetof(loki_gpu_device_info_v1, name);
  out->layout_size = sizeof(loki_gpu_abi_layout_v1);
  out->reserved = 0;
  return LOKI_GPU_OK;
}

extern "C" int32_t loki_gpu_rt_probe_v1(int32_t ordinal,
                                        loki_gpu_device_info_v1 *out,
                                        loki_gpu_status_v1 *status) {
  if (out == nullptr || ordinal < 0)
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  int count = 0;
  cudaError_t e = cudaGetDeviceCount(&count);
  if (e != cudaSuccess) return report_cuda(status, e);
  if (ordinal >= count) return report(status, LOKI_GPU_NO_DEVICE, cudaSuccess);
  e = cudaSetDevice(ordinal);
  if (e != cudaSuccess) return report_cuda(status, e);
  // Force primary-context creation so failures surface here.
  e = cudaFree(nullptr);
  if (e != cudaSuccess) return report_cuda(status, e);
  uint64_t expected = 0;
  g_epoch.compare_exchange_strong(expected, 1);
  // Switching devices invalidates handles minted on the previous one.
  int32_t previous = g_ordinal.exchange(ordinal);
  if (previous >= 0 && previous != ordinal) g_epoch.fetch_add(1);
  return fill_info(ordinal, out, status);
}

extern "C" int32_t loki_gpu_rt_reset_v1(int32_t ordinal,
                                        loki_gpu_device_info_v1 *out,
                                        loki_gpu_status_v1 *status) {
  if (out == nullptr || ordinal < 0)
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  cudaError_t e = cudaSetDevice(ordinal);
  if (e != cudaSuccess) return report_cuda(status, e);
  // Invalidate every outstanding handle before the reset destroys them.
  g_epoch.fetch_add(1);
  e = cudaDeviceReset();
  if (e != cudaSuccess) return report_cuda(status, e);
  e = cudaSetDevice(ordinal);
  if (e != cudaSuccess) return report_cuda(status, e);
  e = cudaFree(nullptr);
  if (e != cudaSuccess) return report_cuda(status, e);
  g_ordinal.store(ordinal);
  return fill_info(ordinal, out, status);
}

extern "C" int32_t loki_gpu_rt_stream_create_v1(loki_gpu_handle_v1 *out,
                                                loki_gpu_status_v1 *status) {
  if (out == nullptr) return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  cudaStream_t s = nullptr;
  cudaError_t e = cudaStreamCreateWithFlags(&s, cudaStreamNonBlocking);
  if (e != cudaSuccess) return report_cuda(status, e);
  mint(out, LOKI_GPU_HANDLE_STREAM, reinterpret_cast<uintptr_t>(s), 0);
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

extern "C" int32_t loki_gpu_rt_stream_destroy_v1(const loki_gpu_handle_v1 *stream,
                                                 loki_gpu_status_v1 *status) {
  int32_t c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaStreamDestroy(as_ptr<cudaStream_t>(stream)));
}

extern "C" int32_t loki_gpu_rt_event_create_v1(uint32_t timing,
                                               loki_gpu_handle_v1 *out,
                                               loki_gpu_status_v1 *status) {
  if (out == nullptr) return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  cudaEvent_t ev = nullptr;
  unsigned flags = timing ? cudaEventDefault : cudaEventDisableTiming;
  cudaError_t e = cudaEventCreateWithFlags(&ev, flags);
  if (e != cudaSuccess) return report_cuda(status, e);
  mint(out, LOKI_GPU_HANDLE_EVENT, reinterpret_cast<uintptr_t>(ev), 0);
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

extern "C" int32_t loki_gpu_rt_event_destroy_v1(const loki_gpu_handle_v1 *event,
                                                loki_gpu_status_v1 *status) {
  int32_t c = check(event, LOKI_GPU_HANDLE_EVENT);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaEventDestroy(as_ptr<cudaEvent_t>(event)));
}

extern "C" int32_t loki_gpu_rt_device_alloc_v1(uint64_t bytes,
                                               loki_gpu_handle_v1 *out,
                                               loki_gpu_status_v1 *status) {
  if (out == nullptr || bytes == 0 ||
      bytes > static_cast<uint64_t>(std::numeric_limits<size_t>::max()))
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  void *p = nullptr;
  cudaError_t e = cudaMalloc(&p, static_cast<size_t>(bytes));
  if (e != cudaSuccess) return report_cuda(status, e);
  mint(out, LOKI_GPU_HANDLE_DEVICE_MEMORY, reinterpret_cast<uintptr_t>(p), bytes);
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

extern "C" int32_t loki_gpu_rt_device_free_v1(const loki_gpu_handle_v1 *memory,
                                              loki_gpu_status_v1 *status) {
  int32_t c = check(memory, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaFree(as_ptr<void *>(memory)));
}

extern "C" int32_t loki_gpu_rt_host_alloc_v1(uint64_t bytes, uint32_t pinned,
                                             loki_gpu_handle_v1 *out,
                                             loki_gpu_status_v1 *status) {
  if (out == nullptr || bytes == 0 ||
      bytes > static_cast<uint64_t>(std::numeric_limits<size_t>::max()))
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  void *p = nullptr;
  if (pinned) {
    cudaError_t e = cudaHostAlloc(&p, static_cast<size_t>(bytes), cudaHostAllocDefault);
    if (e != cudaSuccess) return report_cuda(status, e);
    mint(out, LOKI_GPU_HANDLE_HOST_PINNED, reinterpret_cast<uintptr_t>(p), bytes);
  } else {
    p = std::malloc(static_cast<size_t>(bytes));
    if (p == nullptr) return report(status, LOKI_GPU_OUT_OF_MEMORY, cudaSuccess);
    mint(out, LOKI_GPU_HANDLE_HOST_PAGEABLE, reinterpret_cast<uintptr_t>(p), bytes);
  }
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

extern "C" int32_t loki_gpu_rt_host_free_v1(const loki_gpu_handle_v1 *buffer,
                                            loki_gpu_status_v1 *status) {
  int32_t c = check_host(buffer);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  if (buffer->kind == LOKI_GPU_HANDLE_HOST_PINNED)
    return report_cuda(status, cudaFreeHost(as_ptr<void *>(buffer)));
  std::free(as_ptr<void *>(buffer));
  return report(status, LOKI_GPU_OK, cudaSuccess);
}

extern "C" int32_t loki_gpu_rt_memcpy_h2d_v1(const loki_gpu_handle_v1 *dst,
                                             uint64_t dst_offset,
                                             const loki_gpu_handle_v1 *src_host,
                                             uint64_t src_offset, uint64_t len,
                                             const loki_gpu_handle_v1 *stream,
                                             loki_gpu_status_v1 *status) {
  int32_t c = check(dst, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK) c = check_host(src_host);
  if (c == LOKI_GPU_OK) c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  if (len == 0 || !in_bounds(dst_offset, len, dst->bytes) ||
      !in_bounds(src_offset, len, src_host->bytes))
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  cudaError_t e = cudaMemcpyAsync(as_ptr<uint8_t *>(dst) + dst_offset,
                                  as_ptr<const uint8_t *>(src_host) + src_offset,
                                  static_cast<size_t>(len), cudaMemcpyHostToDevice,
                                  as_ptr<cudaStream_t>(stream));
  return report_cuda(status, e);
}

extern "C" int32_t loki_gpu_rt_memcpy_d2h_v1(const loki_gpu_handle_v1 *dst_host,
                                             uint64_t dst_offset,
                                             const loki_gpu_handle_v1 *src,
                                             uint64_t src_offset, uint64_t len,
                                             const loki_gpu_handle_v1 *stream,
                                             loki_gpu_status_v1 *status) {
  int32_t c = check_host(dst_host);
  if (c == LOKI_GPU_OK) c = check(src, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK) c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  if (len == 0 || !in_bounds(dst_offset, len, dst_host->bytes) ||
      !in_bounds(src_offset, len, src->bytes))
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  cudaError_t e = cudaMemcpyAsync(as_ptr<uint8_t *>(dst_host) + dst_offset,
                                  as_ptr<const uint8_t *>(src) + src_offset,
                                  static_cast<size_t>(len), cudaMemcpyDeviceToHost,
                                  as_ptr<cudaStream_t>(stream));
  return report_cuda(status, e);
}

extern "C" int32_t loki_gpu_rt_launch_copy_v1(const loki_gpu_handle_v1 *dst,
                                              const loki_gpu_handle_v1 *src,
                                              uint64_t bytes,
                                              const loki_gpu_handle_v1 *stream,
                                              loki_gpu_status_v1 *status) {
  int32_t c = check(dst, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK) c = check(src, LOKI_GPU_HANDLE_DEVICE_MEMORY);
  if (c == LOKI_GPU_OK) c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  if (bytes == 0 || bytes > dst->bytes || bytes > src->bytes)
    return report(status, LOKI_GPU_INVALID_ARGUMENT, cudaSuccess);
  const unsigned threads = 256;
  uint64_t blocks64 = (bytes / 16 + threads - 1) / threads;
  if (blocks64 == 0) blocks64 = 1;
  if (blocks64 > 4096) blocks64 = 4096;
  copy_bytes_kernel<<<static_cast<unsigned>(blocks64), threads, 0,
                      as_ptr<cudaStream_t>(stream)>>>(
      as_ptr<uint8_t *>(dst), as_ptr<const uint8_t *>(src), bytes);
  return report_cuda(status, cudaGetLastError());
}

extern "C" int32_t loki_gpu_rt_event_record_v1(const loki_gpu_handle_v1 *event,
                                               const loki_gpu_handle_v1 *stream,
                                               loki_gpu_status_v1 *status) {
  int32_t c = check(event, LOKI_GPU_HANDLE_EVENT);
  if (c == LOKI_GPU_OK) c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaEventRecord(as_ptr<cudaEvent_t>(event),
                                             as_ptr<cudaStream_t>(stream)));
}

extern "C" int32_t loki_gpu_rt_stream_wait_event_v1(const loki_gpu_handle_v1 *stream,
                                                    const loki_gpu_handle_v1 *event,
                                                    loki_gpu_status_v1 *status) {
  int32_t c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c == LOKI_GPU_OK) c = check(event, LOKI_GPU_HANDLE_EVENT);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaStreamWaitEvent(as_ptr<cudaStream_t>(stream),
                                                 as_ptr<cudaEvent_t>(event), 0));
}

extern "C" int32_t loki_gpu_rt_event_synchronize_v1(const loki_gpu_handle_v1 *event,
                                                    loki_gpu_status_v1 *status) {
  int32_t c = check(event, LOKI_GPU_HANDLE_EVENT);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaEventSynchronize(as_ptr<cudaEvent_t>(event)));
}

extern "C" int32_t loki_gpu_rt_event_query_v1(const loki_gpu_handle_v1 *event,
                                              loki_gpu_status_v1 *status) {
  int32_t c = check(event, LOKI_GPU_HANDLE_EVENT);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaEventQuery(as_ptr<cudaEvent_t>(event)));
}

extern "C" int32_t loki_gpu_rt_event_elapsed_ms_v1(const loki_gpu_handle_v1 *start,
                                                   const loki_gpu_handle_v1 *end,
                                                   float *ms,
                                                   loki_gpu_status_v1 *status) {
  int32_t c = check(start, LOKI_GPU_HANDLE_EVENT);
  if (c == LOKI_GPU_OK) c = check(end, LOKI_GPU_HANDLE_EVENT);
  if (c == LOKI_GPU_OK && ms == nullptr) c = LOKI_GPU_INVALID_ARGUMENT;
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaEventElapsedTime(ms, as_ptr<cudaEvent_t>(start),
                                                  as_ptr<cudaEvent_t>(end)));
}

extern "C" int32_t loki_gpu_rt_stream_synchronize_v1(const loki_gpu_handle_v1 *stream,
                                                     loki_gpu_status_v1 *status) {
  int32_t c = check(stream, LOKI_GPU_HANDLE_STREAM);
  if (c != LOKI_GPU_OK) return report(status, c, cudaSuccess);
  if (int32_t d = ensure_device(status)) return d;
  return report_cuda(status, cudaStreamSynchronize(as_ptr<cudaStream_t>(stream)));
}
