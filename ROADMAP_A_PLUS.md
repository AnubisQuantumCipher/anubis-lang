# Anubis A+ Roadmap

**Current (post baseline 2026-07-05):** C-grade real prototype per ANUBIS_REALITY_AUDIT.md.

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
- LSP or honest diagnostic server.
- Expanded CI + fuzz/golden/property.
- All old failure modes impossible.

## Phase 7 — Docs Honesty (A14)
- All required docs with REAL/PARTIAL/... labels.
- CLAIMS.md single source of truth.

## Phase 8 — Hostile Re-audits (A15)
- After 3.x, after 4, after 5, before final.
- A_PLUS_REALITY_AUDIT.md + CLAIM_MATRIX + STEP_STATUS.

## Phase 9 — Sealed Final Audit
`scripts/audit_a_plus.sh` + repro + examples.
Produce final A_PLUS_* reports + exact verdict block.

## Phase 10 — Closeout
Exact report per plan. Only A+ if gates + A15 say so.

**No claims without evidence. No merge without A15 reproduction.**
