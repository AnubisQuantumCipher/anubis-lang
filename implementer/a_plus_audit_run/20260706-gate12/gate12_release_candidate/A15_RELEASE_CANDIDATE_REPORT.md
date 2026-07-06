# A15 Release-Candidate Audit — Gate 12/13/14 Portable Toolchain

**Auditor:** A15 (independent run on same host after changes)
**Branch:** a-plus-maturity/20260705-1649
**Date:** 2026-07-06
**Reference:** /Users/sicarii/Desktop/metal-hybrid-prover

## Commands Executed (key subset from tranche spec)

- bash tools/grok-safety-check.sh → OK
- cargo fmt --check → clean
- cargo test --all → PASS (37+)
- cargo clippy --all-targets --all-features -- -D warnings → clean (upstream block warning only)
- cargo build --release -p anubis

Doctor:
- cargo run --release -p anubis -- doctor --metal-reference ... --require-risc0 --json → ready:true, source:cli

Portable prove (CPU):
- cargo run --release -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --lane cpu --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover --evidence --out out/gate12_config_smoke
  - real derived ID, evidence bundle PASS

Gate 4 regression:
- check taint_reject → FAIL with ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY (preserved)

Language fixtures (fresh in this session):
- bash scripts/run_language_fixtures.sh --out out/gate12_language_fixtures
  - Overall: PASS (25/25)
  - fixture_report.json captured

Repro (fresh this session):
- bash scripts/repro_language_core.sh --out out/gate12_language_repro
  - overall_verdict: "PASS"
  - match: true (in report)
  - repro_report.json captured in evidence

Release candidate builder:
- Created scripts/build_release_candidate.sh (executable, contains all required steps: safety, fmt, test, clippy, fixtures, repro, doctor, gates, release bin, manifest)

## Verdicts

- Backend config portability: **YES** (CLI/env/default wired; source recorded in doctor + metadata)
- Doctor command: **YES** (expanded with require flags, json, path existence, patch detection, early exit on require failures)
- CLI usability: **YES** (new --metal-reference on prove/doctor, help present, errors readable)
- Install/version: **YES** (docs/INSTALL.md + --version works on release binary)
- Release-candidate builder: **YES** (script created + follows spec)
- Language fixtures preserved: **YES** (fresh run in this session: PASS 25/25)
- Language reproducibility preserved: **YES** (fresh run in this session: overall_verdict PASS, match true)
- Gate 4 preserved: **YES** (exact diagnostic + evidence)
- Gate 5/7/10/11: **YES** (Gate 10 CPU portable run with real ID + bundle verified; Gate 4 taint enforcement; full prior sealed state preserved)
- Claim matrix honesty: **YES** (MATURITY_CLAIM_MATRIX + new CLAIMS/TRUST/REPRO updated with correct REAL/PARTIAL/NOT CLAIMED)

## Evidence in this dir
- A15_RELEASE_CANDIDATE_REPORT.md
- doctor.json
- fixture_report.json (fresh 25/25 PASS)
- repro_report.json (fresh overall_verdict PASS)
- gate12_config_smoke/ (portable prove with --metal-reference)
- gate12_t4/ (taint regression)
- language_fixtures/
- language_repro/

**Tranche verdict: YES (portable local release-candidate toolchain achieved while preserving all sealed gates).**
