fn main() { println!("cargo:rerun-if-changed=src/fts_rank_math.c"); cc::Build::new().file("src/fts_rank_math.c").compile("tezbar_fts_rank_math"); }
