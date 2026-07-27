# Anubis — Completion Blueprint

**Standing execution plan. Run phases in order. Do not skip. Do not regress.**

This file is the *execution* plan. It is not an authority on status. The authorities are:

- `docs/CLAIMS.md` § Known open issues — the single living list of open soundness defects
- `docs/language/ROADMAP.md` — the phase arc and living status layer

If this file and those disagree, **they win** and this file gets corrected. Never let this become a
second source of truth; that is the exact disease the project is fighting.

---

## The one sentence that decides "complete"

> `anubis check` PASS ⇒ the program cannot violate its stated contracts, effects, capabilities, or
> information-flow policy at runtime.

`docs/language/ROADMAP.md` states the consequence plainly:

> Phases 0–10 freestanding "DONE / At DoD / ROADMAP COMPLETE" are **FALSE** as a current *soundness*
> claim while CLAIMS open §1 stands. **Open false accepts break it.**

**Everything else can be green and it does not matter while a false accept stands.** That is the
sequencing rationale for the whole file.

---

## Measured baseline (2026-07-27, re-measure before quoting)

| Quantity | Value | How to re-derive |
|---|---:|---|
| Commits | 778 | `git rev-list --count HEAD` |
| Lean theorems | 162 | strip block comments first; naive grep gives 163 |
| Lean modules | 15 | `ls formal/Anubis/*.lean` |
| Security fixtures on disk | 280 | `ls examples/security/*.anb \| wc -l` |
| Language fixtures | 244 | `ls tests/fixtures/language_core/*.anb \| wc -l` |
| Security gate | 278/278 PASS | 2 red fixtures held out of tree pending their fix |
| Language gate | 244/244 PASS | pin `ANUBIS_BIN=./target/release/anubis` |

Re-derive by command. A number quoted from memory in this project has been wrong more than once —
including a "242/242 PASS" that was actually 242/244 FAIL.

### Corrections to the external assessment (2026-07-27)

An outside review rated the project 9.8/10 and is largely fair on architecture and ambition. Three of
its factual claims are **stale or false** and must not be carried forward:

1. **"Security tests 149/149"** — stale. The corpus is 280 fixtures; the gate reads 278/278 with two
   held out.
2. **"150+ theorems / 14 Lean modules"** — stale. Measured: **162 theorems across 15 modules**.
3. **"A green `anubis check` *never* certifies a contract that `anubis run` violates … This is not
   marketing."** — **This was false when written.** On 2026-07-27, with a fully green board, this
   program passed `check` and printed the secret at runtime:

   ```anubis
   fn key() -> secret<i64> { return 42; }
   fn app(f) { print(f()); }
   fn main() { app(key); }        // check exit 0; run printed 42
   ```

   Closed in `d5f0be8`. Four sibling carriers were still open at time of writing. **A green board is
   when a claim surface is most dangerous** — a reviewer sees 244/244 and concludes the promise is
   discharged, when the corpus merely stopped offering a counterexample.

The assessment's architecture read (self-hosting, native solver, dual-use, fail-closed intent) is
sound. Its *soundness* claim was not. Keep the first, discard the second.

---

## PROGRESS — 2026-07-27 (measured, re-derive before quoting)

| Gate | Start of day | Now |
|---|---:|---:|
| security fixtures | 242/244 **FAIL** | **294/294 PASS** |
| language fixtures | 244/244 | **244/244** |
| stdlib fail-closed | 45/45 | **80/80** |

**Phase 1 — DONE.** Twelve function-identity carriers closed: bare name, alias chain, if-join,
match-join, struct-pattern binder, local container, container-as-argument (list/struct/map/enum),
return, multi-hop return, return-position join, method return, identity forwarder, pass-through
builtin, argument position, and the `push`-built container. Both lanes (secret and tainted). The
adversary's pre-registered residual set went 21/30 → 30/30.

**Phase 2 — partially discharged.** The three totality lessons are recorded in-code, each learned by
being wrong: totality over `Expr` is necessary and NOT sufficient (the struct-pattern arm existed and
did nothing); the walk must be total over the forms that HOLD expressions; and a `..` in a match arm
is the tell — it discarded `invariant: Vec<Expr>` in the elevator detector, the FOURTH instance of
that shape in one walker.

**Phase 3 — DONE.** Fail-open `EXPECT` default, instrument drift, empty-corpus guards, all-SKIP
hollow pass, unguarded `grep` under `pipefail`, and constituent gates reselecting their own binary.
The seal could print `SEAL_PASS` with two gates SKIPPED and a constituent grading
`/tmp/WRONG-BINARY` — demonstrated by counterexample, now impossible. `scripts/lib/gate_common.sh`
plus content-addressed read-only binary pins (`scripts/publish_pin.sh`).

