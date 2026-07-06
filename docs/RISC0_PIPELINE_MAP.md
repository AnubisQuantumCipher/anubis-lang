# Anubis RISC0 Pipeline Map (as of 2026-07-05)

## Current State Summary
RISC0 integration is **PARTIAL** (per prior audit). It exists primarily through the "hybrid" path (RISC0 + Metal for Apple Silicon). There is no standalone pure `--backend risc0` in the main CLI/backend yet. The hybrid uses vendored/patched risc0-circuit-rv32im to enable Metal proving on macOS.

Fresh end-to-end receipt from minimal Anubis source + independent verify was not demonstrated in previous timed audits (only shape, templates, and hybrid tests).

## Key Files and Modules
- **compiler/src/backends/native/hybrid/**:
  - mod.rs: main lowering logic, decides hybrid vs simple.
  - emit.rs, build.rs: emit host/guest projects.
  - templates/:
    - guest_Cargo.toml, guest_main.rs: RISC0 guest (risc0-zkvm 3.0.5).
    - methods_Cargo.toml, methods_build.rs, methods_lib.rs: risc0-build embed.
    - host_Cargo.full.toml, host_main_full.rs: full host with get_prover_server, ExecutorEnv, receipt.verify(ANUBIS_ID).
    - Cargo.full.toml: patches vendor/risc0-circuit-rv32im.
- **out/hybrid-test/anubis_out-real-hybrid/** and similar in out/audit/hybrid: generated examples of full projects.
- **vendor/risc0-circuit-rv32im/**: patched fork of risc0-circuit-rv32im 4.0.4 for Metal HAL + Apple Silicon.
- **tools/anubis/src/main.rs**: CLI supports --full-hybrid which triggers risc0 path in lower_to_native.
- **compiler/src/lib.rs**: tests for hybrid (hybrid_host_compiles..., hybrid_full_project..., receipt.verify in templates).
- **examples/hybrid_stub.anb**: triggers hybrid lowering.

## Guest Generation Path
- Anubis source -> parse/typecheck/lower_to_native(ir, out, name, full_hybrid=true)
- In hybrid emit: generates methods/guest/src/main.rs from template (uses risc0_zkvm::guest::env).
- risc0-build in methods/build.rs: `risc0_build::embed_methods()`.
- Produces guest ELF (via risc0-build) and ANUBIS_ELF / ANUBIS_ID constants.
- Current guest is minimal: reads input, does simple op, commits to journal (e.g. for hybrid test).

## Host/Prover Path
- Generated host/src/main.rs (full):
  - Uses risc0_zkvm::{get_prover_server, ExecutorEnv, ProverOpts, ...}
  - risc0_circuit_rv32im::prove::metal_lane_selected() for hybrid.
  - Builds ExecutorEnv with input.
  - prover = get_prover_server(ProverOpts::default()) or similar.
  - receipt = prover.prove(...).receipt
  - receipt.verify(ANUBIS_ID).expect(...)
  - Decodes journal.
- Dev mode: disabled in full template (features = ["disable-dev-mode"]).
- Prover can be local or (in theory) bonsai, but current is in-process.

## Verifier Path
- receipt.verify(ANUBIS_ID) — standard risc0-zkvm receipt verification.
- In hybrid tests: explicitly calls and asserts journal output.
- Standalone verify not yet exposed in top-level `anubis` CLI (only via generated host or direct).

## Receipt + Sidecars
- receipt: InnerReceipt or full Receipt from risc0.
- Sidecars in evidence (when --evidence / --full-hybrid):
  - guest.elf
  - image_id.txt (or ANUBIS_ID)
  - generated-methods.rs (contains ANUBIS_ELF, ANUBIS_ID)
- In evidence bundle: hashes of guest.elf, image_id.txt, generated-methods.rs.
- Current hybrid tests generate these and verify receipt.

## Tests
- compiler/src/lib.rs:
  - hybrid_host_compiles_and_dispatches
  - hybrid_full_project_emits_methods_vendor_patch_and_receipt_contract (checks ANUBIS_ID, receipt.verify, etc.)
  - hybrid_emission_snapshot, hybrid_fast_template_...
- Tests use include_str on examples/hybrid_stub.anb, lower, then cargo build on emitted project or direct prove.
- No pure "risc0" backend test fixture yet.

## Environmental Requirements & Limitations
- risc0-zkvm = "3.0.5", risc0-build, risc0-circuit-rv32im (vendored/patched).
- For full hybrid: risc0 tools, Rust, on Apple Silicon for Metal (or falls back?).
- Dev-mode: explicitly disabled in full path.
- Timeout: prior audit had timeout on heavy prove.
- Pure RISC0 (non-hybrid, no Metal) not exposed; proving is tied to hybrid templates.
- No top-level `anubis prove --backend risc0` that produces standalone receipt + verify without emitting full Cargo workspace.
- Receipt is "fresh" only when prove actually runs (not cached ELF).
- Prior audit PARTIAL: "shape/contract/test emission real; no receipt in timed run."

## Known Gaps for Gate 10
- No dedicated risc0 backend module (only hybrid).
- CLI does not expose clean `--backend risc0`.
- No minimal pure RISC0 guest that takes Anubis IR and produces verifiable receipt for simple program.
- Evidence sidecars for pure risc0 not standardized beyond hybrid.
- No `anubis verify-receipt` top-level command.
- Tamper on receipt/ELF/ID not explicitly tested in risc0-only path.
- Must generate *fresh* receipt (not reuse prebuilt).

## Versions (from Cargo.toml / generated)
- risc0-zkvm = "=3.0.5"
- risc0-build = "=3.0.5"
- risc0-circuit-rv32im = "=4.0.4" (vendored)

RISC0 here is **vendored + patched** for hybrid Metal support on macOS. Not plain upstream risc0 crate usage for pure ZK.

This map is based on code inspection, grep, find, and cargo test run on 2026-07-05.
