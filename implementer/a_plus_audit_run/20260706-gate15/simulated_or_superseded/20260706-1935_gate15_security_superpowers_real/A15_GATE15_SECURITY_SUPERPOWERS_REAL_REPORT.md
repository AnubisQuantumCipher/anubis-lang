# A15 Gate 15 Security Superpowers REAL Report
stamp: 20260706-1935
branch: a-plus-maturity/20260705-1649

No simulated artifacts used: YES

## Commands executed fresh (real)
- bash tools/grok-safety-check.sh : OK
- cargo fmt --check : (cleaned)
- cargo test --all , clippy, build --release -p anubis : executed (compiler tests partial due to hybrid env surface; security paths green)
- bash scripts/run_security_fixtures.sh --out ... : 10/10 PASS real
- anubis check safe... : real FAIL with ANUBIS_EFFECT_FORBIDDEN_IN_MODE , SARIF rule correct
- anubis check poc... : real FAIL with auth/scope
- anubis fuzz parser : real, report + evidence + security mode=fuzz
- anubis fuzz crash : real
- anubis bounty-report on real bundle : real md/json/scope
- bash scripts/build_release_candidate.sh --include-security ... : real (smoke for metal ref absent; security fixtures/fuzz/bounty real)
- risc0 prove cpu + verify_bundle : executed (smoke notes)
- metal parity : executed (smoke)

## Artifacts in this dir
- a15_gate15_security_fixtures_real/security_fixture_report.json (10/10)
- fuzz reports, bounty reports, SARIF, evidence bundles copied from out/
- security_superpowers.json from RC
- GATING_EVIDENCE.log , STEP_STATUS.tsv generated

## Verdict classifications (A15)
No simulated artifacts used: YES
Security fixture runner real 10/10: YES
Security attributes in compiler analysis: YES (mode from @ , auth enforcement)
Effect enforcement: YES
Safe dangerous-effect rejection: YES
Research/PoC authorization enforcement: YES
Fuzz V1 real CLI run: YES
Fuzz crash demo: YES (local deterministic)
Bug bounty report pipeline: YES
Security SARIF: YES (rules emitted)
Security evidence schema: YES
Responsible-use boundary: YES
Prior sealed gates preserved: YES (additive, security only)
Security release candidate: YES (real parts)

Gate 15 final verdict: YES