**Phase 5 — bounded residual published.** 31 builtins returned a plausible WRONG value at rc=0 and
now fail closed. **213** builtins derived by command against the README's "~150". Coverage is a
three-way union (A–L 107 sealed, M–Z non-crypto 87 sealed, crypto 19 UNMEASURED) and the residual is
in `docs/CLAIMS.md` verbatim rather than rounded off.

**Open, and named:**

- crypto/hash/KDF/random/x25519 builtin slice — unmeasured
- research-mode receipt chain — `seal_action`/`collect_loot`/`scrape_guest` have zero real call
  sites on the tart path; guest crashes produce no evidence while `campaign-init` advances the chain
- `vz-c2-cycle` — agent beacons and tasks queue, results stay `[]`
- **`push` returns `0`, and `check` does not catch its misuse:**
  `let ys = push(xs, 3); len(ys)` — `check` rc=0, `run` rc=1 (panic). A check/run divergence of the
  CLAIMS item 2 class, and a footgun: functional-style use silently yields a non-container.
- VM seal (CLAIMS item 3)
- the headline promise rewrite + drift gate

---

## PHASE 1 — Close the false-accept class ← THE BLOCKER

The disease, from `docs/CLAIMS.md`:

> A user writes something down, or a producer computes a label, and a consumer ignores it or
> recomputes it independently.

Proven across 8 closed classes there. Classes 9 and 10 closed 2026-07-27 (struct-pattern binder
`321eafc`; fn-value-by-name `d5f0be8`). Class 11 is the current work.

GROK-SEKHMET established by measurement — not argument — that ordinary `secret<T>` **values** already
propagate correctly through if-join, match-join and list-at-param, while **function references** never
consult that machinery. **Wiring gap, not missing mechanism.** That is what makes this bounded rather
than the infinite higher-order whack-a-mole it has looked like before.

### Carriers

| Carrier | Program | Status |
|---|---|---|
| param | `app(key)` | CLOSED `d5f0be8` |
| alias chain | `let g = key; app(g)` | CLOSED `d5f0be8` |
| if-join | `let f = if c { key } else { other }` | CLOSED `08d0071` |
| struct-pattern binder | `let V { k } = v` | CLOSED `321eafc` |
| **match-join** | `match 1 { 1 => key, _ => other }` | **OPEN** |
| **list element** | `app([key])` then `fs[0]()` | **OPEN** |
| **struct field** | `app(H { f: key })` then `h.f()` | **OPEN** |
| **map value** | `app({ "k": key })` then `m["k"]()` | **OPEN** |
| **return** | `fn get(){ return key; }` | **OPEN** |

Each has an integrity dual (`tainted<T>` → `shell`/`net.send`). `shell` is non-runnable
(`ANUBIS_UNSUPPORTED_NATIVE_LOWERING`), so settle those check-side plus a `print`-substituted runtime
witness.

### Method — non-negotiable

- **Propagate IDENTITY, not the label.** Marking a fn reference "secret" taints every binding that
  merely HOLDS a secret-returning function, including ones never applied. Carrying the alias
  materialises the label only where the function is actually applied, which is where existing
  machinery already computes it correctly. The guard that proves this works: a secret fn in a
  conditional that is **never applied** must still compile.
- **Fail closed at joins.** `fn_alias` holds one name; a join offers two. Prefer the
  secret/tainting branch so branch ORDER cannot decide soundness.
- **Discriminator both directions.** DIRECT form REJECTED + LAUNDERED form ACCEPTED, real pasted
  output. Both-accept is a benign symmetric blind spot, not a leak. Distinguish a true accept from a
  fail-open DEFERRAL.
- **Walker parity.** Confidentiality is decided in `resolve_closure_arg`; effects in
  `analyze_expr_effect`. A fix in one changes nothing in the other. Cost me a full cycle on
  2026-07-27 — patch went into the effect walker and did nothing.
- **Verdict diff before commit.** Full corpus, both gates, **zero accept→reject flips**.

### Done when

- All 9 carriers CLOSED, integrity duals included
- The 2 held fixtures land: `declared_{secret,tainted}_fn_value_returned_rejects.anb`
- Red inventory empty: `for f in examples/security/*_rejects.anb; do ./target/release/anubis check "$f" >/dev/null 2>&1 && echo RED $f; done` → zero lines
- 280/280 security + 244/244 language, zero flips
- GROK-SEKHMET's guard corpus (`scratchpad/fleet_20260726/sekhmet_round4.md`, 521 lines) all passing
- A fresh adversarial hunt over the carrier surface returns no new form

