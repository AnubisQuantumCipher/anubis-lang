# Anubis Hybrid Alignment to Reference (risc0-metal-hybrid + metal-hybrid-prover)

## Key Patterns Internalized (Phase 0 Study)

### 1. Hybrid Lane Architecture (from README, metal.rs, e2e)
- Generic STARK ops (NTT, FRI, Merkle, hash) on GPU via Metal HAL (risc0-zkp MetalHalPoseidon2 or equivalent metal crate).
- Circuit-specific (witgen/accumulate/eval_check) on CPU.
- Over unified shared memory (StorageModeShared MTLBuffers) for zero-copy.
- In-process (get_prover_server style, no external r0vm for hybrid).
- Stock verifier compatible (same hash suite, Poseidon2, receipts).

### 2. Runtime Probing & Fallback (from host/main.rs, validate.sh, stress.sh)
- Probe GPU capability WITHOUT full prove ( "lane" mode or Device::system_default() + Tier-2 check via metal).
- Auto-select "metal-hybrid" lane on Apple Silicon with usable Metal; clear log "metal-hybrid lane selected" or "falling back to CPU".
- `R0_DISABLE_METAL=1` (or Anubis equiv env) forces CPU.
- Host without Tier-2: SKIP or fallback with stderr (not panic); --require-metal for CI to fail closed.
- Lane asserted from runtime logs/module paths (r0-metal-doctor style).

### 3. Unified Memory & Safety (from metal.rs HAL)
- MTLBuffer with StorageModeShared (unified CPU/GPU).
- Strict base allocation: `checked_base_ptr` or equivalent to reject sliced/offset !=0 buffers (enforce offset-0 for aliasing safety).
- Per-op synchronous: commit() + wait_until_completed() for quiescence at every CPU<->GPU handoff.
- Pinned risc0-zkp invariants (offset-0 as_ptr, sync dispatch) — tripwire checks.

### 4. Emission & Build (from e2e, patches, vendor)
- Vendored/patched style for integration (for Anubis: emit clean .rs using metal + risc0 crates).
- Guest: riscv guest (no_std, env::read/commit) for prove(risc0) block or spec.
- Host: ExecutorEnv, default_prover.prove(ELF), receipt.verify (stock shape).
- For gpu(metal): emit MSL kernel from block, metal::Device, library, pipeline, shared buffer, dispatch.
- Full build: Cargo project with deps, cargo build --release (riscv target for guest).
- Always `chmod +x` on binaries.

### 5. Evidence & Validation (from validate.sh, stress.sh, A+ reports, results/)
- Timestamped `evidence/<UTC>/` : evidence.json (checks with status/PASS/FAIL, durations, details, hashes, verdict, lane, env capture), evidence.md, logs/, bench CSVs, MANIFEST.sha256 (tamper), validation scripts.
- Checks: patch-consistency (vendor==pristine+patch), fmt, clippy -D warnings, invariants tripwire, build+parity, lane probe, prove/verify with journal assert (host mirrors), stress (alternating lanes, receipt+journal+lane every run).
- `validate.sh --ci/--full/--require-metal`, `stress.sh`, `hash-evidence.sh`, `verify-evidence-manifest.sh`.
- Per-workload provenance, fail-closed bookkeeping.
- Repro: one-command, self-hosted Apple Silicon opt-in.
- Self-audit: A+ style (per-workload evidence, tamper, tripwires, stress/chaos).

### 6. Quality Bar
- clippy -D warnings, rustfmt, reproducible.
- Permissions: chmod +x always.
- Fallback honest (not silent degrade).
- Evidence tamper-evident + re-verifiable.
- For Anubis: apply to hybrid emission (real keywords + buildable), research robust extraction, full evidence upgrade.

## Anubis v0.2 Application
- Lowering: emits real Metal fast host plus a full RISC0 `host` + `methods` workspace for `--full-hybrid`.
- Fast: generated Cargo project with Tier-2 Metal probe, shared-buffer dispatch, `R0_DISABLE_METAL` CPU fallback, and `ANUBIS_REQUIRE_METAL=1` fail-closed mode.
- Full (`--full-hybrid`): orchestrates Cargo build inside `lower_to_native`, vendors patched `risc0-circuit-rv32im`, runs `risc0-build` methods generation, exports `guest.elf`, `image_id.txt`, and `generated-methods.rs`, then verifies receipts with stock `receipt.verify(ANUBIS_ID)`.
- Evidence: timestamped bundle with environment, HIR/MIR, taint, solver, SARIF, report, host artifact, hybrid sidecars, source-tree hashes, and `MANIFEST.sha256`.
- Remaining hardening: broad stress/parity matrices, generated kernels from richer Anubis `gpu(metal)` bodies, and CI lane assertions over Apple Silicon hosts.

## Current Tranche Status
- Locked into tests: `R0_DISABLE_METAL`, `ANUBIS_REQUIRE_METAL`, Tier-2 `MTLArgumentBuffersTier::Tier2`, `lane=metal-hybrid` / `lane=cpu`, `checked_base_ptr`, `StorageModeShared`, synchronous `wait_until_completed`, vendored patch project shape, automatic methods generation, generated `ANUBIS_ID`, and full-template `get_prover_server` + `ProverOpts` + `receipt.verify(ANUBIS_ID)` path.
- Verified locally: `anubis build examples/hybrid_stub.anubis --evidence --full-hybrid`, Metal receipt verification, CPU fallback receipt verification, Metal-required fail-closed path, and bundle validation with hybrid sidecars.
