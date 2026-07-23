# Anubis A+ Roadmap

**Baseline (2026-07-05):** C-grade real prototype per ANUBIS_REALITY_AUDIT.md — the *starting* point, not the current state.

**Live status is well past this baseline.** The living language tracker is [`docs/language/ROADMAP.md`](docs/language/ROADMAP.md), where phases 0–10 are already documented at the language/freeze level. For this repo-governing A+ roadmap, **Phase 9 is DONE on Thursday, July 23, 2026**: `bash scripts/audit_a_plus.sh --out out/a_plus_phase9_final_rerun_20260723` returned `PASS (15/15 passed, 0 failed, 0 skipped)`, with G14 executed in a disposable tart guest. **Phase 10 is also DONE the same day** via [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md) and [`A_PLUS_CLOSEOUT.md`](A_PLUS_CLOSEOUT.md). **A+ is still NOT CLAIMED** on this checkout because no current `implementer/a_plus_audit_run/*/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` artifact exists in tree.

**Target:** A+ when all 15 gates in A_PLUS_ACCEPTANCE_CRITERIA.md pass + A15 hostile audit clean.

## Phase 0 (done in plan mode)
- Recon, audits read, plan written + approved.
- Git baseline on `a-plus-maturity/20260705-1649`.
- Initial safety + docs skeleton.

## Phase 1 — Arena + Safety + Cartography (A0/A1/A13)
- Worktree dir + safety scripts active.
- ARCHITECTURE_MAP.md (exact gate locations, dataflow).
- tests/a_plus/ skeleton + CI smoke.

## Phase 2 — A+ Criteria Frozen (A0/A14)
- A_PLUS_ACCEPTANCE_CRITERIA.md, ROADMAP, MATURITY_CLAIM_MATRIX, AGENTS.md.

## Phase 3 — Fix Old Weaknesses (priority, A4 A5 A7 A9 A6)
3.1 Brittle assume gate scoped/removed for safe taint paths.
3.2 Hard taint-to-sink + declassify(policy, reason) enforcement.
3.3 Solver fidelity + replay.
3.4 Fresh RISC0 receipt end-to-end.
3.5 Evidence schema v1 + validator + tamper always fails.

**Gate after this phase:** old audit failing cases now behave correctly with evidence.

## Phase 4 — Language Maturity (A2 A3 A10 A11 A7)
- Full required surface + 10+ examples.
- General lowering.
- Minimum stdlib with effects.
- Full CLI surface + Anubis.toml.

## Phase 5 — Backends (A8 A9 A7)
- Metal parity on 3 workloads + fallback + disclosed benches.
- RISC0 verified receipt.
- Repro guarantees.

## Phase 6 — Tooling & Quality (A11-13)
- `anubis doctor`, portable backend config, release-candidate evidence builder (Gate 12/13/14 tranche).
- LSP or honest diagnostic server (future).
- Expanded CI + fuzz/golden/property.
- All old failure modes impossible.

## Phase 7 — Docs Honesty (A14)
- All required docs with REAL/PARTIAL/... labels.
- CLAIMS.md single source of truth.

## Phase 8 — Hostile Re-audits (A15)
- After 3.x, after 4, after 5, before final.
- A_PLUS_REALITY_AUDIT.md + CLAIM_MATRIX + STEP_STATUS.

## Phase 9 — Sealed Final Audit
**DONE (Thursday, July 23, 2026).**
- `bash scripts/audit_a_plus.sh --out out/a_plus_phase9_final_rerun_20260723` → `PASS (15/15 passed, 0 failed, 0 skipped)`.
- Exact verdict block: [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md)
- Sealed evidence mirror: `implementer/a_plus_audit_run/20260723-152552/final_sealed_audit/`
- G14 offensive gate: `PASS (20/20)` with `isolation: tart-disposable-guest`

## Phase 10 — Closeout
**DONE (Thursday, July 23, 2026).**
- Final reports: [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md), [`A_PLUS_CLOSEOUT.md`](A_PLUS_CLOSEOUT.md)
- Truth surfaces refreshed to current front-door status.
- Honest boundary preserved: only claim A+ when gates **and** a current A15 hostile audit say so.

**No claims without evidence. No merge without A15 reproduction.**
