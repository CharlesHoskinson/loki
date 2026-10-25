# ghaiku-7: sm_120 / CUDA 13.4 target facts (accessed 2026-10-24, NVIDIA public docs only)

1. sm_120 = CC 12.0 (RTX 5090). Targets compute_120 / 120a / 120f; 'a' runs only on exact CC.
2. A cubin runs only on the same major version with the same or a higher minor version, so sm_100 cubins will NOT load on CC 12.0. Ship native sm_120. Whether PTX compiled for compute_100 can be JIT-compiled to sm_120 is still open.
3. Build flags: -gencode=arch=compute_120,code=sm_120 -gencode=arch=compute_120,code=compute_120 (derived from nvcc 13.4 docs).
4. CC 12.0 limits (Blackwell Tuning Guide 13.4 §1.4.1.1):
   - 99 KB shared memory per block (opt-in above 48 KB).
   - 48 warps per SM; 32 blocks per SM.
   - 64K registers per SM; 255 registers per thread; 1024 threads per block.
   - The shared-memory-per-SM figure conflicts between docs (128 vs 100 KB). Plan for 100 KB and query cudaDevAttrMaxSharedMemoryPerMultiprocessor at runtime.
5. CUDA 13.4 (NVCC 13.4.59) and Update 2 (13.4.92) add a ptxas fix for predicated REDUX.SUM on Blackwell. Whether 13.4.3 is out is unverified; pin `nvcc --version` in PROVENANCE. The driver is no longer bundled with the toolkit on Linux.

Implications: keep kernels under 99 KB of shared memory per block and 255 registers per thread. Expect 48-warp occupancy, and don't reuse B200 (CC 10.0) tuning.
Sources: docs.nvidia.com cuda-programming-guide compute-capabilities; blackwell-compatibility-guide; cuda-compiler-driver-nvcc; blackwell-tuning-guide; cuda-toolkit-release-notes.
(Saved by coordinator; the scout's Write was refused by its harness.)
