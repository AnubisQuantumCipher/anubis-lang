# Safe Mode Trust Spine — A15-Equivalent Hostile Audit

**Verdict:** PASS within the tested claim boundary

**Sealed run:** `20260725T230143Z/safe_mode_trust_spine`

**Completed:** 2026-07-26T01:20:07Z

**Baseline commit:** `4a361b6e2d55f0769cece575fb99d389385dfca6`

**Branch:** `a-plus-maturity/safe-mode-trust-spine-20260725`

## Independence boundary

This is an A15-equivalent hostile reproduction performed by the primary implementer. It is not
represented as an independent human or separately controlled reviewer. The publication remains a
draft pull request until the repository's A15 independence requirement is satisfied.

## Claim under test

The tested change makes contracts and Safe Mode enforcement command-boundary properties:

- `check`, `run`, `test`, and default `build` reject false or unproved obligations.
- `build --no-verify` is an explicit bypass whose evidence says `UNVERIFIED`; it makes no proof
  claim.
- A failed command cannot exit zero while emitting `verdict: FAIL`.
- Rejected evidence is internally hash-valid, artifact-free, tier `rejected`, and carries the
  actual diagnostic.
- Program mode is the recursive maximum `Safe < Research < Exploit`; source order and nesting
  cannot hide privileged code.
- An explicit `@safe` function remains a Safe enclave in a mixed-mode program.
- Runtime integer lowering and solver semantics agree at signed/unsigned boundaries covered by the
  regression matrix.
- Checker-approved taint qualifiers, secret values, and explicit declassification lower through
  ordinary native execution.
- Comments and verification annotations survive formatting.
- Research, PoC, fuzz, exploit, and crash-capable execution retain their capability inside the
  mandatory disposable Tart/VZ lane; the host has no execution fallback.

## Acceptance matrix

| Test | Expected | Reproduced result |
|---|---|---|
| False contract: `check` | nonzero + counterexample | PASS |
| False contract: `run` | nonzero before execution | PASS |
| False contract: `test` | nonzero | PASS |
| False contract: `build` | nonzero, no artifact | PASS |
| Explicit unsafe bypass | valid `UNVERIFIED` envelope, no proof claim | PASS |
| Valid contract: all four commands | zero | PASS |
| Evidence verdict `FAIL` | command cannot exit zero | PASS |
| Later/nested Research or Exploit item | aggregate mode elevated | PASS |
| Explicit Safe enclave in mixed program | Safe checks remain enforced | PASS |
| Imported program evidence | analyze resolved program, cold-verify | PASS |
| Integer solver/runtime boundaries | agree on exercised i64/u32 limits | PASS |
| Approved taint/secret lowering | native run succeeds | PASS |
| Undeclassified taint/secret flow | rejected before execution | PASS |

Primary regression commands:

```text
cargo test -p anubis --test execution_contract_gate
cargo test -p anubis --test safe_mode_program_gate
cargo test -p anubis-compiler solver_models_i64_signed_not_32bit_unsigned
cargo test -p anubis-compiler phase3_unsigned_fixed_width_boundary_coercion
cargo test -p anubis-compiler unsigned_and_signed_integer_boundaries_match_solver_contract
```

## Formal and differential evidence

- `bash scripts/run_formal_gate.sh`: PASS, Lean 4.32.0, 18 jobs, with no
  `sorry`/`admit`/axiom/`native_decide`.
- `formal/Anubis/ModeAggregation.lean` proves the abstract privilege join, member upper bound,
  Safe-if-and-only-if-all-members-Safe property, and that Research/Exploit cannot hide behind a
  Safe prefix.
- Scope warning: Lean proves the abstract lattice, not the Rust AST traversal. Rust unit and CLI
  integration tests cover the traversal correspondence across source order, modules, impls, and
  Exploit dominance.
- `bash scripts/run_native_authoritative_gate.sh`: PASS; 569 differential files, zero mismatches
  and zero solver/runtime disagreements.
- `bash scripts/run_security_fixtures.sh --out
  out/safe_mode_trust_spine_20260725/security_resolved`: PASS 149/149.

## Sealed A+ gate reproduction

Command:

```text
bash scripts/audit_a_plus.sh --out implementer/a_plus_audit_run/20260725T230143Z/safe_mode_trust_spine
```

Result: **PASS (15/15 passed, 0 failed, 0 skipped)**.

| Gate | Result |
|---|---|
| G1 formatting | PASS, no diffs |
| G2 Clippy | PASS, zero warnings/errors |
| G3 Rust tests | PASS, 827 tests |
| G4 release build | PASS |
| G5 language fixtures | PASS, 244/244 |
| G6 Turing core | PASS, 13/13 |
| G7 PCA | PASS, 13/13 |
| G8 security fixtures | PASS, 149/149 in report |
| G9 PoC kit | PASS, 4/4 |
| G10 prove | PASS, 11/11 |
| G11 enum/match | PASS |
| G12 for-in | PASS |
| G13 language trio | PASS |
| G14 offensive | PASS, 34/34 |
| G15 dogfood/feel | PASS, 8/8 programs |

Canonical machine-readable result: `gate_report.json`.

## Isolation reproduction

G9 and G14 ran through the mandatory Anubis Tart/VZ path:

- Base: `anubis-xcode`
- G9 guest: `anubis-poc-kit-gate-54712`
- G14 guest: `anubis-offensive-gate-57758`
- Isolation marker: `tart-disposable-guest`
- G9 execution boundary: `mandatory disposable anubis-xcode guest; no host fallback`

G9 reproduced the packing smoke, intentional gold-target crash, process mutation fuzz
(`unique_crashes=57`, `total_crashes=58`), and network-target rejection. G14 reproduced 34/34
offensive-platform checks. These results demonstrate preserved lab capability behind a stronger
execution boundary; they do not authorize use outside the approved scope.

## Dogfood program and counterexamples

`examples/programs/maat_recovery_authority/main.anb` passed `check`, `run`, and `build`, and its
check/build evidence cold-verified. Its deterministic output selected Osiris with score 126,
eligible quorum 3, score sum 374, and a proved countdown result of zero.

Seven companion controls reproduced:

- false postcondition;
- violated direct precondition;
- violated nested precondition;
- failed loop invariant;
- tainted public sink;
- secret public egress;
- explicit declassification acceptance.

The first six rejected with nonzero exits and precise diagnostics; the declassification control
passed.

## Evidence-honesty boundary

The generated bundles in this run are integrity-valid but unsigned unless a signing key is
explicitly configured. `anubis verify` proves manifest/hash consistency and rechecks claims; it
does not turn an unsigned bundle into third-party identity attestation.

The result supports the bounded statement that the tested Safe Mode and command/evidence trust
spine is fail-closed at this source state. It does not establish total language soundness,
implementation equivalence to the Lean model, or independent A15 approval.