---

## PHASE 2 — Make the class unable to return

Phase 1 alone buys "no KNOWN defects", not completeness. Eleven surfaces of one disease, and
form-by-form closure demonstrably does not terminate — three times in this arc a fix closed one form
while its isomorphic twin stayed open (D4 closed enum-payload-at-binder, left plain struct pattern;
class 1 closed declared returns, left the fn-VALUE carrier; the corpus closed the local alias, left
the param boundary).

The repo already has the working pattern: `solver/src/fragment.rs::is_proven_authoritative` — a total
walker with **no wildcard arm**, so a new `Term` variant fails to **compile** rather than riding as
authoritative.

**Two hard-won caveats, both learned by being wrong:**

1. **Totality over the enum is necessary and NOT sufficient.** The struct-pattern bug lived in an arm
   that *existed* and did not consult the field types. The match was total; the compiler had nothing
   to complain about. Totality catches a missing VARIANT, never an under-implemented arm. The `..` in
   `Pattern::Struct { fields, .. }` was the tell — a field destructured away in the arm that most
   needed it.
2. **The walk must be total over the forms that HOLD expressions**, not just over `Expr` itself.
   Totality stops a new variant; only descending into every held expression stops a new *position*.

### Done when

- Every construct that binds or carries a value is a named arm in ONE total walker
- Adding a new binder/carrier variant **breaks the build** until its qualifier consumer is written
- The ~7 parallel value-flow walkers are reduced to one shared abstraction, or proven kept in sync
  by construction

---

## PHASE 3 — Instrument integrity

Every number in Phases 1–2 is only as good as the harness producing it. On 2026-07-27 the board was
red while being reported green, and the two headline gates were grading two different builds.

Closed already: fail-open EXPECT default (`b3b3202`), instrument drift + empty-corpus guard
(`0435070`).

Open — **15 latent traps** from GROK-HORUS's 63-harness census
(`scratchpad/fleet_20260726/horus_gate_integrity.md`):

- missing `total -eq 0` guards (capset gate, shadow diff)
- absent filename/header cross-checks (runtime gate, stdlib fail-closed gate)
- fail-open EXPECT defaults in those same two gates
- all-SKIP hollow pass: a gate reporting PASS with `agree=0`
- `grep -m1` under `set -euo pipefail` without `|| true`

Plus the audit that matters most: **can `scripts/run_seal_checklist.sh` report SEAL_PASS while a gate
was skipped, errored, measured an empty corpus, or graded a binary other than the one it pinned?** A
seal that can pass without measuring is the most expensive instance of this class, because everything
downstream cites it.

Leave `run_shadow_diff.sh:56` alone — its default makes diagnostics UNEXPECTED, which alarms rather
than hides. Already fails closed.

**Recurrence question, still unanswered:** is a shared `scripts/lib/gate_common.sh`
(`score_fixture` / `require_nonempty_corpus` / `finalize`) the right construction, or is the churn
worse than the disease? Patching 15 sites does not stop the 16th.

### Done when

- Every latent trap closed, each with a microbench that PROVES the guard fires
- The seal audited and unable to report PASS without measuring
- A verdict recorded on the shared-library question, yes or no

---

## PHASE 4 — Research mode (the other half of the product)

Safe mode is the strong half. Research mode's blocking defect: **the tart path collects no evidence
at all.** `seal_action`, `receipt`, `collect_loot`, `scrape` have zero call sites there.

Measured: `vz exploit` and `vz fuzz` produced a SIGABRT and 14 unique crashes inside disposable
guests and left `receipt-verify` **byte-identical**, while `campaign-init` — which only writes a
Markdown file — advanced the chain. The proof-carrying thesis is inverted for exactly the operations
the lane exists to prove. An operator cannot prove what they did; an auditor sees a clean engagement
with no activity in it.

Guest staging was fixed 2026-07-27 (`vz-agent-test` exit 0, `vz-fuzz --target` exit 0 with an
evidence hash). The doctor was made route-truthful in the same commit.

Open:

- Scrape the guest BEFORE teardown (`vz.rs` disposable path: clone → body → `tart stop` →
  `tart delete`, no scrape between body and delete)
- Seal the action into the receipt chain
- Stop minting the run capability with hardcoded `engagement_id: "vz-session"`
- `vz-c2-cycle` exits 1 `ANUBIS_VZ_C2_NO_RESULTS` — agent beacons and tasks queue, results stay `[]`
  across 50 polls. Lead: beacon reports `"os":"linux"` from a macOS guest, so either a misreported
  field or a wrong-architecture agent.

