# Vendored sppark 0.1.16 field arithmetic

Files copied verbatim from the crates.io package `sppark` 0.1.16
(https://github.com/supranational/sppark), licensed Apache-2.0. Each file
keeps its original copyright and SPDX header. The Apache-2.0 license text is
the repository `LICENSE`.

| File | Upstream path | sha256 |
|---|---|---|
| `ff/mont_t.cuh` | `sppark/ff/mont_t.cuh` | `f6f89b574507a6b07dec5e5f62d4d4d705a4a3b062187d7cda9d003809b96473` |
| `ff/pow.hpp` | `sppark/ff/pow.hpp` | `487b8c43b7c64956f5ba5a45413c218c5a1848c2b56141f643b124bddb49611f` |

Only the device-side Montgomery field template is used (by `../ntt_field.cuh`).
No sppark NTT driver, twiddle table, stream or GPU-selection code is included.
Do not edit these files; replace them only with another verbatim release.
