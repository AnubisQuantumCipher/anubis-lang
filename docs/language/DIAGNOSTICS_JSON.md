# `anubis-diagnostics/1` — the machine-readable refusal format

Status: normative for the fields it defines. Emitted by `anubis check --message-format=json`.
Date: 2026-09-21.

A refusal only a human can read is a refusal an agent must guess at, and an agent
that guesses reaches for the cheapest edit rather than the correct one. This
format is the other half of the language's fail-closed discipline: the compiler
already refuses honestly, and this is how it says so to a machine without saying
anything it cannot stand behind.

---

## 1. Shape

JSON Lines on stdout. One `anubis.diagnostic` object per refusal, then exactly
one `anubis.summary`. Nothing else shares the channel, so a consumer can pipe
stdout into a reader without stripping banners. Warnings and the human error
text go to stderr.

```
$ anubis check prog.anb --message-format=json
{"$type":"anubis.diagnostic","schema":"anubis-diagnostics/1","code":"ANUBIS_ASSERTION_DISPROVED",…}
{"$type":"anubis.summary","schema":"anubis-diagnostics/1","verdict":"fail","counts":{…}}
```

A line arrives as it is decided, so a consumer can act on the first refusal
without waiting for the run to finish. The exit code is unchanged: `0` on pass,
non-zero on any refusal.

`--message-format` accepts `human` (the default) and `json`. **Any other value
is refused** with `ANUBIS_MESSAGE_FORMAT_UNKNOWN` and no stream is written. It
does not fall back to `human`: a consumer that typed `jsonl` would otherwise
receive an empty stdout, which is indistinguishable from a clean run to anything
counting findings.

---

## 2. The one invariant

> **`verdict` is `fail` whenever the check failed.**

Every other field may be incomplete. This one cannot be wrong, because a
consumer that trusts a false `pass` ships the program.

In particular a refusal raised before the solver ran — a parse error, a type
error, an undeclared effect — still produces a diagnostic and still reports
`fail`. A stream that reported the empty solver finding set as `pass` would
announce acceptance, in the machine-readable channel, for a program the compiler
refused.

`verdict: pass` appears only with `counts.total == 0`. There is no third verdict
and no diagnostic a consumer may proceed past: every diagnostic carries
`severity: "error"` and `build_blocking: true`.

---

## 3. Why not SARIF

SARIF is the obvious thing to emit, and it is the wrong normative format here.

Its `level` enum is `none | note | warning | error`. Anubis distinguishes two
refusals that both land on `error`:

| `status` | what the compiler established | correct repair |
|---|---|---|
| `disproved` | a concrete counterexample exists; the obligation is **false** | fix the body, or fix the contract if the contract is what is wrong |
| `undecided` | neither proved nor disproved within the declared budget; the obligation **may still be true** | restate the obligation in a decidable fragment, or raise the declared bound |

Collapse those and an agent "fixes" an undecided obligation by weakening the
contract until it passes — which is precisely the laundering this language
exists to refuse, performed automatically and at scale. So the epistemic class
is a **field**, never a severity, and never inferred from an exit code.

SARIF also has no field for a counterexample, which is the most useful thing
Anubis knows and the thing no comparable toolchain can produce.

A SARIF projection may be added later. If it is, it will be declared **lossy**
and non-normative, and this document stays the definition.

---

## 4. Fields

### `anubis.diagnostic`

| field | type | notes |
|---|---|---|
| `$type` | `"anubis.diagnostic"` | discriminator; do not infer a line's kind from its position |
| `schema` | `"anubis-diagnostics/1"` | |
| `code` | string | stable `ANUBIS_*` code, **one per finding** |
| `family` | `contract` \| `wrap_safety` \| `solver_trust` \| `frontend` | what kind of obligation |
| `status` | `disproved` \| `undecided` \| `replay_mismatch` \| `refused` | what the compiler established — see §3 |
| `defect_locus` | `program` \| `compiler` \| `capability` | where the defect is |
| `agent_action` | `repair_program` \| `restate_or_raise_budget` \| `investigate_compiler` | the single next step |
| `severity` | `"error"` | always |
| `build_blocking` | `true` | always |
| `message` | string | human-readable; never the sole carrier of a fact no field holds |
| `obligation` | object, optional | absent when the refusal predates any obligation |
| `counterexample` | object, optional | §6 |
| `location` | object, optional | §7 |
| `budget` | object, optional | §8 |
| `suggestions` | array | **always empty in v1** — §9 |

`code` is per finding, never a batch headline. A run with two disproofs and one
undecided emits three codes, not one, because a headline standing in for
heterogeneous findings is strictly less informative than the findings it
introduces.

`defect_locus: compiler` means **do not edit the program**. An agent that
repairs source in response to a solver disagreement makes the tree worse and
destroys the evidence of the disagreement.

### `anubis.summary`

| field | type | notes |
|---|---|---|
| `$type` | `"anubis.summary"` | |
| `schema` | `"anubis-diagnostics/1"` | |
| `verdict` | `pass` \| `fail` | §2 |
| `counts` | `{total, disproved, undecided, replay_mismatch, refused}` | derived from the diagnostics actually emitted, so they cannot disagree |
| `coverage` | object, optional | §5 — how much of the verdict carries a witness |

---

## 5. `coverage`

`{certified, trusted_to_solver, discharged, uncertified}`, present on a `pass`
as well as a `fail`.

Without it a `pass` says the same thing whether every obligation carried a
machine-checkable refutation or none did. With it, the verdict states its own
worth:

```
check passed
certificates: 9/10 obligations carry a re-checkable witness; 1 trusted to the solver with none
  no witness (REG-002, out of the proven fragment): ensures:(bvsge (bvsdiv anb_a anb_b) (_ bv0 64))
```

