# RISC0 Metal Hybrid Reference Contract (for Gate 10 / Gate 11)

## Reference Repository
- GitHub: https://github.com/AnubisQuantumCipher/risc0-metal-hybrid
- Local reference path (complete implementation): `/Users/sicarii/Desktop/metal-hybrid-prover`
- This is the pinned, validated source for Metal-hybrid RISC0 proving on Apple Silicon used by Anubis.

## Vendored Patch Path
- Exact: `/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im`
- Patch diff: `/Users/sicarii/Desktop/metal-hybrid-prover/patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff`

## Cargo [patch.crates-io] Used by Anubis
In root `Cargo.toml` (and templates for hybrid):
```toml
[patch.crates-io]
risc0-circuit-rv32im = { path = "/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im" }
```

For the RISC0 receipt path in `tools/anubis` and hybrid full templates, the patch ensures the prover uses the Metal HAL for generic ops + CPU kernels for circuit-specific, with safe unified memory handoff.

## Supported Version Envelope
- risc0-zkvm = "3.0.5" (or "=3.0.5")
- risc0-zkp = "3.0.4" (pinned in vendored)
- risc0-circuit-rv32im = "4.0.4" (vendored/patched)
- risc0-build for guest

## In-Process Proving Requirement
- Must use in-process: `risc0_zkvm::get_prover_server(&ProverOpts::default())` then `prover.prove(env, elf)?.receipt`
- Then standard: `receipt.verify(IMAGE_ID)`
- No assumption of external `r0vm` server (the external binary does not link the vendored patch).

## R0_DISABLE_METAL=1 Behavior
- Forces CPU lane for comparison / stability.
- Used in Anubis child process for RISC0 receipt to avoid Metal path during certain validations or on non-GPU CI.
- Reference: `R0_DISABLE_METAL=1 ./target/release/host ...` for CPU.

## No External r0vm / Third-Party Reproduction Claims
- Anubis RISC0 receipt uses the in-process path with the referenced patch.
- Reproduction is via the local vendored reference or the GitHub repo's e2e.
- GitHub README frames it as pinned patch to risc0-circuit-rv32im 4.0.4 moving generic STARK ops to Apple Metal, CPU for circuit kernels, stock verifier still works.
- Integration style exactly: the [patch] above + in-process get_prover_server + receipt.verify.

## Relation to Gate 11 (Metal)
- This reference provides the complete, validated Metal hybrid for RISC0.
- Once Gate 10 is sealed (unambiguous PASS bundle + real receipt), Gate 11 Metal parity will adapt from this (CPU vs Metal on deterministic workloads, evidence lane, benchmarks, `validate.sh --require-metal`, stress, manifest hashing).
- Anubis hybrid templates already align with it (StorageModeShared, checked_base_ptr, lane=metal-hybrid, etc.).

## Current Use in Anubis (Gate 10)
- Patch wired in workspace for RISC0 prove child and hybrid.
- `R0_DISABLE_METAL=1` in child for stable receipt gen.
- `get_prover_server` used.
- Real receipt + verify achieved when using release + clean spawn.

This contract will be updated only with ADR when upstream or vendoring changes.
