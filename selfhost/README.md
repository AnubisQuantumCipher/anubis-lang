# Anubis self-host (Phase 8)

See `SUBSET.md` and `docs/language/SELFHOST.md`.

```bash
# From repo root
cargo build --release -p anubis
bash scripts/run_selfhost_gate.sh
# SELFHOST_GATE: PASS (8/8)  — real stage0→1→2→3 bootstrap

# Manual stage0
./target/release/anubis run selfhost/src/anubis_sh.anb --allow-research -- lex corpus/ok_hello.anb
./target/release/anubis run selfhost/src/anubis_sh.anb --allow-research -- check corpus/bad_type.anb
./target/release/anubis run selfhost/src/anubis_sh.anb --allow-research -- compile corpus/ok_hello.anb -o /tmp/hello.rs
rustc -O /tmp/hello.rs -o /tmp/hello && /tmp/hello   # prints: hello, anubis

# Manual bootstrap step
./target/release/anubis run selfhost/src/anubis_sh.anb --allow-research -- \
  compile src/anubis_sh.anb -o /tmp/stage1.rs
rustc -O /tmp/stage1.rs -o /tmp/stage1
/tmp/stage1 compile src/anubis_sh.anb -o /tmp/stage2.rs
```

Runtime embedded into stage packages: `runtime/anubis_sh_interp_rt.rs`.
