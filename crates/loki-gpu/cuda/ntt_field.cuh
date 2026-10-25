// SPDX-License-Identifier: Apache-2.0
//
// BLS12-381 scalar field Fr for the P4 transform kernels.
//
// The Montgomery arithmetic template `mont_t` is sppark 0.1.16
// (vendor/sppark-0.1.16/ff/mont_t.cuh, Apache-2.0, Supranational LLC). The
// instantiation constants below are mathematical facts about
// r = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
// and were recomputed independently (Python big integers):
//   rRR  = 2^512 mod r     (to-Montgomery factor)
//   rone = 2^256 mod r     (Montgomery one)
//   rx2  = 2r              (left-aligned modulus used by mont_t)
//   m0   = -r^-1 mod 2^32
// Limbs are little-endian 32-bit words.

#ifndef LOKI_GPU_NTT_FIELD_CUH
#define LOKI_GPU_NTT_FIELD_CUH

#include <cstdint>

// The constants are declared in both compilation passes: the host-side stub
// registers every __constant__ symbol even though only device code reads it.
namespace loki_fr {
static __device__ __constant__ __align__(16) const uint32_t R[8] = {
    0x00000001u, 0xffffffffu, 0xfffe5bfeu, 0x53bda402u,
    0x09a1d805u, 0x3339d808u, 0x299d7d48u, 0x73eda753u};
static __device__ __constant__ __align__(16) const uint32_t RRR[8] = {
    0xf3f29c6du, 0xc999e990u, 0x87925c23u, 0x2b6cedcbu,
    0x7254398fu, 0x05d31496u, 0x9f59ff11u, 0x0748d9d9u};
static __device__ __constant__ __align__(16) const uint32_t RONE[8] = {
    0xfffffffeu, 0x00000001u, 0x00034802u, 0x5884b7fau,
    0xecbc4ff5u, 0x998c4fefu, 0xacc5056fu, 0x1824b159u};
static __device__ __constant__ __align__(16) const uint32_t RX2[8] = {
    0x00000002u, 0xfffffffeu, 0xfffcb7fdu, 0xa77b4805u,
    0x1343b00au, 0x6673b010u, 0x533afa90u, 0xe7db4ea6u};
static __device__ __constant__ /*const*/ uint32_t M0 = 0xffffffffu;
}  // namespace loki_fr

#if defined(__CUDA_ARCH__)

#include "vendor/sppark-0.1.16/ff/mont_t.cuh"

typedef mont_t<255, loki_fr::R, loki_fr::M0, loki_fr::RRR, loki_fr::RONE,
               loki_fr::RX2>
    fr_t;

// Canonical little-endian limbs -> Montgomery. `in` must be < r (resident
// buffers only ever hold canonical bytes written by this code or by
// `to_repr` uploads).
__device__ __forceinline__ fr_t fr_from_canonical(const uint32_t *in) {
  fr_t x(in);
  x.to();
  return x;
}

// x >= r ? (compare limbs from the top)
__device__ __forceinline__ bool fr_ge_modulus(const uint32_t *x) {
  for (int i = 7; i >= 0; --i) {
    if (x[i] != loki_fr::R[i]) return x[i] > loki_fr::R[i];
  }
  return true;
}

__device__ __forceinline__ void fr_sub_modulus(uint32_t *x) {
  uint64_t borrow = 0;
  for (int i = 0; i < 8; ++i) {
    uint64_t d = (uint64_t)x[i] - loki_fr::R[i] - borrow;
    x[i] = (uint32_t)d;
    borrow = (d >> 63) & 1u;
  }
}

// Montgomery -> fully reduced canonical little-endian limbs. The explicit
// reduction makes the output canonical regardless of how far mont_t keeps
// intermediate results reduced (any 256-bit value is < 3r).
__device__ __forceinline__ void fr_store_canonical(fr_t x, uint32_t *out) {
  x.from();
  uint32_t v[8];
  x.store(v);
  if (fr_ge_modulus(v)) fr_sub_modulus(v);
  if (fr_ge_modulus(v)) fr_sub_modulus(v);
  for (int i = 0; i < 8; ++i) out[i] = v[i];
}

#endif  // __CUDA_ARCH__

#endif  // LOKI_GPU_NTT_FIELD_CUH
