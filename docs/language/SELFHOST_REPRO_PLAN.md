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
subversion here too. Full trusting-trust closure needs a second *independent
backend* (emit-C or a bytecode VM), which is a separate multi-session capstone,
tracked in SELFHOST.md as the open `NEEDS-HUMAN`. Keep the two claims distinct.

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

- Land 1–4 first (macOS, no container) — small, uses existing normalizer, closes
  the "same-machine-only" gap in the current fixpoint. Ship as one green unit.
- Then 5 (hermetic Linux) — the externally-publishable claim. Ship as a second unit.
- Do NOT start the second-backend DDC capstone until both land.

Related: [[anubis-host-runtime-rc-fix-2026-07-12]], SELFHOST.md (trusting-trust residual).