- `certified` — the native solver decided it, which requires all of the proven
  fragment gate, a CDCL root refutation, and an independent checker accepting
  that refutation. A witness for it exists in the evidence bundle.
- `trusted_to_solver` — proved on the solver's word alone, with no witness
  anyone can re-check. This is the REG-002 residual: `bvsdiv`, `bvurem`,
  `bvsrem`, `bvudiv`, `bvashr` and `sign_extend` have no machine-checked
  bit-blast, so the native lane declines.
- `uncertified` — those obligations **named**, not merely counted, so the
  admission is specific.

`discharged` is `certified + trusted_to_solver`. It is not the number of checks:
a failure discharges nothing.

**Absent, never zero, when nothing was discharged.** A program with nothing to
prove has not failed to witness anything, and `0/0` would read as "nothing was
witnessed" rather than "nothing was attempted".

Coverage may understate itself and may never overstate it: an obligation whose
provenance the classifier does not recognise is counted as neither, rather than
as certified.

What coverage does **not** claim: a certified obligation means a stranger can
re-check that *this CNF is unsatisfiable*. Nothing in the bundle yet binds that
CNF to its SMT query, or the query to the source. That chain is still the
compiler's word, and `docs/PROOF_CORRESPONDENCE.md` says so. Coverage is a
statement about witnesses, not about end-to-end verification.

## 6. `counterexample`

The concrete assignment that falsifies an obligation. This is the asset no
comparable toolchain ships — rustc has no counterexample to give — and it is
also the field most capable of misleading a consumer. So it carries its own
trust.

| field | type | notes |
|---|---|---|
| `trustworthy` | bool | `true` only when the compiler **replayed** this model against the obligation |
| `model_completeness` | `complete` \| `partial` | whether every declared variable is assigned |
| `missing_vars` | array of string | variables the obligation declares that the model left free |
| `assignments` | array of `{var, smt_value, decimal?}` | `var` is the source name, with the compiler's `anb_` prefix removed |
| `raw_model` | string | the solver's output verbatim, so a reader can check this parse |

Three rules a consumer must honour:

- **Do not repair against an untrustworthy model.** A model the compiler did not
  replay is shipped for audit, not for action.
- **Do not read a missing variable as zero.** The solver left it free because
  any value works. That is a different and stronger statement than "it is 0",
  and treating it as 0 produces a repair that fixes nothing.
- **`decimal` is absent, not guessed,** for any term that is not a bitvector
  literal. Bitvector values are read as **signed** 64-bit, matching the source
  types, so `#xffffffffffffffff` is `-1`.

## 7. `location`

`{file, line, column, span_start, span_end}`, with `line` and `column` 1-based.

**Optional, and its absence is informative.** Only the compiler's own structured
span is ever reported. The parse lane fills it. The semantic lanes currently do
not, because their span is start-of-file rather than the violating construct —
a known, named residual recorded in `docs/CLAIMS.md`.

Emitting `1:1` there would be the mislead-with-authority failure that made an
earlier CLI fix worth reverting, except aimed at an agent that will act on it
without looking at the file. An absent field says "not tracked". A wrong one
says "here". That asymmetry is why the field is optional rather than defaulted.

## 8. `budget`

`{metric, limit, measured, consumed?}`.

`metric` is `z3-rlimit`: a **deterministic resource counter**, not a clock. The
verdict is a function of the query, not of machine load. Nothing in this format
calls it a timeout, because that vocabulary tells a reader to retry on a quieter
machine when the answer will not change.

`measured` is `false` and `consumed` is absent until the compiler asks z3 what
it actually spent. An invented figure would let a consumer conclude "raise the
budget" about an obligation no budget can decide.

**Absent entirely when no solver ran.** Reporting a bound against a parse error
would imply work was spent deciding something and invite a reader to raise a
bound that had nothing to do with the refusal.

## 9. `suggestions` is empty, deliberately

The compiler can already print a "possible fix" that re-fails when applied
verbatim, because it never re-proves its own suggestion. Until a re-proof gate
exists, this array is empty.

This absence is the point, not an oversight. An unvalidated suggestion in a
machine-readable field is worse than no field: it converts a heuristic a human
would eye into an edit an agent will apply. When suggestions do ship they will
carry the result of re-proving them, and no suggestion that edits a `requires`,
`ensures` or `invariant` will ever be marked machine-applicable.

Nothing in this format names a way to silence a refusal: no bypass flag, no
suppression key, no confidence score. That is locked by tests at both the
renderer and the CLI.

---

## 10. Stability

The schema string carries the major version. Within `anubis-diagnostics/1`:

- **fields may be added**, so a consumer must ignore unknown fields;
- **enum members may be added**, so a consumer must not assume a closed set —
  treat an unrecognised `status` or `agent_action` as blocking, never as passing;
- **no field is removed or repurposed**, and no member is given a new meaning.

Anything that would break those emits `anubis-diagnostics/2`. An optional field
that is currently always absent — `location` on a semantic refusal,
`budget.consumed` — appearing later is an addition, not a break.

Codes in `code` are stable identifiers. A code is retired rather than reused.

---

## 11. What this does not do yet

Named so nobody reads more into it than it says.

- **Semantic refusals have no location.** §7.
- **`budget.consumed` is never populated.** §8.
- **No suggestions.** §9.
- **Only the `check` command emits it.** `build`, `prove` and `evidence-verify`
  still speak prose.
- **Coverage counts witnesses, not correspondence.** §5.
- **The counterexample is the solver's, not the source's.** `assignments` names
  source variables, but nothing yet reconstructs the failing call as source-level
  values a reader could paste into a test. That is the same correspondence gap
  `docs/PROOF_CORRESPONDENCE.md` records for the certificate chain.
