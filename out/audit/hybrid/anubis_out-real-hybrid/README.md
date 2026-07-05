Anubis full hybrid host project.

Shape: vendored risc0-metal-hybrid patch + risc0-build methods crate + generated guest ELF/image ID + stock receipt.verify(ANUBIS_ID).
Build: cargo build --release
Run: ./target/release/anubis_hybrid_host
CPU lane: R0_DISABLE_METAL=1 ./target/release/anubis_hybrid_host
