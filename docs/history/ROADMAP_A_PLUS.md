# Anubis A+ Roadmap

**Baseline (2026-07-05):** C-grade real prototype per ANUBIS_REALITY_AUDIT.md — the *starting* point, not the current state.

**Live status is well past this baseline.** The living language tracker is [`docs/language/ROADMAP.md`](../../docs/language/ROADMAP.md), where phases 0–10 are already documented at the language/freeze level. For this repo-governing A+ roadmap, **Phase 9 is DONE** (`out/a_plus_phase9_final_rerun_20260723`, 15/15; G14 was 20/20). **Phase 10 is DONE** via [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md) and [`A_PLUS_CLOSEOUT.md`](A_PLUS_CLOSEOUT.md). **A15 was DONE on 2026-07-24:** [`implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md`](../../implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md) with re-derived front door `out/a_plus_a15_frontdoor_20260724-154145` → **15/15 PASS**, G14 **34/34**. **A+ was CLAIMED** on that seal (hostile findings F1–F4 remediated and re-verified).

**That seal is now stale relative to 2026-07-26 fleet findings.** The 15 gates do not cover the
open soundness class. **Living status only:**
[`docs/CLAIMS.md` § Known open issues (2026-07-26)](../../docs/CLAIMS.md#known-open-issues-2026-07-26).

Honest snapshot (GROK-MAAT round 8 — details only in CLAIMS):

- **Numbers:** security **228/228**, language **244/244**, stdlib **45/45**, capset **5/5**,
  formal PASS, native **681/0**. Published red list **empty**.
- **Theme (8+ classes):** user writes something down / producer computes a label → consumer
  ignores or recomputes independently. Proven across returns, fields, (R)/PCA, M1–M3, D1–D4,
  research auth, unknown attrs.
- **Green = no KNOWN defects, not no defects.** D1–D4 closed this stamp; composition residuals
  may remain.

Read every "A+ is CLAIMED" / Phase 9–10 DONE line in this file as **as of the 2026-07-24 seal**,
not current. Whether the original 15 gates still pass on a fresh re-run has **not** been
re-verified in this pass.

**Target (definition, not current achievement):** all 15 gates in A_PLUS_ACCEPTANCE_CRITERIA.md
pass + A15 hostile audit clean **and** the false-accept / walker-parity class closed with
independent re-hunt — see CLAIMS.

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
- All required docs with under Command/PARTIAL/not claimed/... labels.
- CLAIMS.md single source of truth.

## Phase 8 — Hostile Re-audits (A15)
- After 3.x, after 4, after 5, before final.
- A_PLUS_REALITY_AUDIT.md + CLAIM_MATRIX + STEP_STATUS.

## Phase 9 — Sealed Final Audit
**DONE (Thursday, July 23, 2026; re-confirmed under A15 2026-07-24).**
- Historical: `bash scripts/audit_a_plus.sh --out out/a_plus_phase9_final_rerun_20260723` → `PASS (15/15)`.
- Current re-seal: `out/a_plus_a15_frontdoor_20260724-154145` → `PASS (15/15)`; G14 **34/34**.
- Exact verdict block: [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md)
- A15 package: `implementer/a_plus_audit_run/20260724-154145/full_language_audit/`
- Prior mirror: `implementer/a_plus_audit_run/20260723-152552/final_sealed_audit/`

## Phase 10 — Closeout
**DONE (Thursday, July 23, 2026; A+ label sealed Friday, July 24, 2026).**
- Final reports: [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md), [`A_PLUS_CLOSEOUT.md`](A_PLUS_CLOSEOUT.md)
- Truth surfaces refreshed to current front-door status.
- A15 hostile audit present and clean after remediation of F1–F4 (isolation fail-open, stale guest binary, clippy, stack).

**No claims without evidence. A+ requires gates + current A15 — both present on the 2026-07-24 seal
(see the staleness note above for what has changed since).**
