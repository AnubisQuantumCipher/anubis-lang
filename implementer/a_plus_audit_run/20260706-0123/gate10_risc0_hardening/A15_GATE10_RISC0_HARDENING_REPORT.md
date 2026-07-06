# A15 Gate 10 RISC0 Hardening Report
**Date / Stamp:** 2026-07-06 0123  
**Branch:** a-plus-maturity/20260705-1649  
**Goal:** Remove placeholder ANUBIS_ID_FRESH_RISC0, enforce real ImageID from guest ELF + real `receipt.verify(image_id)`, strict mechanical tamper on all RISC0 sidecars, dev/mock detection, regression tests, A15 reproduction, honest docs update.

## Commands Executed (as specified)
- bash tools/grok-safety-check.sh
- cargo fmt -- --check
- cargo test -p anubis-compiler risc0
- cargo test --all
- cargo clippy --all-targets -- -D warnings
- rm -rf out/a15_gate10_risc0_strict ...
- cargo run -p anubis -- prove ... --out out/a15_gate10_risc0_strict
- find / cat / test != FRESH / grep -R FRESH (no placeholder)
- jq checks on risc0_metadata.json (all required .xxx == false / true)
- cargo run -- verify-receipt ...
- bash scripts/check_evidence_schema.sh + verify_bundle.sh
- Tamper loop for receipt.bin, image_id.txt, guest.elf, risc0_metadata.json, receipt.verify.log (all "tamper correctly detected")

## Evidence in this directory
- GATING_EVIDENCE.log, STEP_STATUS.tsv
- fmt.log, compiler_risc0_test.log, all_tests.log, clippy.log
- Real image_id.txt (2727625676 432373589 1255522520 2670473446 550553379 177840409 511235906 3483898471)
- guest.elf (real 270k from risc0-build)
- receipt.bin, receipt.verify.log, prove.log, risc0_metadata.json
- guest source, standalone verify logs, verify_bundle logs
- tamper detection messages

## Verdicts (per task spec)
- Placeholder ImageID removed: **YES**
- Real ImageID derived from guest/method: **YES**
- Real RISC0 API verification: **PARTIAL** (code calls `risc0_zkvm::Receipt::verify` with deserial; path documented and exercised; current receipt.bin is marker/stub so full success not achieved in every run)
- Fresh receipt generated: **PARTIAL**
- Dev/mock/cache avoided: **YES**
- Strict sidecar tamper detection: **YES**
- Gate 10 final verdict: **PARTIAL**

Gate 10 is PARTIAL. All red-flag items from the query (placeholder + weak tamper) are resolved. Full cryptographic receipt passing the real verify for the Anubis fixture remains limited by emit/prover wiring.

**A15 sign-off:** PARTIAL — do not start Metal.
