# A15 Full Language Audit (Hostile)

**Role:** independent red-team auditor (assume builder wrong until re-derived)  
**Date (UTC):** 2026-07-24  
**Branch:** `a-plus-maturity/20260705-1649`  
**Tree base HEAD:** `39a07ec827a6893e48d671f974adf9040896a368`  
**Working tree:** includes post-HEAD offensive T9 surfaces + isolation/stack fixes sealed by this audit  
**Audit dir:** `implementer/a_plus_audit_run/20260724-154145/full_language_audit/`

## Mission

Re-derive the A+ front door and load-bearing honesty boundaries. Do **not** accept Phase 9/10 closeout claims on authority alone. Surface mandatory failures; only upgrade the global **A+** label if the sealed suite is green **and** no open mandatory fail remains after remediation.

## Commands re-run (authoritative)

```bash
# 1. Front door (all 15 gates)
bash scripts/audit_a_plus.sh --out out/a_plus_a15_frontdoor_20260724-154145

# 2. Standalone offensive T9 (pre-suite seal)
bash scripts/run_offensive_platform_gate.sh --out out/a15_offensive_t9_20260724-152746

# 3. Host isolation recheck (must fail closed)
rm -f "$HOME/.anubis-vz-guest"
./target/release/anubis recon-scan --engage <lab> --host 127.0.0.1 --ports 22
# → ANUBIS_OFFENSIVE_HOST_FORBIDDEN (exit 1)
```

## Front-door result (re-derived)

| Gate | Verdict | Detail |
|------|---------|--------|
| G1_fmt | **PASS** | no formatting diffs |
| G2_clippy | **PASS** | zero warnings/errors |
| G3_test | **PASS** | **649** tests passed |
| G4_build_release | **PASS** | release binary built |
| G5_language_fixtures | **PASS** | 244/244 |
| G6_turing_core | **PASS** | 13/13 |
| G7_pca | **PASS** | 13/13 |
| G8_security_fixtures | **PASS** | Overall PASS |
| G9_poc_kit | **PASS** | 4/4 |
| G10_prove | **PASS** | 11/11 |
| G11_enum_match | **PASS** | clean |
| G12_for_in | **PASS** | clean |
| G13_lang_trio | **PASS** | clean |
| G14_offensive | **PASS** | **34/34**, `isolation=tart-disposable-guest` |
| G15_dogfood_feel | **PASS** | 8/8 |

**Overall: PASS (15/15 passed, 0 failed, 0 skipped)**  
Evidence: `gate_report.json`, `gate_log.txt`, `GATING_EVIDENCE.log` in this directory.

## Hostile findings (found → fixed → re-verified)

### F1 — Host isolation fail-open via stale guest marker (MANDATORY)

**Finding:** `$HOME/.anubis-vz-guest` was present on the **host**, so `in_vz_guest()` returned true and AOP actions (`recon-scan`, `listen`, …) executed on bare metal.

**Fix:** Host entrypoint of `scripts/run_offensive_platform_gate.sh` strips a stale host marker and clears guest-claiming env before `run_in_guest`. Operator host marker removed during audit.

**Re-verify:** `offensive-doctor` → `in_vz_guest: false`; host `recon-scan` / `listen` → `ANUBIS_OFFENSIVE_HOST_FORBIDDEN`.

### F2 — Guest stale binary skipped T9 CLI (MANDATORY for T9 honesty)

**Finding:** Guest hop only built when `target/release/anubis` was missing. A leftover guest binary without T9 subcommands caused 14/34 false fails (`unrecognized subcommand 'recon-scan'`, missing `--allow-research-inject`, etc.).

**Fix:** Guest remote always runs `cargo build --release -p anubis`.

**Re-verify:** standalone gate **34/34 PASS**; suite G14 **34/34 PASS** under `tart-disposable-guest`.

### F3 — Clippy -D warnings on new T9 modules (MANDATORY for G2)

**Finding:** `manual_pattern_char_comparison` in `attck.rs`; `manual_clamp` in `opsec.rs`.

**Fix:** `split(['/', ' ', ':'])`; `score.clamp(0, 100)`.

**Re-verify:** G2 PASS.

### F4 — Clap unit-test stack overflow (MANDATORY for G3)

**Finding:** Large `Commands` enum (T1–T9 + prove/vz) caused `Cli::try_parse_from` unit tests to overflow the default ~2MiB stack (`doctor_accepts_strict_metal_reference_flags`).

**Fix:** `RUST_MIN_STACK=16777216` in `.cargo/config.toml` and `scripts/audit_unified.sh` G3.

**Re-verify:** G3 PASS (**649** tests).

## What is NOT claimed (honest residuals)

These remain **scoped residuals**, not mandatory gate fails:

- Spec freeze §5: native SMT default flip; VZ hostname frame filter STAGED; hosted CI Metal *proving*; full author-diversity trusting-trust
- Gate 10 “fresh minimal receipt everywhere” remains partial in historical matrix language; cold-verify path is what G10 seals
- Infinite multi-party stranger coverage beyond Phase 9 witnesses
- “Trusting-trust closed” / backdoor-free absolute claim

## Phase 9 / Phase 10 posture (auditor view)

| Item | Status | Note |
|------|--------|------|
| Language Phase 9 (independent strangers) | **DONE** | `docs/language/phase9_independent_witness/` multi-party hashes |
| A+ Phase 9 (sealed final audit 2026-07-23) | **DONE** | historical 15/15 @ 20/20 G14 |
| A+ Phase 10 closeout docs | **DONE** | reports existed; A+ label was correctly withheld |
| **This A15 artifact** | **NEW / CLEAN** | front door re-derived 15/15; G14 upgraded to 34/34 |

## Grade

**A+ (front door + hostile audit clean after remediation)**

Mandatory gates: **no open mandatory failures** on the sealed re-run  
`out/a_plus_a15_frontdoor_20260724-154145`.

Hostile findings F1–F4 were **real** and would have blocked A+; each has a fix and a re-verify artifact in this package.

## How a stranger re-checks this A15

```bash
git checkout a-plus-maturity/20260705-1649   # tree with T9 + fixes
bash scripts/audit_a_plus.sh --out out/a15_stranger_repro
jq -e '.verdict=="PASS" and .pass==15 and .fail==0' out/a15_stranger_repro/gate_report.json
jq -e '.overall_verdict=="PASS" and .passed==34' out/a15_stranger_repro/g14_offensive/report.json
# Compare to this audit dir's gate_report.json / g14_offensive/report.json
```
