# TASK fuzz-practice: fuzzing practice for a Rust crypto library

Research 2026-10-25. Each claim cites a URL. "Fetched" means I read the page; "snippet" means only a search result summary, not opened.

## Sources
- S1 libFuzzer docs, https://llvm.org/docs/LibFuzzer.html (fetched)
- S2 Rust Fuzz Book, cargo-fuzz, https://rust-fuzz.github.io/book/cargo-fuzz.html (fetched; overview only)
- S3 Rust Fuzz Book, structure-aware fuzzing, https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html (fetched; partial)
- S4 OSS-Fuzz Rust guide, https://google.github.io/oss-fuzz/getting-started/new-project-guide/rust-lang/ (fetched)
- S5 OSS-Fuzz bug disclosure, https://google.github.io/oss-fuzz/getting-started/bug-disclosure-guidelines/ (fetched)
- S6 Quarkslab, "Differential fuzzing for cryptography", https://blog.quarkslab.com/differential-fuzzing-for-cryptography.html (snippet only)
- S7 AdaCore, "Automated Assurance through Differential Fuzzing", https://www.adacore.com/blog/automated-assurance-through-differential-fuzzing (snippet only)
- S8 Ding and Le Goues, "An Empirical Study of OSS-Fuzz Bugs", https://arxiv.org/abs/2103.11518 (snippet only; 2021 data)

## Practices

### Libfuzzer and sanitizers
1. Run with sanitizers. S1: "combining libFuzzer with AddressSanitizer, UndefinedBehaviorSanitizer, or both" is recommended; MSan is experimental. S4: OSS-Fuzz supports only `libfuzzer` and `address`.
2. Set explicit limits. S1 defaults: `-rss_limit_mb` 2048, `-timeout` 1200 s, `-max_len` guessed from corpus when 0. Loki's `fuzz/run-target.sh:14-15` already sets `-timeout=120 -rss_limit_mb=8192 -max_len=4096 -len_control=0`. Matches S1 guidance; `-len_control=0` is the documented setting for "immediately tries inputs up to max_len" (S1), which is the fix for the 4-6 byte problem in `LOKI-FUZZ.md:105-111`.
3. Deterministic target. S1: the target "should be as deterministic as possible"; threads "should ideally all be joined at the end of the function". Loki's harness sets process-wide env vars (`fuzz/diff/src/msm.rs:64-65`, `ntt.rs:66-70`); S1 says global state "is not strict" but the FAQ calls significant unreset global state a poor fit (S1).
4. Corpus minimisation. S1: `-merge=1 NEW_DIR FULL_DIR` minimizes a corpus. Loki's `LOKI-FUZZ.md` records `after cmin` counts (`LOKI-FUZZ.md:91-96`), which matches.
5. Value profile. S1: `-use_value_profile` "up to 2x additional slowdown", corpus may grow several times. Not used by `run-target.sh`; worth a measured A/B, not a default.

### Structure-aware input
6. S3 says structure-aware generation produces structured inputs "rather than raw, pseudorandom bytes" and that `arbitrary` derive support is provided via `libfuzzer-sys` (`arbitrary-derive` feature). S3 also reports that `fuzz_mutator!` (mutatis) "gave better coverage over time than arbitrary-based generation" in one experiment it links. Loki's own byte decoder (`fuzz/diff/src/lib.rs:30-98`) is already structured, but the seed is a `u64` drawn after the case, so most of the input is spent on the circuit shape. This matches the T32 gap (`TODO.md:131`).
7. Practice to adopt (my synthesis from S1 and S3, not a quoted rule): derive the ChaCha seed from the input bytes instead of consuming a fixed tail, so libFuzzer mutates the seed bytes too. Loki currently reseeds the stream from `input.u64()` (`fuzz/diff/src/proof.rs:352-354`), which is why 4-byte inputs sampled seeds (`LOKI-FUZZ.md:107-109`).

### Corpora and crash handling
8. S4: OSS-Fuzz needs `fuzz/Cargo.toml` and `fuzz/fuzz_targets/*.rs`, a build with `cargo fuzz build -O`, and the nightly builder. Loki's `fuzz/` layout already matches.
9. Reproduction: S2 "it is quite easy to run the fuzzers locally yourself if you get a failing test case" (S4 quotes the same idea). Loki keeps reproducers as `cases.rs` tests (`LOKI-FUZZ.md:126-164`), which is a good practice; keep it.
10. Timeouts and OOMs: S5 and a secondhand OSS-Fuzz FAQ (not opened) say timeouts and OOMs are not deduplicated, so each target files one of each. NOT verified against the FAQ.

### Disclosure timeline (for an upstream/security finding)
11. S5: "we will open reported issues to the public in 90 days," or earlier "after the fix is released (whichever comes earlier)". If a fix is scheduled within 14 days after an expired deadline and upstream confirms it before the deadline, disclosure waits for the patch. Weekend deadlines move to the next working day.
12. Conflict: S8 (2021 study) describes an earlier 30-days-after-patch rule. S5, the current page, does not state that rule. Treat the 30-day rule as UNVERIFIED and do not write it into the spec.
13. Fix verification: S5 does not describe how a fix is verified (reproducer rerun). NOT verified.

### Differential crypto fuzzing
14. S6 (snippet): round-trip oracles (decrypt after encrypt, verify after sign) test consistency without fixed expected output. Applies here as prove-then-verify under both stacks (`proof.rs:358-405` already does this).
15. S6 (snippet): crypto-condor and CDF use a reference implementation as oracle. S7 (snippet) uses XKCP as a reference model plus test vectors (35,982 vectors and 146 unit tests cited in S7).
16. Limit of the oracle (snippet, S6/S7 and the DBMS survey in search results): a differential oracle cannot see bugs both implementations share. Loki's stock oracle has this blind spot for the four upstream defects in `LOKI-FUZZ.md:125-164`; they are shared, so only fixed-vector and Lean checks catch them. Fuzzing alone is not acceptance evidence (`docs/ARCHITECTURE.md:171`).
17. Negative controls: the repo's `truncated-loki-only` configuration (`fuzz/Cargo.toml:17`, `quick.sh:12`) is a deliberate mismatch that must fail. This follows the oracle-positive-control practice in S6/S7 and is good.

### Campaign budget data
18. S8 (2021, dated): 90% of OSS-Fuzz fixes landed within 90 days; median time-to-fix 5.3 days. Dated data; do not use as a current budget.

## What is unverified
- The S5 30-day-after-fix rule (conflicting, item 12).
- Fix verification mechanics (item 13).
- Timeout/OOM dedup policy (item 10).
- S6, S7 and S8 content beyond the search snippets.
- Nothing here measured Loki campaigns; all budget numbers are from `LOKI-FUZZ.md`.
