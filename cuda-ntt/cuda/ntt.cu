#include <ff/bls12-381.hpp>
#include <ntt/ntt.cuh>

// Fixed ABI: reduced Montgomery residues, four little-endian u64 limbs.
static_assert(sizeof(fr_t) == 32, "unexpected sppark Fr size");

extern "C" RustError::by_value loki_ntt(fr_t* values, uint32_t log_n,
                                       bool inverse, const fr_t* root)
{
    // Root comparison also guards against a future change in either library's
    // root convention. Never return a transform with a silently permuted domain.
    const auto& expected = inverse ? inverse_roots_of_unity[log_n]
                                   : forward_roots_of_unity[log_n];
    if (std::memcmp(root, &expected, sizeof(fr_t)) != 0)
        return RustError{-1, "Loki/sppark NTT roots differ"};
    try {
        if (ngpus() == 0)
            return RustError{-1, "cuda-ntt requires a CUDA device"};
        return NTT::Base(select_gpu(0), values, log_n,
                         NTT::InputOutputOrder::NN,
                         inverse ? NTT::Direction::inverse
                                 : NTT::Direction::forward,
                         NTT::Type::standard);
    } catch (const std::exception& e) {
        return RustError{-1, e.what()};
    } catch (...) {
        return RustError{-1, "unknown CUDA NTT error"};
    }
}
