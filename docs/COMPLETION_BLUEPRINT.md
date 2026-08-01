# Anubis — Completion Blueprint

**Standing execution plan. Run phases in order. Do not skip. Do not regress.**

This file is the *execution* plan. It is not an authority on status. The authorities are:

- `docs/CLAIMS.md` § Known open issues — the single living list of open soundness defects
- `docs/language/ROADMAP.md` — the phase arc and living status layer

If this file and those disagree, **they win** and this file gets corrected. Never let this become a
second source of truth; that is the exact disease the project is fighting.

---

## Product promise — the only shipping promise

> **`anubis check` passing means Anubis found no way for the program to violate its stated
> contracts, effects, capabilities, or information-flow policy at runtime — and everything it could
> not decide, it refused rather than assumed.**

This is **not a totality claim**. Green means no known defects on the measured surface, not no
defects. [`docs/CLAIMS.md`](CLAIMS.md) is the single living list of known open residuals; a claim
stronger than that list is a defect in the claim.

## Research aspiration — not scheduled and not claimed

A future research track may mechanize correspondence from source semantics through the implemented
analyzer, solver obligations, runtime semantics, and evidence verifier. That requires a linking
proof over the production implementation, not a larger audit corpus or a greener board. It has no
ship date and is never presented as a current guarantee.

Completion for the product therefore means: the product promise remains honest, every observed
residual is named, every security lane is structurally total over the surface it claims, unknown
security labels fail closed, and every completion claim is sealed behind a reproducible gate.

## Status is derived, never maintained here

This document carries **no live board counts**. Re-derive current state from:

- `docs/CLAIMS.md` for open and permanent residuals;
- `docs/language/ROADMAP.md` for graded feature status;
- `bash scripts/phase_metrics.sh` for convergence metrics;
- `bash scripts/run_seal_checklist.sh` for a source-bound host seal after a lead publishes a pin.

Historical observations remain in dated evidence artifacts. They are not copied into current prose.

---

## Phase order

1. **Phase 0 — define done, correct the record, install convergence instruments.**
2. **Phase 1 — evidence and isolation integrity.**
3. **Phase 1.5 — GitHub as the system of record.**
4. **Phase 2 — replace duplicated value-flow walkers with one total, lane-parameterized mechanism.**
5. **Phase 3 — separate the security-label lattice from accept-biased type inference.**
6. **Phase 4 — close or explicitly publish the residual soundness surface.**
7. **Phase 5 — complete the language surface; optional for the product promise.**
8. **Phase 6 — make regression controls and CI permanent.**
9. **Phase 7 — produce the product-release evidence pack.**
10. **Phase 8 — open-ended mechanized-correspondence research.**

The detailed exit criteria are phase-owned. A later phase cannot redefine an earlier phase's exit.

## Mandatory phase stop

At each phase boundary:

1. stop before beginning the next phase;
2. write `PHASE_<n>_COMPLETION_<YYYY-MM-DD>.md` with all required evidence sections;
3. name the absolute tree, commit, branch, dirty state, binary provenance, and toolchain;
4. map every exit criterion to the command and verbatim verdict line that decided it;
5. show RED before GREEN for each fix and an accept-side guard for each enforcing change;
6. try direct, alternate-carrier, and dead-branch falsification twins;
7. paste start and end output from `scripts/phase_metrics.sh` verbatim;
8. separate verified, believed, skipped, and unknown work;
9. list what was not verified and what the phase got wrong;
10. obtain operator approval before proceeding.

A phase with an unmet criterion reports **INCOMPLETE** and stops. It does not move the criterion,
weaken the gate, or call the omission out of scope.

## Landing discipline

- One bounded slice per review unit; never carry an unbisectable stack of compiler changes.
- Code and documentation land in separate commits. Fixtures stay with the code slice they verify.
- Trust-surface changes state the old and proposed accept conditions explicitly.
- Only the active lead may build, publish pins, commit, or push; explicit paths only in a mixed tree.
- A frozen pin is evidence about that artifact until source binding is verified; it is not evidence
  about a later working tree.
- Research, crash, fuzz, exploit, and offensive execution uses the required disposable guest. A host
  run is never substituted for guest evidence.
