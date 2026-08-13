# Anubis Portability Audit (Gate 12 baseline)

**Date:** 2026-07-06 (on branch `a-plus-maturity/20260705-1649`)

**Purpose:** Enumerate all local-only / hardcoded assumptions before introducing portable backend configuration.

**Commands used to generate this audit:**
```bash
bash tools/grok-safety-check.sh   # OK

grep -R "/Users/sicarii\|metal-hybrid-prover\|Desktop\|TMPDIR\|R0_DISABLE_METAL\|APPLE_SILICON\|risc0-circuit-rv32im\|patch.crates-io" \
  Cargo.toml .cargo compiler tools scripts docs examples tests 2>/dev/null || true

cargo fmt --check   # clean
cargo test --all    # 37 passed (known upstream block v0.1.6 warning)
cargo clippy --all-targets --all-features -- -D warnings  # clean (same warning)
```

---

## 1. Hardcoded `/Users/sicarii/...` Paths

### Cargo / Patch (build of Anubis itself)
- `Cargo.toml:12`
  ```
  risc0-circuit-rv32im = { path = "/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im" }
  ```
- `compiler/src/backends/native/hybrid/templates/Cargo.full.toml:13` (same literal)
- Generated `methods/Cargo.toml` at prove time (written by `render_methods_cargo_toml` in tools/anubis/src/main.rs) also injects the literal.

### Compiler emission + vendoring copy
- `compiler/src/backends/native/hybrid/emit.rs:82`
  ```rust
  let vendored_src = Path::new("/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im");
  ```
- Same file also contains a long README string that mentions the path indirectly via context.

### CLI / Prove path (tools/anubis)
- `tools/anubis/src/main.rs`:
  - `metal_hybrid_reference_root()` → `PathBuf::from("/Users/sicarii/Desktop/metal-hybrid-prover")`
  - `risc0_vendor_patch_path()` → joins the above + `/vendor/risc0-circuit-rv32im`
  - Multiple places that write `risc0_metadata.json` hardcode or call the above:
    - `reference_path`, `vendored_patch_path`
  - Tests and assertions contain the literal Desktop path.
  - Comments throughout reference the Desktop tree as the "canonical" source.
- `tools/anubis/src/main.rs:1540` (test assertion)
- Child process comments (lines ~1401) explicitly call out the Desktop tree.

### Evidence validation
- `compiler/src/evidence/mod.rs:628`
  ```rust
  == Some("/Users/sicarii/Desktop/metal-hybrid-prover")
  ```
- Same for `vendored_patch_path` (line ~632).
- `risc0_metadata_check` performs exact string match.

### Compiler unit tests
- `compiler/src/lib.rs` contains several large JSON string literals with the exact Desktop paths inside `metal_hybrid` objects (for expected metadata).
- Multiple `assert!` strings also contain the Desktop path for hybrid workspace checks.

### Scripts
- `scripts/check_metal_parity.sh`:
  - Hard echoes the Desktop path into logs and `parity_report.json`.
  - Runs proves with the assumption that the path is present.

### Docs (intentional references)
- `docs/RISC0_METAL_HYBRID_REFERENCE.md` — entire contract section is written around the exact Desktop path.
- `docs/METAL_BACKEND.md`, `docs/METAL_BACKEND_PIPELINE_MAP.md` — reference the path as the source of truth.
- `ARCHITECTURE_MAP.md`, `docs/history/ANUBIS_REALITY_AUDIT.md`, `REPO` comments, goal patches, etc.

### Out/ and implementer/ artifacts
- Many historical bundles and reports contain the literal path (expected; they are evidence of past runs on this machine).

---

## 2. Environment Variables (current usage)

- `R0_DISABLE_METAL` — primary control for CPU vs Metal lane (set/cleared in parent, inherited or cleared for child). Observed by both parent logging and the patched crate's `metal_lane_selected()`.
- `TMPDIR`, `PATH`, `HOME` — explicitly re-injected into the clean-env child process.
- `ANUBIS_RISC0_PROVE_CHILD` — optional override for the child binary (already somewhat portable).
- No `ANUBIS_RISC0_METAL_REFERENCE` yet (this tranche introduces it).

