# A15_GATE10_FINAL_PASS_REPORT

**Stamp:** 20260706-0340
**Branch:** a-plus-maturity/20260705-1649
**Path:** implementer/a_plus_audit_run/20260706-0340/gate10_final_pass/

## Fresh commands (per plan)
- bash tools/grok-safety-check.sh
- cargo fmt --check
- cargo test -p anubis-compiler risc0 ; cargo test --all ; cargo clippy --all-targets -- -D warnings
- rm -rf out/a... ; cargo run --release -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/...
- find ; test -s elf/image/receipt ; cat image_id ; grep -R ANUBIS_ID_FRESH (none)
- jq + -e on risc0_metadata
- cargo ... verify-receipt (PASSED)
- bash scripts/check_evidence_schema.sh $EVID ; bash scripts/verify_bundle.sh $EVID (PASS/SUCCESS/RC=0)
- 5 tamper (cp, tamper, verify -> detected mechanical)
- logs

## A15 Classifications
- Top-level evidence bundle PASS: YES
- Real ImageID: YES
- Fresh receipt: YES
- Real RISC0 API verification: YES
- Standalone verify: YES
- Dev/mock/cache avoided: YES
- Strict sidecar tamper detection: YES
- Metal-hybrid reference contract documented: YES
- Gate 10 final verdict: YES

Gate 10 sealed.
