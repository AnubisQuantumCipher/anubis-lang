# A15 Gate 11 Metal Parity Report (2026-07-06)

**All core items: YES**

## 13 Sub-verdicts
- Apple Silicon Tier-2 Metal available: YES
- CPU lane observed: YES (logs + risc0_metadata.json)
- Metal-hybrid lane observed: YES (logs + risc0_metadata.json)
- CPU receipt verifies: YES (real risc0_zkvm::Receipt::verify)
- Metal receipt verifies: YES
- CPU/Metal ImageID parity: YES (identical real ImageID)
- CPU/Metal journal/output parity: YES — journals extracted via deserialization of receipt.bin (verify-receipt now writes sibling journal.bin); both lanes produce identical journal sha256 e8a4b2ee7ede79a3afb332b5b6cc3d952a65fd8cffb897f5d18016577c33d7cc
- external r0vm avoided: YES (in-process only)
- Metal evidence sealed: YES (full bundle with evidence.json, MANIFEST, sidecars including journals, parity_report)
- strict tamper detection: YES (5 patterns including journal.bin produced mechanical failure on verify_bundle)
- reference repo validation captured: YES (hash + manifest OK on reference evidence dir)
- unsupported hosted CI Metal claim avoided: YES (docs and reports state local Tier-2 only)
- Gate 11 final verdict: YES

## Evidence
- Corrected parity_report.json with matching journal shas from actual extraction.
- journals/ in this dir.
- evidence-gate11/ contains the sealed bundle.
- Tamper log in gate11_tamper.log (or session scratch).

Commands for A15 block were the plan-specified ones (safety, fmt, tests, clippy, rm + check_metal_parity.sh --require-metal, jq asserts, grep lane_observed, verify-receipt, schema, verify_bundle, tamper loops, reference validate/hash/verify).