---

## 3. Assumptions About Apple Silicon + Metal

- Code paths and tests assume `aarch64-apple-darwin` / arm64 Darwin.
- Tier-2 Metal detection is done via logs containing "Tier2" / "MTLArgumentBuffersTier" or successful metal-hybrid observation.
- `host_main*.rs` templates contain Metal-specific dispatch + `R0_DISABLE_METAL` fallback text.
- Many comments and the child prove logic assume unified memory + Metal HAL from the vendored crate.
- `gate11-metal-parity` and doctor will need `--require-metal` that can only be satisfied on real Tier-2 hardware.

---

## 4. RISC0 Install + Layout Assumptions

- Exact versions pinned: `risc0-zkvm = "=3.0.5"`, `risc0-zkp = "3.0.4"`, `risc0-circuit-rv32im = "=4.0.4"`.
- In-process proving only (`get_prover_server` + `ProverOpts`, never external `r0vm`).
- The patched crate must be at `<ref>/vendor/risc0-circuit-rv32im` and must export `risc0_circuit_rv32im::prove::metal_lane_selected()`.
- `risc0-build` embed + `methods` workspace layout is generated at prove time.
- `cargo metadata` is used at runtime to verify the patch is active.

---

## 5. Release vs Debug + Build Assumptions

- Release binary preferred for child (`ensure_risc0_prove_child_exe` tries release sibling).
- `cargo build --release -p anubis` is part of the "ready" workflow.
- Some hybrid tests force release profile.

---

## 6. Local Scratch / Out Dir Assumptions

- Default `--out out` (relative).
- Heavy use of `out/...` for all evidence, methods crates, receipts, parity reports, language fixtures, RC bundles.
- Temporary dirs used by hybrid lowering tests (`/private/var/folders/...`).
- No assumption that `out/` is committed (it is mostly gitignored or large artifacts).

---

## 7. Git Branch / State Assumptions

- All work is expected on `a-plus-maturity/20260705-1649`.
- Safety check and many audit docs record `pwd + branch`.
- Repro and A15 runs capture `git rev-parse` style data indirectly via logs.

---

## 8. Commands That Only Work on This Machine (Today)

- Any `anubis prove --backend risc0 --lane metal-hybrid` without the Desktop reference present.
- `anubis doctor` (current stub) + future expanded version that checks the reference.
- Full hybrid vendoring copy (`emit_full...`) fails if the Desktop tree is missing the expected Metal HAL files.
- `check_metal_parity.sh --require-metal` on non-Apple-Silicon or without Tier-2.
- Direct use of the vendored crate source for copy or patch verification.

---

## 9. Other Local-Only Artifacts

- `vendor/risc0-circuit-rv32im/` inside the repo (a copy of the patched crate from the Desktop tree).
- Numerous `out/` subdirs with concrete receipts, journals, methods builds from this host.
- Historical A15 and gate runs under `implementer/a_plus_audit_run/`.

---

## Summary Classification

| Category                    | Portable Today? | Action Required |
|-----------------------------|-----------------|-----------------|
| Reference path (Metal hybrid) | No             | Task 2 config resolution |
| Evidence recording + validation | No (string match) | Task 2 + evidence updates |
| Doctor | Minimal        | Task 3 (major) |
| CLI surface + errors | Partial        | Task 4 |
| RC packaging script | Does not exist | Task 5 |
| Install / version story | Partial        | Task 6 |
| Claim / trust docs | Out of date    | Task 7 |

**Next:** After this document is committed, implement the portable resolution order so that `--metal-reference`, env, `Anubis.toml`, and default can all drive the above surfaces while recording the source of truth in evidence.

All sealed gates (4/5/7/8/10/11) and 25/25 language results must continue to pass after the changes.
