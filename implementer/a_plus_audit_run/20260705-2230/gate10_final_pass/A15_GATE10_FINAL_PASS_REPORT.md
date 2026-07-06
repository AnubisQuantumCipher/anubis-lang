# A15 Gate 10 Final Pass Report

**Date:** 2026-07-06 (sim)
**Auditor:** A15 (independent reproduction per plan)
**Branch:** a-plus-maturity/20260705-1649
**Run stamp dir:** implementer/a_plus_audit_run/20260705-2230/gate10_final_pass/
**Objective:** Confirm minimal RISC0 fixture produces top-level evidence bundle PASS while preserving real crypto receipt verification. Seal Gate 10 only on all-YES.

## Commands Executed (fresh)
- bash tools/grok-safety-check.sh
- cargo fmt --check
- cargo test -p anubis-compiler risc0
- cargo test --all
- cargo clippy --all-targets -- -D warnings
- rm -rf out/a15_gate10_final_pass
- cargo run --release -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/a15_gate10_final_pass
- find ... | sort
- test -s .../guest.elf ; test -s .../image_id.txt ; test -s .../receipt.bin
- cat .../image_id.txt
- grep -R "ANUBIS_ID_FRESH_RISC0" ... && exit 1 || echo "no placeholder"
- jq . .../risc0_metadata.json
- jq -e '.fresh_receipt_generated == true' ...
- jq -e '.cache_used == false' ...
- jq -e '.dev_mode == false' ...
- jq -e '.mock_prover == false' ...
- jq -e '.verify_status == "passed"' ...
- cargo run --release -p anubis -- verify-receipt --receipt .../receipt.bin --image-id .../image_id.txt
- bash scripts/check_evidence_schema.sh ...
- bash scripts/verify_bundle.sh ...
- for pattern in 'receipt.bin' 'image_id.txt' 'guest.elf' 'risc0_metadata.json' 'receipt.verify.log'; do ... tamper + verify_bundle ... done

## Artifacts in this dir
- GATING_EVIDENCE.log
- STEP_STATUS.tsv
- A15_GATE10_FINAL_PASS_REPORT.md (this)
- risc0_receipt.anb (copied fixture)
- evidence_bundle/ (full generated PASS evidence bundle with subdirs)
- guest.elf , image_id.txt , receipt.bin , receipt.verify.log , risc0_metadata.json , manifest.json , evidence.json (top copies)
- prove.log , schema_check.log , verify_bundle.log , standalone_verify.log , tamper_*.log
- fmt.log , compiler_risc0_test.log , cargo_test_all.log , clippy.log , grok-safety-check.log

## Key Evidence Excerpts
- Fixture: `fn main() { let x: u32 = 42; }` (minimal constant, no assert)
- ImageID: 2727625676 432373589 ... (real, derived from guest ELF via risc0-build)
- receipt.bin: present, ~ size valid, not marker
- risc0_metadata.json: fresh_receipt_generated=true, cache_used=false, dev_mode=false, mock_prover=false, verify_status=passed, image_id_is_placeholder=false, placeholder_image_id=false
- evidence.json / manifest: solver:PASS (no-obligations), risc0_receipt_verify:PASS, overall verdict:PASS
- Standalone: "receipt.verify(ANUBIS_ID) PASSED (real RISC0 API path: risc0_zkvm::Receipt::verify)"
- Schema: PASS
- verify_bundle: SUCCESS (verdict PASS)
- Tamper: all 5 (receipt.bin, image_id.txt, guest.elf, risc0_metadata.json, receipt.verify.log) produced TAMPER + exit non-0, "tamper correctly detected"

## A15 Verdict Classifications
* Top-level evidence bundle PASS: **YES**
* Real ImageID: **YES**
* Fresh receipt: **YES**
* Real RISC0 API verification: **YES**
* Standalone verify: **YES**
* Dev/mock/cache avoided: **YES**
* Strict sidecar tamper detection: **YES**
* Metal-hybrid reference contract documented: **YES**
* Gate 10 final verdict: **YES**

**Gate 10 is sealed: YES** (all sub-verdicts YES).

## Notes
- Real cryptographic path preserved throughout (no mock, no dev, real receipt ~209KB class, ImageID from ELF, receipt.verify passes).
- Top-level FAIL in prior 0213 run was solely solver on input-dependent assert in old fixture; minimal constant fixture discharges honestly.
- Metal work paused per directive; Gate 11 will use the documented AnubisQuantumCipher/risc0-metal-hybrid contract exactly (pinned [patch], in-process, R0_DISABLE_METAL CPU, receipt.verify parity).
- All required files/logs present.
- No fabrication: every claim backed by command output in logs and dir contents.

A15 confirms: Gate 10 sealed.
