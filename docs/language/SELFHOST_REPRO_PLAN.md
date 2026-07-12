# External Reproducibility Gate — scope

Next phase after the self-host dogfood (`selfhost-fixpoint-v1`). Goal: turn the
**internal** binary fixpoint (same machine, same toolchain) into an **externally
reproducible** one — a third party, on a different machine, reproduces the exact
sealed self-host binary from source. This is the reproducible-builds standard
(Debian/Nix), and it is the trustworthiness increment Rust still lacks cleanly
(rust-lang/rust#34902).

## What is already true (measured 2026-07-12, not assumed)

- **Same-toolchain determinism holds.** Building `stage2.rs` twice from a
  canonical source path (`canon.rs`) under one toolchain, then stripping the
  ad-hoc signature (`codesign --remove-signature`) and zeroing `LC_UUID`
  (`scripts/macho_normalize.py`), yields **bit-identical** binaries
  (sha `12e62c50…`). The seal's binary fixpoint (`c640badd…`) is legitimate.
- **The obstacle to *external* repro is embedded absolute paths.** The binary
  contains `/Users/<user>/.rustup/toolchains/<ver>/lib/rustlib/src/...` (9
  occurrences). A different machine/user gets different bytes.
- **Fix is proven to work.** `rustc --remap-path-prefix $HOME=/anubis-home
  --remap-path-prefix <builddir>=/anubis-build` drops machine-specific paths to
  **0** while keeping the pinned toolchain version (correct — the version is a
  pinned input, not machine identity).
- **Cross-*version* bit-identity is NOT a target.** stable 1.94 vs nightly 1.97
  emit different-sized binaries (826920 vs 856224). Different rustc = different
  codegen is normal, not a defect. Do not gate on it.

## Claim boundary (do not overclaim)

This phase proves **reproducibility**: pinned toolchain + hermetic environment →
bit-identical binary, re-derivable by anyone. It does **not** prove
toolchain-diversity / Thompson closure — a subverted rustc reproduces its own
subversion here too. Toolchain diversity is now addressed by a separate lane, the
**Diverse Double-Compiling gate** (`scripts/run_selfhost_ddc_gate.sh`): a second,
non-LLVM interpreter (`selfhost/backend_c/anubis_sh_interp_rt.c`, gcc) must emit
byte-identical compiler output to the rustc lane. It narrows the Thompson attack
but does not fully close it (the shared payload-AST *source* is still rustc-derived
— see SELFHOST.md's DDC scope). Keep the reproducibility and diversity claims
distinct.

## Gate design (fail-closed, wire into run_selfhost_gate.sh)

1. **Pin the toolchain.** `rust-toolchain.toml` → `nightly-2026-05-10` (the
   version the seal already uses). The gate refuses to run under any other.
2. **Canonicalize the build.** Compile `stage2.rs` from `canon.rs` with
   `--remap-path-prefix $HOME=/anubis-home --remap-path-prefix <builddir>=/anubis-build`
   and `SOURCE_DATE_EPOCH=0`; normalize (sig strip + UUID zero). Record the sha.
3. **Determinism check (host):** two independent clean-dir builds → bit-identical.
   (Already passes; make it explicit and gated.)
4. **Machine-independence check:** build once with `HOME` remapped to a decoy
   prefix; assert 0 machine-identity strings remain. (PoC passes.)
5. **Hermetic lane (Docker — installed; Nix is not):** build `stage2.rs` inside a
   pinned `rust:<ver>` container twice, in two independent runs; require
   bit-identical Linux ELF (own normalizer for ELF `.note.gnu.build-id`). This is
   the publishable artifact: `docker run … → sha X`, reproducible by anyone with
   the image digest + source. Emit `repro_manifest.json` (toolchain version,
   image digest, remap rules, `SOURCE_DATE_EPOCH`, expected sha).
6. **Verdict:** `SELFHOST_REPRO_GATE: PASS` only if 3+4 (macOS) and 5 (hermetic
   Linux) all reproduce their pinned shas; red otherwise.

## Order of work

- **DONE (unit 1, commit 9b7d3bf) — macOS remap + determinism.** `run_selfhost_repro_gate.sh`,
  5 checks, fail-closed. Reproducible sha `1db6a019` (stable across runs); negative control
  (no-remap build leaks 9 machine paths) confirms the check is load-bearing.
- **DONE (unit 2) — hermetic Linux lane.** Same script; builds the fixpoint source inside a
  pinned `rust:1.83-slim-bookworm` (digest `sha256:540c902e…`) twice → bit-identical ELF
  `80323a20…`. Runs when Docker is up; `ANUBIS_REPRO_DOCKER=1` makes it required.
  Gate now `SELFHOST_REPRO_GATE: PASS (6/6)`.
- **DONE (unit 3) — second-backend DDC capstone** (partial trusting-trust defense).
  `run_selfhost_ddc_gate.sh`, fail-closed, 20 checks. Second interpreter authored in C
  (`selfhost/backend_c/anubis_sh_interp_rt.c`), compiled with gcc-15 (non-LLVM); it emits
  byte-identical stage output (`ca310c4b…`) to the rustc lane for `anubis_sh.anb`, plus
  lex/parse/check agreement over the corpus. Negative control (one-token perturbation of the
  C interpreter) confirms the comparison is load-bearing; clang is refused fail-closed.
  Emits `ddc_manifest.json`. Chose emit-C-interpreter over a bytecode VM: it reuses the
  proven `anubis_sh_interp_rt.rs` semantics line-for-line (least new surface, and the
  byte-identical oracle validates the port), whereas a bytecode VM would add a second codegen
  to trust. ~660 LoC C, self-compile 1.0s (matches the Rust lane — the H5b copy-on-write
  append was ported to avoid O(n²)).
- **RESIDUAL (open `NEEDS-HUMAN`)** — the payload AST that both engines run is still derived
  through the Rust host (no C-native Anubis parser). A subversion baked into the shared AST
  *source* is inherited by both lanes. Closing it needs an independent, non-rustc Anubis
  parser for cB. Tracked in SELFHOST.md's DDC scope.

Related: [[anubis-host-runtime-rc-fix-2026-07-12]], SELFHOST.md (Diverse Double-Compiling).
