#![no_main]
use libfuzzer_sys::fuzz_target;

// The negative_cases target: invalid witness, altered proof bytes, changed
// public inputs, mismatched VK, truncation and trailing bytes.
fuzz_target!(|data: &[u8]| loki_diff::proof::soundness(data));
