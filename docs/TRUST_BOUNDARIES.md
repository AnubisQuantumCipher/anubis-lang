# Anubis Trust Boundaries (Release-Candidate)

## What the local toolchain trusts

- The operator's copy of `/Users/sicarii/Desktop/metal-hybrid-prover` (or equivalent resolved reference) for the patched `risc0-circuit-rv32im` Metal HAL.
- The RISC0 3.0.5 in-process proving stack + stock `receipt.verify`.
- The Anubis compiler + evidence layer for taint, declass, symbolic, and bundle construction.
- Apple Silicon unified memory + Tier-2 Metal for the observed lane.

## What is NOT trusted / claimed

- Any other machine's build of the reference tree.
- External `r0vm` or cloud provers.
- Third-party reproduction of Metal parity or full end-to-end receipts without the exact pinned reference + in-process path.
- General language completeness or production readiness beyond the sealed gates + language-core fixtures.

## Evidence as the source of truth

Every important artifact (receipt, bundle, parity report, doctor.json, fixture_report) is hashed and included in a `MANIFEST.sha256` or the bundle's own manifest. Tamper detection is enforced by the verify tools.

See also `docs/REPRODUCIBILITY.md`.
