# Anubis Metal-Hybrid RISC0 Backend Contract (Gate 11)

**Date**: 2026-07-06 (TASK 1 mapping, pre-edit inspection)
**Branch**: a-plus-maturity/20260705-1649
**Purpose**: Canonical map of how Anubis integrates the risc0-metal-hybrid reference for CPU vs Metal-hybrid parity under `--backend risc0`. This is the source of truth for reproducible, observed-lane proving with real receipts.

## Inspection Commands Executed (verbatim)
- `bash tools/grok-safety-check.sh` → safety-check: OK
- `grep -R "metal-hybrid-prover\|risc0-circuit-rv32im\|patch.crates-io\|R0_DISABLE_METAL\|MTL\|Metal\|StorageModeShared\|get_prover_server\|ProverOpts\|receipt.verify" Cargo.toml .cargo compiler crates src scripts docs 2>/dev/null`
- `find /Users/sicarii/Desktop/metal-hybrid-prover -maxdepth 3 -type f | sort | head -300`
- `grep -R "StorageModeShared\|metal-hybrid\|R0_DISABLE_METAL\|lane=metal\|lane=cpu\|receipt.verify\|validate.sh\|stress.sh" /Users/sicarii/Desktop/metal-hybrid-prover 2>/dev/null | head -300`

## Where Anubis Currently Patches risc0-circuit-rv32im
- Root `Cargo.toml` (workspace):
  ```toml
  [patch.crates-io]
  risc0-circuit-rv32im = { path = "/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im" }
  ```
- Hard-wired in prove path for `--backend risc0` (tools/anubis/src/main.rs): the temporary `methods/Cargo.toml` written at prove time contains the identical `[patch.crates-io]` pointing at the vendored path.
- Hybrid templates also embed the patch:
  - `compiler/src/backends/native/hybrid/templates/Cargo.full.toml`
  - `compiler/src/backends/native/hybrid/templates/host_Cargo.full.toml`

Anubis **does** point at `/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im` (exact required envelope).

## Current RISC0 Prove Path (--backend risc0)
1. `anubis prove <input> --backend risc0 [--evidence] --out <dir>`
2. `lower_to_native(...)` (with full_hybrid logic for risc0).
3. For is_risc0: dynamically materializes a self-contained `methods/` crate in `<out>/methods/`:
   - Writes guest program (currently minimal read+mul+commit, aligned to risc0_receipt fixture shape).
   - `cargo build --release` (inside methods) → runs risc0-build → produces real guest ELF + ImageID (extracted from generated methods.rs or riscv-guest paths).
4. Copies guest.elf + image_id.txt + guest source to `<out>/backend/risc0/`.
5. Calls `run_risc0_proof_attempt`:
   - Spawns self as hidden `risc0-prove-child` (clean env: PATH/HOME/TMPDIR preserved; RISC0_DEV_MODE=0).
   - **Hard-coded today**: `.env("R0_DISABLE_METAL", "1")` (forces CPU lane for Gate 10 stability).
6. Child (`run_risc0_prove_child`):
   - `ExecutorEnv` (writes a u32 input).
   - `get_prover_server(&ProverOpts::default())`.
   - `prover.prove(env, &elf_bytes)?.receipt`.
   - `receipt_obj.verify(id_words)` (real risc0_zkvm::Receipt::verify).
   - Writes `receipt.bin` (bincode) and `receipt.verify.log`.
7. Evidence emission (when --evidence):
   - risc0 sidecars under `backend/risc0/` (guest.elf, image_id.txt, receipt.bin, risc0_metadata.json, receipt.verify.log, prove.log, guest/src/...).
   - Top-level manifest + hashes via `build_evidence_bundle` / `verify_bundle.sh`.
   - risc0_metadata.json currently minimal (written in path + tests).

**Child spawn / env isolation**: explicit `Command` with `env_clear()` + selective re-export. No global mutation outside the controlled prove entry. R0_DISABLE_METAL injected only here for the risc0 lane.

## Current R0_DISABLE_METAL=1 Behavior
- In prove attempt: always set to "1" (CPU forced).
- In child: if absent, force-set to "1".
- Templates (hybrid fast/full) probe it:
  ```rust
  if env::var("R0_DISABLE_METAL").is_ok() || tier != Tier2 { cpu lane } else { metal-hybrid }
  ```
- Logs emit "lane=cpu" or "lane=metal-hybrid".
- Reference (metal-hybrid-prover) treats it as the canonical CPU comparison lane.

## Current Lane Metadata
- Evidence `manifest.lane` (string) set in some paths (hybrid detection or mode).
- `risc0_metadata.json` (in backend/risc0/ and evidence copies) has:
  - verify_status, fresh_receipt_generated, mock_prover, dev_mode, cache_used, placeholder_image_id (or similar).
- **No full `metal_hybrid` object yet** (this is added in TASK 2).
- Lane observation currently inferred from env or hardcoded; Gate 11 requires mechanical log/runtime observation (not "host is Mac").

## Where Receipt Verify Occurs
- Real: inside child via `receipt.verify(derived ImageID)` (risc0_zkvm API).
- Standalone: `anubis verify-receipt --receipt <bin> --image-id <txt>` (uses same parse + verify path).
- Evidence checks: "risc0_receipt_verify" check looks for "PASSED" or "receipt.verify PASSED" in log.
- Tamper: `verify_bundle.sh` + schema require the log + binary presence + matching status.

