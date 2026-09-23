# Item 21 Family 1 — contract `requires` carried through a function-valued parameter (2026-09-23)

Branch `item21/soundness-slices`. Baseline pin `anubis-029d5538` (sha256 `c5887c72…`, = committed
HEAD before this slice). Code commit: `84296cef`. This doc commit follows it (code and docs never share a
commit).

## The defect

A `requires` on a function passed as a value was discharged only at applications that run
unconditionally and only when the callee's NAME was a formal. Any guard, loop, local alias, `match`/
`if let`, or a call through a builtin hid the application, so the precondition went unchecked. Item 21
rows 1/2. Reproduced (11-leak corpus, harness `scratchpad/repro`): `app(g){ if 1==1 { g(-1); } }` with
`app(f)` and a contracted `f` accepted and ran `f(-1)`.

## The fix

A new total collector `compiler/src/middle/contract_carrier.rs` records every site a function-valued
parameter's contract can be reached (Apply / CallNamed / Escape / Return), sound by construction: a
fact is used only over a value that cannot change during one call (a never-assigned formal, a
once-bound local substituted by its initializer, a literal, a `for`-var over stable bounds); every
other callee-internal name becomes an unmodeled placeholder. `discharge_carried_call_requires`
(rewritten) resolves each site at the call, substitutes the actual arguments (with the callee's
declared unsigned/float coercions), pushes each stable guard, and discharges the resolved function's
precondition under the caller's facts. It fires only on KNOWN contracted-function identities; an
unknown value is not assumed to be a function (the item-11 principle already in the singleton policy),
which is what keeps it from flooding ordinary code. A `function_like_formals` gate keeps pure-data
parameters out of the analysis. Anything unresolvable becomes an explicit `requires-unresolved@…`
obligation (new `UNRESOLVED_REQUIRES_PREFIX` / `UNRESOLVED_PRECONDITION_DETAIL`), short-circuited in
`check_obligations` to `ANUBIS_ASSERTION_UNDECIDED` — no solver runs, no model, honest through every
consumer (JSON diagnostics budget/locus, `format_check_failures`, evidence `proofs.json`
`unresolved_not_encoded`, `solver_replay.json` `not_encoded`).

## Verification (pin `anubis-f1-final2`)

- Acceptance matrix `scratchpad/repro`/`~/.cache/anubis-item21/matrix2.sh` — SLICE **61 PASS, 0
  silent-accepts**, 3 explicit precision-loss refusals (below).
- Corpus verdict-diff vs baseline: **0 real-program flips** over the pre-existing corpus (the only
  flips are this slice's own `carrier_*_rejects` fixtures correctly closing).
- `cargo test` compiler-lib (deterministic, `--test-threads=2`): **848 / 0**. (At full parallelism a
  single native-build test flakes under load and passes in isolation — the documented ETXTBSY/load
  class, not this code.)
- Fixtures: security **356/356**, language **259/259**, stdlib fail-closed **104/104**. clippy/fmt clean.
- 13 new `examples/security/carrier_*.anb` (RED→GREEN closures + accept guards).
- Two independent code reviews + one focused re-review of the post-review deltas. The soundness review
  found one confirmed silent accept (a formal applied only through a function-applying builtin outside
  the HOF list) — fixed by making the marking total. The precision review confirmed 0 corpus
  regressions, determinism, honest evidence rows, old collector removed; two diagnostic-hygiene leaks,
  Leak A fixed.

## Bounded honestly — NOT closed by this slice

- **Precision losses** (explicit `UNDECIDED`/`MIXED`, never silent): loop-carried mutation of the
  applied argument; requires-seeded recursion; a call-result used as a stable local then guarded;
  builtin higher-order escapes (`map`/`each`/…) and closure factories.
- **Residuals** (silent-accept unchanged from baseline, own future slices): a function value flowing
  through a LOCAL holding a container (`let xs=[f]; app(xs)`) or pushed into a list; a function
  RETURNED out of a callee and resolved by the caller's join (the item-10 join lane); the direct
  lane's own silent drop of unmodelable clauses and its zero-iteration/post-return over-rejections.
- Item 21 overall: **9 of 11 reproduced leaks now closed** across slices 1–3; D9 (unannotated formal)
  and the taint sink-argument carrier remain.

## Known diagnostic limitation (not a soundness or verdict defect)

The re-review noted that `carrier_display` rewrites every internal placeholder to the literal
`<unmodeled value>`, and `carrier_unresolved` dedups obligations by name. Two distinct unmodeled
preconditions in one function can therefore collapse to one obligation NAME and render as a single
diagnostic line. This cannot hide a refusal — every `requires-unresolved@…` obligation is forced to
`FAIL`, so at least one survives and the program still refuses `UNDECIDED`. It only means a program
with several distinct unmodeled preconditions may show fewer diagnostic lines than it has. A future
refinement can append a per-site index to the obligation key; it has no soundness bearing.
