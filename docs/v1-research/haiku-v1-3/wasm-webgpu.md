# TASK wasm-webgpu: WebGPU feasibility for Loki proof acceleration, and the v1 boundary

Evidence in repo: docs/PLATFORMS.md:360-400 (section 9.2, design only, all TODO; WGSL has no 64-bit integer; 32-bit limbs of 8 for Fr and 12 for Fp; async mapAsync readback; `GPUDevice.lost` mapped to ContextLost; separate GPU buffer and wasm linear memory limits). Matrix row PLATFORMS.md:415-421 shows WebGPU MSM/NTT/quotient as TODO.

Browser status (checked 2026-10-25):
- MDN WebGPU API page, https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API: marked "Limited availability", not Baseline, because it does not work in some widely used browsers. Secure context (HTTPS) required.
- web.dev, https://web.dev/blog/webgpu-supported-major-browsers (from search, page not opened): says WebGPU is supported in Chrome, Edge, Firefox and Safari as of Nov 2025.
- Firefox: Windows from 141 per linuxiac, https://linuxiac.com/webgpu-lands-in-firefox-141-on-windows-eyes-linux-and-macos-next/ (opened only via search snippet). Later Mac status is reported only by a secondary guide and is not verified.
- Safari: Safari 26 on macOS per the same secondary guide; not verified against WebKit release notes.
- shader-f16: Chrome shipped WGSL f16 in M120. Whether Firefox and Safari expose `shader-f16` was not found in sources here. Runtime check: `adapter.features.has('shader-f16')`, with a fallback path.

Feasibility judgment:
- Compute is feasible in principle. The only precedent sources are the ones listed in docs/PLATFORMS.md:380-390 (none on BLS12-381 KZG). Their numbers are not Loki numbers.
- Limits to design around: no 64-bit integers in WGSL (32-bit limbs required, as in PLATFORMS.md:362); no blocking readback (async `mapAsync`); device loss mapping; buffer limits queried from the adapter at runtime.
- Browser-level constraints: GPU buffer capacity and wasm linear memory are separate budgets. Measure per browser on target devices.

v1 boundary (recommendation, for compat-api and gpu owners to decide):
- In v1: CPU wasm32 build (single-threaded default, optional threads) with a stock-compat gate run on the produced bytes. No WebGPU.
- Post-v1: WebGPU backend, starting with the 32-bit-limb Fr/Fp kernels as a correctness oracle (PLATFORMS.md:374, 394), then NTT and MSM. Gate it behind an opt-in feature, never on the default CPU path, and require byte-equality with the CPU path.
- Explicitly out of v1: memory64 (Safari unsupported, see wasm-best-practice.md), any claim of WebGPU speedup.

Sources:
- https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API
- https://web.dev/blog/webgpu-supported-major-browsers
- https://linuxiac.com/webgpu-lands-in-firefox-141-on-windows-eyes-linux-and-macos-next/
- https://www.w3.org/TR/webgpu/ and https://www.w3.org/TR/WGSL/ (cited in PLATFORMS.md; not opened)
