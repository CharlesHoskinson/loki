// -*- mode: Bluespec; -*-
// Deliberately wrong W13 variants. Each must violate the invariant named in
// q4/check.py; M_LEGACY_LEN is the one expected-benign control.
module mut_addr_only { import srs_w13(REGISTRY = true, MUT = 1).* from "./srs_w13" }
module mut_addr_only_legacy { import srs_w13(REGISTRY = false, MUT = 1).* from "./srs_w13" }
module mut_first_block { import srs_w13(REGISTRY = true, MUT = 2).* from "./srs_w13" }
module mut_first_block_legacy { import srs_w13(REGISTRY = false, MUT = 2).* from "./srs_w13" }
module mut_no_len { import srs_w13(REGISTRY = true, MUT = 3).* from "./srs_w13" }
module mut_ordinal0_sm { import srs_w13(REGISTRY = true, MUT = 4).* from "./srs_w13" }
module mut_charge_n { import srs_w13(REGISTRY = true, MUT = 5).* from "./srs_w13" }
module mut_no_stale_evict { import srs_w13(REGISTRY = true, MUT = 6).* from "./srs_w13" }
module mut_budget_used { import srs_w13(REGISTRY = true, MUT = 7).* from "./srs_w13" }
module mut_release_drop { import srs_w13(REGISTRY = true, MUT = 8).* from "./srs_w13" }
module mut_short_join { import srs_w13(REGISTRY = true, MUT = 9).* from "./srs_w13" }
module mut_floor_chunk { import srs_w13(REGISTRY = true, MUT = 10).* from "./srs_w13" }
module mut_validate_prefix { import srs_w13(REGISTRY = true, MUT = 11).* from "./srs_w13" }
module mut_skip_failed { import srs_w13(REGISTRY = true, MUT = 12).* from "./srs_w13" }
module mut_legacy_len { import srs_w13(REGISTRY = false, MUT = 13).* from "./srs_w13" }
module mut_evict_no_release { import srs_w13(REGISTRY = true, MUT = 14).* from "./srs_w13" }
module mut_release_keep { import srs_w13(REGISTRY = true, MUT = 15).* from "./srs_w13" }
module mut_legacy_three { import srs_w13(REGISTRY = false, MUT = 16).* from "./srs_w13" }
module mut_refresh_snapshot { import srs_w13(REGISTRY = true, MUT = 17).* from "./srs_w13" }