## Where Guest ELF, ImageID, Receipt, Metadata, Logs Written
- During risc0 prove:
  - `<out>/backend/risc0/guest.elf`
  - `<out>/backend/risc0/image_id.txt` (space-separated words from risc0-build)
  - `<out>/backend/risc0/receipt.bin` (bincode serialized Receipt)
  - `<out>/backend/risc0/receipt.verify.log`
  - `<out>/backend/risc0/risc0_metadata.json` (status + flags)
  - `<out>/backend/risc0/prove.log` (partial)
  - `<out>/backend/risc0/guest/src/main.rs` (source copy)
- Also copied to top-level out/ for convenience in some paths.
- Evidence bundle copies sidecars under `backend/risc0/` inside the tar-ish evidence-*/ dir + hashes them in manifest.

## Exact Files in metal-hybrid Reference That Anubis Depends On
From find + grep on /Users/sicarii/Desktop/metal-hybrid-prover:
- `vendor/risc0-circuit-rv32im/` (the vendored+patched crate root; Cargo.toml version 4.0.4)
- `patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff`
- `scripts/validate.sh`, `stress.sh`, `hash-evidence.sh`, `verify-evidence-manifest.sh`
- `e2e/` (host + methods demonstrating get_prover_server + receipt.verify + lane)
- `m0-metalhal-smoke/`
- `results/*.json` (per-device)
- `README.md`, `A_PLUS_FINAL_REPORT.md`, `MISSION_LEDGER.md` (contract + results)
- Key symbols observed in Anubis templates/tests: `StorageModeShared`, `MTLArgumentBuffersTier::Tier2`, `metal_lane_selected()`, `R0_DISABLE_METAL`, lane logs.

Anubis depends on the **vendored patch path** and the **in-process proving contract** (get_prover_server + ProverOpts + real verify), not the full e2e binary of the reference.

## Exact Supported Version Envelope (enforced)
- risc0-zkvm = "3.0.5"
- risc0-zkp = "3.0.4" (transitive via patch)
- risc0-circuit-rv32im = "=4.0.4" (via [patch.crates-io] to vendored)
- risc0-build = "=3.0.5"
- In-process only: `get_prover_server(&ProverOpts::default())`, `ExecutorEnv`, `receipt.verify(ImageID)`
- `R0_DISABLE_METAL=1` forces CPU comparison lane.
- No external r0vm / server mode for this gate.

## What Can Be Verified on Hosted CI vs Local Apple Silicon
- **Hosted CI (Linux x86_64, no Apple Silicon Metal)**: CPU lane only (`R0_DISABLE_METAL=1` must be set; Metal path unavailable → lane_observed=cpu or unknown). Can verify receipt shape, ID derivation, verify call, evidence schema, tamper, but **cannot observe metal-hybrid lane**.
- **Local Apple Silicon (M-series with Tier-2)**: Full CPU + Metal-hybrid. Requires Tier-2 argument buffers (`MTLArgumentBuffersTier::Tier2`). Can observe both lanes mechanically from logs + runtime.
- **No third-party reproduction claim**: Gate 11 evidence is local A15 reproduction only. Hosted CI cannot claim Metal parity.

## Known Limitations (must be documented, never claimed around)
- Apple Silicon Tier-2 Metal only (argument buffers + unified memory StorageModeShared).
- External r0vm (or any out-of-process) **bypasses** the local patch and Metal HAL → disallowed for Gate 11.
- Not a full GPU port of the entire proving stack: only selected STARK ops (NTT/Merkle/etc.) via the hybrid HAL; many kernels remain CPU-side.
- Circuit-specific (rv32im) kernels in the patched risc0-circuit-rv32im.
- Lane must be **observed** (logs / explicit runtime metadata from metal_lane_selected or equivalent); host "is a Mac" inference is invalid and forces "unknown".
- If Tier-2 unavailable or probe fails → "unknown" or "cpu" only; Gate 11 cannot be YES.
- Performance numbers from reference repo are **reference evidence only**; this slice does not measure or claim speedup in Anubis.

## Current RISC0 / Hybrid Integration Points (Anubis source)
- `tools/anubis/src/main.rs`: Prove command, risc0 special case, child spawn, env injection (currently forces CPU), meta writing.
- `compiler/src/backends/native/hybrid/`: emit.rs (templates with full Metal probe + get_prover_server), mod.rs, templates/host_main*.rs (StorageModeShared, Tier2 check, R0_DISABLE, lane= strings).
- `compiler/src/lib.rs`: hybrid tests, risc0 sidecar checks, lane assertions.
- `compiler/src/evidence/mod.rs`: lane field, risc0 sidecar collection, risc0_metadata checks.
- `scripts/verify_bundle.sh`, `check_evidence_schema.sh`, gate10 reproduce scripts: require risc0_metadata.json + receipt.verify.log + tamper on 5 sidecars.
- Existing docs: RISC0_PIPELINE_MAP.md, RISC0_METAL_HYBRID_REFERENCE.md, hybrid-reference-patterns.md (partial alignment already present from Gate 10 work).

## Gate 11 Contract Reminder (non-negotiable)
same Anubis source
  -> CPU (R0_DISABLE_METAL=1) → valid receipt, observed "cpu"
  -> Metal-hybrid (Tier-2 present, no R0_DISABLE) → valid receipt, observed "metal-hybrid"
  -> same ImageID
  -> matching journals/outputs
  -> evidence records lane
  -> sidecars hashed
  -> tamper detection mechanical FAIL on any core sidecar or report

No marketing numbers. No third-party claim. No hosted-CI Metal YES.

This document is the pre-edit snapshot. Edits for Gate 11 (lane flag, metadata emission, parity checker, fixtures, A15 run) will update observed behavior while preserving the patch contract and real verify path.
