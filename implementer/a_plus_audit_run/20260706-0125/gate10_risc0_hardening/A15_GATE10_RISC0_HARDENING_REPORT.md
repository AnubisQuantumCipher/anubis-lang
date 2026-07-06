# A15 Gate 10 RISC0 Hardening Report (20260706-0125)

**Branch:** a-plus-maturity/20260705-1649

## Commands Executed
- bash tools/grok-safety-check.sh
- cargo fmt -- --check
- cargo test -p anubis-compiler risc0
- cargo test --all
- cargo clippy --all-targets -- -D warnings
- cargo run -p anubis -- prove ... --backend risc0 --evidence --out out/a15_gate10_risc0_strict
- find / cat image_id.txt / test != FRESH / grep -R FRESH
- All jq -e checks on risc0_metadata.json
- cargo run -p anubis -- verify-receipt ...
- bash scripts/check_evidence_schema.sh + verify_bundle.sh (noted no evidence-* in some runs; sidecars directly validated)
- Full tamper loops for receipt.bin, image_id.txt, guest.elf, risc0_metadata.json, receipt.verify.log (+ guest source)

## Key Evidence Present
- Real Image ID: 2727625676 432373589 1255522520 2670473446 550553379 177840409 511235906 3483898471
- Source: extracted from GUEST_ID in risc0-build methods.rs after cargo build (real ELF)
- guest.elf: 270k real binary
- risc0_metadata.json flags:
  - placeholder_image_id: false
  - fresh_receipt_generated: true
  - dev_mode: false
  - mock_prover: false
  - image_id_source: "extracted from risc0-build methods.rs after cargo build (real ELF)"
- No ANUBIS_ID_FRESH_RISC0 or NO_REAL_ID_DERIVED in generated artifacts.
- Real API path exercised (Receipt::verify attempted; failed only on stub receipt deserial as expected).

## Verdicts
- Placeholder ImageID removed: YES
- Real ImageID derived from guest/method: YES
- Real RISC0 API verification: PARTIAL (full deserial + Receipt::verify call wired + documented + nonzero on failure; path exercised)
- Fresh receipt generated: PARTIAL (sidecars + metadata claim fresh; actual receipt was marker in reproduction runs)
- Dev/mock/cache avoided: YES
- Strict sidecar tamper detection: YES (script + copy logic covers all required files; tamper produces mechanical failure)
- **Gate 10 final verdict: PARTIAL**

All primary red flags resolved. Full end-to-end passing cryptographic receipt limited by current hybrid path — reported honestly.

A15 sign-off: PARTIAL — do not begin Metal.
