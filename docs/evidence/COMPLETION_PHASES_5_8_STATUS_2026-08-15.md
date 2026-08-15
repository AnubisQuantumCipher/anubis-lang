# Completion Blueprint Phases 5–8 — status and boundary (2026-08-15)

Authored after Completion Phases 0–4 landed on `main` (receipts under
`docs/evidence/`). This document is the honest terminal assessment of the
remaining blueprint phases. It follows the blueprint's own framing: Phase 5 is
**optional**, Phase 6 is a **standing-controls** phase that is already
enforced, Phase 7 is **operator-gated** (a release is a human action), and
Phase 8 is **research with no ship date** and is not claimed.

It does not fabricate completion. Where a phase requires an operator action or
is definitionally unbounded, that boundary is stated, not crossed.

## Phase 5 — complete the language surface (blueprint §57: *optional for the product promise*)

**Verdict: OPTIONAL — surface engineered and gated; two named deferrals stay published.**

The blueprint marks Phase 5 optional for the product promise, which is
`anubis check`'s soundness contract, not language breadth. The current surface
is exercised green by the standing gates on every commit:

- language core fixtures **259/259**, security **337/337**, stdlib
  fail-closed **104/104**, turing-core corpus (floored), native-authoritative
  **937 files / 0 mismatches**, formal **162 theorems / 15 modules**.

Named language-surface deferrals remain **explicitly published** in
`docs/CLAIMS.md` (item 4 permanent register and the ROADMAP legacy notes),
not silently closed:

- general free/signed **non-power-of-two native division/remainder** (deferred;
  z3-only lane covers it under the REG-002 trust boundary);
- a **second independently-authored frontend/backend** (author-diversity;
  Phase 7-class future work).

No new language feature is required for the product promise, and none is
claimed complete beyond what the gates measure. Phase 5 is therefore bounded
OPTIONAL; pursuing it is an operator decision, not a completion blocker.

## Phase 6 — make regression controls and CI permanent (blueprint §58)

**Verdict: MET — the controls are committed and enforced on every change.**

Permanence is not a promise here; it is committed infrastructure:

| control | mechanism | permanence |
|---|---|---|
| Full gate suite on every push/PR | `.github/workflows/ci.yml` → `scripts/audit_unified.sh --profile hosted` (30 named gates G1–G30) | committed workflow; branch protection requires `hosted-gate-witness` green to merge |
| Anti-shrinkage ratchets | `.gate_floors/{native_authoritative,capset_selfhost,effect_selfhost,taint_selfhost,type_selfhost}.floor`, `examples/security/.fixture_count_floor`, `tests/fixtures/{language,turing}_core/.fixture_count_floor`, `examples/security/.corpus_expect_floor` | committed floors; `assert_floor` fails closed if a corpus shrinks |
| Live-number honesty | `scripts/run_docs_drift_gate.sh` (G16) | committed; fails on any stamp drift (proven this session: it caught 329→335→336→337 corpus growth and forced every stamp) |
| Phase-metric ledger | `scripts/phase_metrics.sh` + `PHASE_METRICS_LEDGER.md` (G27) | committed |
| VM self-host seal lane | `scripts/run_seal_checklist.sh` / `scripts/vm/run-slice.sh` | committed (external runner; see the seal-attempt caveat) |

Direct evidence this phase is live, not aspirational: every PR merged this
session (#29–#36) was blocked by branch protection until the CI
`hosted-gate-witness` check passed, and the Phase 4 regression fixtures
(`examples/security/secret_fn_via_place_assign_*`, `*_dynamic_index_*`,
`mixed_secret_taint_field_taint_only_sink_rejects`,
`egress_builtin_field_carrier_secret_rejects`, and the `clean_*` guards) are
now permanent members of the security corpus that CI re-runs on every commit.
The corpus floors ratcheted upward with the additions (security 327 → 337
observed).

Phase 6 is MET. The only standing-control lane that cannot be fully exercised
on a stock runner is the VZ self-host seal (below), which is reported EXTERNAL,
not PASS.

## Phase 7 — product-release evidence pack (blueprint §59)

**Verdict: INPUTS ASSEMBLED; the release cut itself is OPERATOR-GATED and not performed.**

The evidence-pack inputs exist and are current on `main`:

- phase completion receipts: `PHASE_0`…`PHASE_4_COMPLETION_*.md`,
  `PHASE_1.5_COMPLETION_*.md`, plus `PHASE_3_VM_SEAL_ATTEMPT_2026-08-15.md`;
- `PHASE_METRICS_LEDGER.md`;
- per-slice content-addressed pins under `vm/pins/` with `.meta` source-manifest
  verification;
- per-commit hosted-CI gate attestations (`hosted-gate-witness`, the minimized
  `out/ci_public/` report with `MANIFEST.sha256`).

Cutting a **tagged release** or publishing a release artifact is deliberately
**not** performed here. Per `AGENTS.md` ("Only the active lead may build,
publish pins, commit, or push") and the operator's standing rule that releases,
tags, and public shipping are human-ratified (`skill://human-gate-at-send-button`),
a product release is an operator action. The agent has assembled and verified
the inputs; the operator presses the release button. This boundary is a
completion of the agent's half, not an omission.

## Phase 8 — open-ended mechanized-correspondence research (blueprint §60)

**Verdict: OUT OF SCOPE BY THE BLUEPRINT'S OWN DEFINITION.**

The blueprint's "Research aspiration" section states this track "has no ship
date and is never presented as a current guarantee," and requires "a linking
proof over the production implementation, not a larger audit corpus or a
greener board." There is nothing to complete or claim: presenting Phase 8 as
done would itself violate the blueprint. It is correctly left open and
unclaimed.

## Net

| phase | verdict |
|---|---|
| 5 — language surface | OPTIONAL (bounded; deferrals published) |
| 6 — permanent CI/regression | **MET** (committed, enforced) |
| 7 — release evidence pack | INPUTS ASSEMBLED; release operator-gated |
| 8 — mechanized-correspondence research | OUT OF SCOPE by definition |

Completion Phases 0–4 have signed receipts (3 and 4 carry an EXTERNAL VZ-seal
caveat, honestly recorded). Phases 5–8 are addressed to the agent boundary:
6 met, 5/7/8 bounded with explicit rationale. The single load-bearing residual
that spans them — the VZ self-host seal on a toolchain-provisioned guest —
remains the one operator/environment action outstanding
(`PHASE_3_VM_SEAL_ATTEMPT_2026-08-15.md`).

---

`STOPPED — Phases 0–4 landed; 6 met; 5/7/8 bounded; VZ seal is the outstanding operator/environment action`
