// sppark is Apache-2.0; this adapter instantiates its BLS12-381 MSM templates.
#include <cuda.h>
#include <ff/bls12-381.hpp>
#include <ec/jacobian_t.hpp>
#include <ec/xyzz_t.hpp>

using point_t = jacobian_t<fp_t>;
using bucket_t = xyzz_t<fp_t>;
using affine_t = bucket_t::affine_t;
using scalar_t = fr_t;
#include <msm/pippenger.cuh>

#ifndef __CUDA_ARCH__
#include <cstring>
#include <exception>

using resident_t = msm_t<bucket_t, point_t, affine_t, scalar_t>;
struct wire_affine { uint64_t x[6], y[6]; };
struct wire_scalar { uint64_t limbs[4]; };
static_assert(sizeof(affine_t) == sizeof(wire_affine), "affine ABI");
static_assert(alignof(affine_t) == alignof(wire_affine), "affine alignment");
static_assert(sizeof(scalar_t) == sizeof(wire_scalar), "scalar ABI");
static_assert(alignof(scalar_t) == alignof(wire_scalar), "scalar alignment");

extern "C" RustError::by_value
loki_gpu_prepare(void** out, const wire_affine* bases, size_t n) noexcept
{
    *out = nullptr;
    try {
        // Constructor copies bases to persistent VRAM and allocates buckets.
        *out = new resident_t(reinterpret_cast<const affine_t*>(bases), n,
                              sizeof(wire_affine), 0);
        // Upload uses sppark's zero stream; kernels use flip-flop streams.
        // Complete the copy before Rust drops the host wire array.
        select_gpu(0).sync();
        return RustError{};
    } catch (const cuda_error& e) {
        return RustError{e.code(), e.what()};
    } catch (const std::exception& e) {
        return RustError{-1, e.what()};
    } catch (...) {
        return RustError{-1, "unknown GPU preparation error"};
    }
}

extern "C" RustError::by_value
loki_gpu_msm(void* resident, wire_affine* out, const wire_scalar* scalars,
             size_t n) noexcept
{
    try {
        // CUDA's active device is per host thread; callers may use Rayon.
        select_gpu(0);
        point_t result;
        auto status = static_cast<resident_t*>(resident)->invoke(
            result, static_cast<const affine_t*>(nullptr), n,
            reinterpret_cast<const scalar_t*>(scalars), true);
        if (status.code != cudaSuccess) return status;
        if (result.is_inf()) {
            std::memset(out, 0, sizeof(*out));
        } else {
            // sppark returns Jacobian coordinates. Midnight's internal
            // projective conventions are irrelevant at this affine boundary.
            affine_t affine = result;
            std::memcpy(out, &affine, sizeof(*out));
        }
        return RustError{};
    } catch (const cuda_error& e) {
        return RustError{e.code(), e.what()};
    } catch (const std::exception& e) {
        return RustError{-1, e.what()};
    } catch (...) {
        return RustError{-1, "unknown GPU MSM error"};
    }
}

extern "C" RustError::by_value loki_gpu_release(void* resident) noexcept
{
    try {
        select_gpu(0);
        // Check for a poisoned context before releasing. build.rs makes
        // sppark's destructor noexcept(false), so errors during free/sync
        // also reach this catch. Rust disables the backend after any error.
        CUDA_OK(cudaDeviceSynchronize());
        delete static_cast<resident_t*>(resident);
        return RustError{};
    } catch (const cuda_error& e) {
        return RustError{e.code(), e.what()};
    } catch (const std::exception& e) {
        return RustError{-1, e.what()};
    } catch (...) {
        return RustError{-1, "unknown GPU release error"};
    }
}
#endif