**Isolation is absolute and not negotiable.** Anything research-gated or crash-capable runs inside a
disposable tart guest cloned from `anubis-xcode`. Host `anubis fuzz`, host `anubis run
--allow-research` crash PoC, host `exploit-run` are FORBIDDEN as primary evidence. **Calling a host
run "isolated" is fabrication.** Tear down with `tart stop X; sleep 2; tart delete X` then CONFIRM
with `tart list` — `delete` silently no-ops on a RUNNING guest. Report
`isolation: tart-disposable-guest` + guest name. Crash isolation ≠ air-gap; no zero-NIC claim without
`native-preflight`. If tart is red: STOP, do not fall through to the host.

### Done when

- A crash op produces at least as much evidence as `campaign-init` does (the control that makes it
  conclusive)
- `receipt-verify` COUNT and TIP advance after a guest run, artifacts present on the host after
  teardown
- `vz-c2-cycle` returns results
- The residual is stated: what an auditor still **cannot** conclude from a green `receipt-verify`,
  so the fix does not become the next false assurance

---

## PHASE 5 — Builtin surface

**213 builtins** (derive by command from `run.rs`: `emit_builtin_call` + its inline `matches!` +
`is_proof_input_builtin` + `is_poc_kit_builtin` + `is_non_run_builtin`, deduplicated — do not trust
this number or the README's "~150"). The 45 fail-closed fixtures cover only the collection-first-arg
matrix. **~168 names have no domain/arity/wrong-type/IO matrix at all.**

Why this is not busywork: a builtin that returns a plausible but incorrect value instead of refusing
feeds that value into a `requires`/`ensures` and makes a contract hold **for the wrong reason** —
corrupting the proof rather than stopping it. Strictly worse than a crash.

**Never patch documented leniency**, and mark it so nobody "fixes" it later: `int`/`float`/`parse_*`
per LANGUAGE.md:518, IEEE NaN/inf from `sqrt`/`ln`/`log`/`pow`, float division by zero yielding
`inf`, `position` returning `-1`, string auto-stringify.

### Done when

- Complete deduplicated builtin set enumerated by command
- Every cell classified: `SILENT_WRONG` / `SILENT_SUCCESS` / `FAIL_CLOSED_OK` / `DOC_OK_LENIENT` /
  `CRASH`
- Fail-closed patch + fixture per defective cell; a must-stay-PASS fixture per lenient cell
- Or an honest bounded residual published, if the surface proves unenumerable

---

## PHASE 6 — Seal and honest claim

- VM seal — ROADMAP Phase 0 living residual (post-registry host fixpoint unsealed)
- Rewrite the headline promise to exactly what is discharged, scoped and explicitly NOT claiming
  totality. `CLAIMS.md`'s internal framing is already right — *"Green means no KNOWN defects, not no
  defects"* — the failure is that the headline promise does not inherit it.
- A docs gate that FAILS when the promise string drifts out of sync with the open-issues section
- Reconcile every drifted count across `README.md`, `LANGUAGE.md`, `MATURITY_CLAIM_MATRIX.md`,
  `A_PLUS_*.md`

### Done when

A skeptical outsider holding this repo can reproduce every headline number by command, and the
promise sentence survives an adversarial reading by someone holding the open-issues list.

---

## Standing rules (violating these has cost real time)

- **Zero fabrication.** Real command, real output, `file:line` for code claims. Not-run is SKIPPED,
  never PASS. Out of evidence → `[NEEDS-HUMAN]`.
- **Validate the instrument first.** "44/44 gates FAIL" was exit 127. "1764 corpus fails" were
  timeouts. "load 109" was my own orphaned busy-loops. If a result looks dramatic, suspect the
  harness before the finding.
- **Verify the committed HEAD**, not the dirty tree — a directory-scoped commit slice ignores the
  dependency graph and can land a signature without its caller.
- `./target/release/anubis`, never bare `anubis` (a shell alias hijacks it).
- `git commit -F <file>`, never `-m` with backticks. Stage explicit paths. The post-commit hook
  AUTO-PUSHES `a-plus-maturity/*`. Never `--force`.
- **The human presses send.** Agents draft disclosures; the operator transmits. No agent-pressed
  sends, no external filing/posting, no public repos.
- Agents: no git, no commits, ever. Only the lead commits. Agents must not run `cargo build` — the
  shared target dir lock stalls the fleet.
