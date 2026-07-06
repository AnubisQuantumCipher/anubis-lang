# A15 Hostile Reproduction Report — Gate 10 RISC0 Hardening
**Run stamp:** 20260706-0120  
**Branch:** a-plus-maturity/20260705-1649  
**Auditor:** A15 (independent reproduction pass)  
**Objective:** Harden Gate 10 (RISC0 fresh receipt + strict sidecar tamper) from PARTIAL+ to unambiguous YES or honest PARTIAL.

## Reproduction Commands Executed (verbatim per task)
- bash tools/grok-safety-check.sh
- cargo fmt -- --check
- cargo test -p anubis-compiler risc0
- cargo test --all
- cargo clippy --all-targets -- -D warnings
- rm -rf out/a15_gate10_risc0_strict out/a15_gate10_risc0_strict_tampered
- cargo run -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/a15_gate10_risc0_strict
- find out/a15_gate10_risc0_strict -maxdepth 6 -type f | sort
- cat out/.../backend/risc0/image_id.txt
- test "..." != "ANUBIS_ID_FRESH_RISC0"
- grep -R "ANUBIS_ID_FRESH_RISC0" ... && exit 1 || echo "no placeholder"
- jq . .../risc0_metadata.json
- jq -e '.placeholder_image_id == false' ...
- jq -e '.fresh_receipt_generated == true' ...
- jq -e '.cache_used == false' ...
- jq -e '.dev_mode == false' ...
- jq -e '.mock_prover == false' ...
- cargo run -p anubis -- verify-receipt --receipt .../receipt.bin --image-id .../image_id.txt
- bash scripts/check_evidence_schema.sh ...
- bash scripts/verify_bundle.sh ...
- (tamper loop for receipt.bin, image_id.txt, guest.elf, risc0_metadata.json, receipt.verify.log)

## Generated Artifacts (in this dir + out/a15...)
- GATING_EVIDENCE.log
- STEP_STATUS.tsv
- fmt.log / compiler_risc0_test.log / all_tests.log / clippy.log
- guest_source.anb (the .anb fixture)
- guest.elf (real from risc0-build riscv-guest bin)
- image_id.txt (real 8-word)
- receipt.bin
- receipt.verify.log
- prove.log
- risc0_metadata.json (all required flags)
- standalone_verify_attempt.log (real API exercised)
- verify_bundle.log / check_schema.log

## Key Evidence
**Image ID (real, derived):**
```
2727625676 432373589 1255522520 2670473446 550553379 177840409 511235906 3483898471
```
- Source: extracted from `.../out/methods.rs` after `cargo build` inside the isolated risc0 methods crate (GUEST_ID produced by risc0-build 3.0.5 from the actual guest ELF).
- `image_id_is_placeholder: false`
- No `ANUBIS_ID_FRESH_RISC0` or `NO_REAL_ID_DERIVED` in generated evidence.

**risc0_metadata.json (selected):**
- `image_id_source`: "extracted from risc0-build methods.rs after cargo build (real ELF)"
- `placeholder_image_id`: false
- `fresh_receipt_generated`: true
- `cache_used`: false
- `dev_mode`: false
- `mock_prover`: false

**Guest ELF:** 270kB real riscv32im binary from the build (not stub).

## Verdicts (A15 classification)
- Placeholder ImageID removed: **YES**
- Real ImageID derived from guest/method: **YES**
- Real RISC0 API verification: **PARTIAL** (full `risc0_zkvm::Receipt::verify(image_id)` call path is implemented with exact deserial + call + error propagation + comments; receipt.bin in this reproduction is minimal because full prover execution in the emit path produced limited seal in the env — the API contract is exercised and rejects bad/placeholder IDs)
- Fresh receipt generated: **PARTIAL** (sidecars + metadata claim fresh; cryptographic receipt size limited)
- Dev/mock/cache avoided: **YES**
- Strict sidecar tamper detection: **YES** (verify_bundle.sh + copy_hybrid_sidecars now cover guest.elf, guest source, image_id.txt, receipt.bin, risc0_metadata.json, *.log; MANIFEST includes risc0_* hashes; tamper append causes mechanical nonzero + "TAMPER" on hash mismatch for all patterns)
- Gate 10 final verdict: **PARTIAL**

Gate 10 may only be YES if *all* are YES. Here the core red flags (placeholder + weak tamper script) are sealed, but end-to-end real RISC0 receipt for the Anubis semantics remains wiring-limited in the hybrid emit/prove slice. Honest PARTIAL.

## Commits in this slice (summary)
- gate10-hardening: remove placeholder image id (generalized extract, real GUEST_ID path)
- gate10-hardening: enforce real receipt verification api (deserial + Receipt::verify, comments, nonzero)
- gate10-hardening: strict risc0 sidecar tamper detection (enhanced copy + script)
- gate10-hardening: detect dev mock cache and placeholder proof modes (metadata + flags + rejection)
- gate10-hardening: add strict risc0 regression tests (targeted + evidence)
- gate10-hardening: update risc0 truth boundaries (docs + matrix)

## Recommendation
Carry Gate 10 as PARTIAL. Do not promote to A+ or REAL until a full fresh cryptographic receipt (seal that passes .verify for the emitted ELF) is produced and A15 re-runs with success on verify-receipt. Metal remains paused.

**A15 sign-off:** 2026-07-06
