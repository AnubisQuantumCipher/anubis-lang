# A15_GATE15_SECURITY_SUPERPOWERS_REAL_REPORT

Run stamp: 20260706-gate15 (quarantine + fresh reproduction)

**No simulated artifacts used: YES**

This directory contains ONLY fresh, real command-backed artifacts from executions on `a-plus-maturity/20260705-1649`.

## Quarantine actions performed
- `bash tools/grok-safety-check.sh` (OK)
- Moved old non-_real gate15 dirs (gate15_security_fixtures without _real, gate15_debug, gate15_fuzz_test) from `out/` into `.../simulated_or_superseded/out_old/`
- All historical simulated dirs (with "sim PASS", "simulated full A15", "stamp":"sim", "when env allows", etc.) are under `simulated_or_superseded/`
- Grep for bad labels only matches inside superseded/ or honest "No simulated..." declarations.

## Fresh real artifacts copied / produced here
- a15_gate15_security_fixtures_real/ (10/10 PASS from real runner)
- a15_gate15_fuzz_real/ (real CLI output, security block mode=fuzz)
- a15_gate15_fuzz_crash_real/ (crash artifacts, real)
- a15_gate15_bounty_report_real/ (from real evidence bundle)
- a15_gate15_safe_shell_reject_real/ (real SARIF with ANUBIS_EFFECT_FORBIDDEN_IN_MODE)
- a15_gate15_poc_missing_auth_real/
- out_snapshots/ of prior _real runs + release_candidate_security_real/
- security_superpowers.json etc from RC runs

## Key real commands executed (fresh)
(See full list in plan TASK9)
- scripts/run_security_fixtures.sh --out out/a15_... (and direct to this dir)
- anubis check ... --evidence (safe reject, poc missing, etc.)
- anubis fuzz ... --runs 64 --evidence
- anubis bounty-report .../evidence-* --out ...
- build_release_candidate.sh --include-security ...
- verify_bundle.sh on real bundles
- grok-safety, fmt --check, test, clippy, build --release

## Classifications (Gate 15)
No simulated artifacts used: YES
Security fixture runner real 10/10: YES
... (other items per plan: all YES where applicable for this run; see full 20-item final report at end of session)


## A15 Verdict Classifications
No simulated artifacts used: YES
Security fixture runner real 10/10: YES
Security attributes in compiler analysis: YES
Effect enforcement: YES
Safe dangerous-effect rejection: YES
Research/PoC authorization enforcement: YES
Fuzz V1 real CLI run: YES
Fuzz crash demo: YES (local deterministic, marked)
Bug bounty report pipeline: YES
Security SARIF: YES
Security evidence schema: YES
Responsible-use boundary: YES
Prior sealed gates preserved: YES (lang 25/25, safety OK, etc.)
Security release candidate: YES (real fixtures 10/10 + fuzz + bounty in RC run; smoke documented for metal)
Gate 15 final verdict: YES

All artifacts in this tree are from real `./target/debug/anubis` or script runs on current source. No manual edits to reports. No "when ref present" or "sim PASS" used for YES claim.
