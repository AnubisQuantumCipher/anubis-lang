# Anubis Claims (1.0 freeze — evidence-first)

See `MATURITY_CLAIM_MATRIX.md` for historical gate rows. Living freeze:
[`docs/language/SPEC_1_0_FREEZE.md`](language/SPEC_1_0_FREEZE.md) ·
[`docs/language/SEMVER_1_0_POLICY.md`](language/SEMVER_1_0_POLICY.md).

## Known open issues (baseline 2026-07-27; entries dated inline)

Any A15/A+ seal dated 2026-07-24 or earlier (`ROADMAP_A_PLUS.md`, `A_PLUS_CLOSEOUT.md`,
`A_PLUS_FINAL_REPORT.md`, and the tail of `MATURITY_CLAIM_MATRIX.md`) predates every item below —
read those files' "CLAIMED"/"DONE"/"PASS"/"COMPLETE" language as *what was true on that seal
date*, not as current. **This section is the single source of truth for current status.** Other
owned docs link here; they must not restate the list.

**A green board is when a claim surface is most dangerous.** Read the disease theme, the green
table, and "green = no KNOWN defects" with equal weight.

### The disease — proven across eight separate classes

> **A user writes something down, or a producer computes a label, and a consumer ignores it or
> recomputes it independently.**

That sentence is now **proven** across **at least eight** closed classes this arc (not eight
unrelated bugs — one disease, eight surfaces):

| # | Class | Consumer failure | Closed |
|---|---|---|---|
| 1 | Declared `-> secret/tainted` returns | Summary body-derived only | `6fb055f` |
| 2 | Declared struct **field** qualifier (R1) | Field type ignored at project | `f4d2f37` |
| 3 | (R) runtime + PCA twin | Policy re-derived from assignment provenance | PTAH / `6fb055f` |
| 4 | Stored-callable pipeline (M1) | Apply lost stored/returned callable identity | `47ab408`…`eb5be1f` |
| 5 | Multi-candidate denotation (M2) | Expression not a single Var/Lambda | `ae1fa17`…`c491237` |
| 6 | Value-block nested discharge (M3) | Stmt-`if` in value block not walked | `2168bf1` |
| 7 | **D1/D2/D3** field qualifier **through CALL result** | Place type ignored return type of free-fn/method | `f9fc7a7` |
| 8 | **D4** enum-payload qualifier at match binder | Payload declaration ignored at bind | `c9415b7` |

**Same disease, additional closed surfaces this stamp:**

| Class | Failure | Closed |
|---|---|---|
| **Research authorization bypass** | Bare `@research { … }` elevated out of Safe with **no** authorization — any Safe program could obtain research capabilities by wrapping code | `e6ebfd2` |
| **Unknown attributes** | Silently ignored (fail-open on the declaration surface) | `ec65724` fail-closed |

Prefer making the correct binding reachable over a parallel consumer.

The original **eleven** published red witnesses closed via **four mechanisms** (M1, M2-reg,
M2-B, M3) — not eleven fixes. D1–D4 and research/attr closes extend the same disease map.

### Green means no KNOWN defects — not no defects

**Green corpus = no defects in the published residual inventory. It does not mean no defects.**

Do not stamp "false-accept class closed forever," "roadmap soundness complete," or "Safe is
total." Residual composition shapes may still exist (e.g. D5 generic field instantiation, D6
struct-lit IF-at-construction from earlier HORUS census) without a current published red row.
Absence of a red row is **not** evidence of absence.

### Latest bounded working-tree evidence (technical epoch 2026-07-31 — externally activated; not a total-soundness claim)

This table combines the last stable baseline with explicitly named later working-tree receipts; it
is not a release status. The deciding technical epoch is
`0281e8034022fc62f4f853906a33173bc0286e9ae9a0e07b26d761a495962b03` on immutable compiler pin
`vm/pins/anubis-51f4a964347a`. The external receipt
`out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md` proves the required source-current
VM/offensive/921-row-diff refreshes and exact docs-bound host seal; its independent review records
`APPROVE` with no blocking finding and zero source writes. The frozen report's predicate is therefore
satisfied: **Phase 1 is bounded COMPLETE / ACTIVATED** for source tree
`b3b5bfd8e472aec45856ff95a6d307670c20083c620f9971f90e5d4ce50be1a1`. This reconciliation moves
the live tree beyond that exact epoch. No landing, release, shipping, or total-soundness claim
follows, and the dirty-epoch pin is not eligible for a tagged release.

**Historical tip commits:** `ec65724` (unknown attr fail-closed) · `e6ebfd2` (research auth bypass) ·
`c9415b7` (D4) · `f9fc7a7` (D1/D2/D3). Later rows explicitly name the immutable technical-epoch
instrument; do not substitute mutable `./target/release/anubis`.

| Surface | Observation | Repro / boundary |
|---|---|---|
| **Security fixtures** | Lead gate **327/327 PASS**. Live disk inventory **327** `.anb`; **published red list EMPTY** (0 `EXPECT: FAIL` still check-PASS this pass) | Green ≠ no bugs. Re-enumerate command below. |
| **Language core** | **253/253 PASS** — Phase 1 adds the research-block local-field accept fixture; the earlier 252/252 receipt remains historical. See the float-lane residual below | pin `ANUBIS_BIN` (§6) |
| **Stdlib fail-closed** | **104/104 PASS** | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/…` |
| **Capset selfhost** | **5/5 PASS** | `bash scripts/run_capset_selfhost_gate.sh` |
| **Taint / type / effect selfhost** | **0 disagreements** each | lead-verified |
| **Formal gate** | **PASS** — every theorem machine-checked; **no `sorry` / `admit` / free `axiom`** | `bash scripts/run_formal_gate.sh`; Lean **162 theorems / 15 modules** (comment-stripped) |
| **Native authoritative** | **PASS over 926 files, 0 mismatches** (current corpus; the earlier 2026-07-29 ratchet raised 906 → 916) | `bash scripts/run_native_authoritative_gate.sh` |
| **Unified gate suite** | **22/22 PASS** at commit `4e7ee94` — 0 failed, 0 skipped, 0 external, `tree_state: clean` | `bash scripts/audit_head.sh --rev <sha>` — grades a COMMIT in a throwaway worktree, not the live tree |
| Research elevation | Bare `@research` **without** authorization → REJECT | Live: `research_block_without_authorization_rejects.anb` EXIT=1 |
| Unknown attributes | **Fail closed** | Live: `unknown_attribute_rejects.anb` EXIT=1 |
| Ordinary Safe `run` | Vault contacts EXIT=0 post-PTAH | Proof/shell non-run by design (§2 B) |
| VM seal of post-registry fixpoint | **SEALED for the named technical epoch** — fresh **22/22** run on 2026-07-31 (guest `anubis-run-23962`), 0 gate failures, fixpoint `46ddce14…ba60` == `scripts/vm/EXPECTED_FIXPOINT_VM`, source identity stable before/sync/after, strict validator PASS, verified teardown; earlier receipts remain historical | `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z`; this is distinct from the external docs-bound finalization seal and is not a landing or shipping claim |

#### Honest-number methodology

**A full green gate is an empty published residual inventory, not a proof of total Safe soundness.**

- **Do not** rewrite as "100% secure."
- **Do not** quote older stamps (219/219, 216/219, …) as current without re-run.
- **Do** re-enumerate after any checker change.
- **Do** seal with one pinned `ANUBIS_BIN` (§6).

```bash
# expect ZERO lines if published red inventory still empty
for f in examples/security/*_rejects.anb; do
  ./target/release/anubis check "$f" >/dev/null 2>&1 && echo "RED $f"
done
find examples/security -name '*.anb' | wc -l
```

Counting rules: **Lean = 162 / 15**. **Builtins ≈ 213** (five-function union).

### Open — load-bearing (blocks honest "complete")

1. **Composition residuals — D1–D6 closed; the class is NOT claimed total.**
   D1–D4 closed earlier this arc. **D5** (generic field instantiation) and **D6**
   (IF-at-construction) now have fixtures and are closed (2026-07-27), which is what this entry
   demanded — "not claimed closed unless fixtures + mechanism land":

   | shape | fixture | verdict |
   |---|---|---|
   | D5 `Box<secret<i64>>` field via instantiation | `d5_generic_field_instantiation_rejects` | REJECT |
   | D5 instantiation in a PARAMETER's declared type | `d5_generic_field_via_param_rejects` | REJECT |
   | D6 construction inside a value-position `if` | `d6_if_at_construction_rejects` | REJECT |
   | D6 construction inside a `match` | `d6_match_at_construction_rejects` | REJECT |

   Each ships an over-rejection guard (`d5_generic_field_plain_accepts`,
   `d6_if_at_construction_public_accepts`) so a future instantiation fix that marks EVERY generic
   field, or every join-constructed struct, is caught rather than shipped.

   **Still not a totality claim.** These six are the shapes that were NAMED. The
   function-identity carrier family closed the same day (bare name, alias chain, if/match join,
   list/struct/map/enum element, return, identity forwarder, pass-through builtin, argument
   position) with one carrier still open — see boundary item B4. Green board does not invent
   completeness.

2. **check/run divergence — (R) CLOSED; (B) residual named.**  
   **(A)=0, (B)=7, (R)=3**; (R)+PCA **CLOSED**. **(B)** non-run by design — do not equate
   `check` PASS with ordinary `run` for shell/symbolic.

3. **Self-host registry — HOST-FIXED; VM seal DONE (corrected 2026-07-29).**  
   This read "VM seal pending / do not publish post-drift host fixpoint as sealed" while the
   § *VM seal* section below already recorded SEALED at 19/19 on 2026-07-28. The seal is real:
   re-run 2026-07-29 in guest `anubis-run-14791` gave **22/22 gates `EXIT=0`**, `gate failures : 0`,
   fixpoint `46ddce14…ba60` matching `scripts/vm/EXPECTED_FIXPOINT_VM` — the battery has grown
   19 → 22 gates (`walker`, `formal-kernel`, `correspondence`) since the seal was taken.
   **Two stale "pending" cells outlived the event they were gating.** Recorded rather than quietly
   flipped: a status line that contradicts a section of the same document is the disease this file
   exists to catch, and it survived here for a day.

4. **Permanent external/TCB residuals — OPEN and named.** Keychain/Secure Enclave enforcement
   remains dependent on OS signing, entitlements, hardware, and the Apple security boundary;
   Softnet hostname policy retains the HARD post-pin DNS-rebind residual; DDC is not TT-total;
   hosted CI does not prove Metal execution; and native general free/signed non-power-of-two
   division remains deferred. Unit or hosted greens must not silently close these boundaries.

5. **REG-001 — precondition fail-open on opaque-provenance arguments — CLOSED (2026-08-13, PR #16).**
   The four-way `is_bool_modelable_*` cascade in `compiler/src/middle/mod.rs:7527-7639` used to drop
   the caller-side precondition obligation entirely when a clause was judged unmodelable in all
   four lanes (int / float / string / strlen). The terminal `else` at `:7637-7639` set
   `all_requires_checkable = false` without emitting an obligation — so a green `anubis check`
   did not guarantee call-site preconditions held when the argument's provenance was opaque to
   the solver (e.g. the return value of an uncontracted function).

   Reproducer (adversarial-eval preserved 2026-08-13, built from `6f4a141c`):

   ```text
   fn produce() -> i64 { return 0 - 42; }
   fn needs_pos(x: i64) -> i64 requires(x > 0) ensures(result == x) { return x; }
   fn main() { let v = produce(); let r = needs_pos(v); print(r); return 0; }
   ```

   Pre-fix: `anubis check` → **passed**. `anubis run` → prints **-42**, exit 0. Precondition
   `x > 0` violated at runtime with no trap.

   **Fix landed 2026-08-13 (PR #16, commit `898c31ff`).** The `Stmt::Let` handler for a
   `Call` initializer now registers the bound name in `ctx.solver_int_vars` whenever the callee
   has a declared integer return type, even with no `ensures`. The concrete-ensures loop still
   applies its constraints when non-empty; the change only removes the "empty ensures" bypass.
   Post-fix on the same reproducer: `anubis check` → `ANUBIS_ASSERTION_DISPROVED` on
   `requires@needs_pos:(bvsgt anb_v (_ bv0 64))` with counterexample `v = 0`. The
   `docs/PROOF_CORRESPONDENCE.md:55-57` acknowledgment ("A value wrongly judged unmodelable
   silently drops its obligation") no longer applies to this class.

   Downstream demo ripple: the strengthened checker exposed an unstated invariant in
   `examples/programs/formal_kernel/{formal_kernel,formal_kernel_hard_tests}.anb`, where
   `dpll`'s "try false" branch computed `0 - br` on a value from `pick_branch_var(...)`. Fixed
   in the same PR by applying the golden `wrap_guarded_negation_accepts.anb` pattern (explicit
   `br == i64::MIN` early return + positive-form `br > 0` guard). G25 formal_kernel gate is
   green on the fixed binary; 1215+ `cargo test --release` and 258/258 G5 language fixtures
   also green.

6. **REG-002 — z3-only fragment is forgeable by a compromised z3 — MITIGATED (2026-08-13).**
   Obligations outside the native Lean-proven fragment (division, remainder, nonlinear beyond
   native, floats, strings, quantifiers) fall through to z3 alone. Pre-mitigation: if z3 answered
   `unsat` (= "proven"), the runtime trusted it with no certificate check and no replay — replay
   only triggers on `sat`/counterexamples. A malicious z3 that answered premise-satisfiability
   honestly and forged `unsat` on the proof goal made a false division contract pass. Threat
   model: attacker controls the z3 binary.

   Reproducer (adversarial-eval preserved 2026-08-13, malicious z3 script preserved separately):

   ```text
   fn div_lie(a: i64, b: i64) -> i64
       requires(b != 0)
       ensures(result * b == a)     // false in the general case
   { return a / b; }
   ```

   Under a stock z3 the ensures fails; under a smart malicious z3 that reports `unsat` on the
   z3-only fragment, `anubis check` returned PASS pre-mitigation. The native fragment stays
   protected — a lying z3 on a native-decidable goal is caught by `ANUBIS_NATIVE_DISAGREE`; the
   residual was the z3-only surface.

   **Mitigation landed 2026-08-13.** Two opt-in env-var controls exposed at
   `compiler/src/middle/mod.rs`'s `run_z3_obligation_with_smt` fall-through:

   - `ANUBIS_Z3_ONLY_LOG=<path>` — audit trail. Writes one JSONL record per z3-only obligation
     (SMT + verdict, verbatim) so the operator can enumerate exactly which obligations relied
     on z3-only trust and diff them against the native-decidable fragment offline. Sound-by-
     construction: the log never influences the returned verdict.
   - `ANUBIS_REQUIRE_NATIVE_PROOFS=1` — fail-closed. Refuses to trust z3 alone: any obligation
     the native authoritative solver declines returns `FAIL` with
     `ANUBIS_Z3_ONLY_UNTRUSTED` before z3 is even spawned. This narrows the trust to the
     machine-checked native fragment plus a documented, refusable "z3-decided, unchecked"
     class, exactly as this entry previously named as the acceptable Phase-4 outcome.

   Regression fixture: `tests/fixtures/language_core/z3_only_log_records_declined_obligation.anb`.
   Rust integration test: `tools/anubis/tests/reg002_z3_only_mitigation.rs` — locks both the
   default-passes + log-records path AND the require-native fails-closed path end-to-end through
   the real CLI.

   **Remaining residual — full-cert replay.** A checkable UNSAT proof certificate parsed and
   replayed in-process (DRAT/LRAT for QF_BV, or z3's own proof format) is still not implemented.
   For high-assurance builds, the operator sets `ANUBIS_REQUIRE_NATIVE_PROOFS=1` and either
   accepts fewer decided contracts (safe: fail-closed) or rewrites obligations to land in the
   proven fragment. That is the correct tradeoff surface for this class of residual.

7. **REG-003 — linear capability double-spend across a parameter boundary — CLOSED (2026-08-13, PR #15).**
   Pre-fix: `cap_use(tok); cap_use(tok)` on a token acquired LOCALLY was caught with
   `ANUBIS_CAPABILITY_REUSE`. The identical double-use on a token received as an UNTYPED
   PARAMETER was not caught — the capability-linearity check did not track consumption count
   across the caller/callee boundary when the token flowed in as an opaque parameter.

   Reproducer (adversarial-eval preserved 2026-08-13):

   ```text
   fn spend_twice(tok) { cap_use(tok); cap_use(tok); }
   fn main() { let t = cap_mint("pay:100"); spend_twice(t); return 0; }
   ```

   Pre-fix: `anubis check` → **passed**. Intra-procedural form
   `fn main() { let t = cap_mint("pay:100"); cap_use(t); cap_use(t); return 0; }` correctly
   returned `ANUBIS_CAPABILITY_REUSE`; the interprocedural form was the failing side.

   **Fix landed 2026-08-13 (PR #15, commit `2eaf6cd2`).** The store-then-project path in
   `compiler/src/middle/capability.rs` at `note_container_ne_mutation` was silently draining
   unknown-provenance callee args; the walker now tracks the callee's use as a real consumption
   even when the token's provenance is a formal parameter. Post-fix on the reproducer:
   `anubis check` → `ANUBIS_CAPABILITY_REUSE`. Regression fixtures
   (`tests/fixtures/language_core/capability_double_use_via_param_rejects.anb` and the
   accepting variants) landed alongside.

   **The unifying theme across REG-001 / REG-003.** Both defects lived at the interprocedural
   boundary with an opaque value (an uncontracted return; a parameter-passed token). Anubis's
   static analyses are strong intra-procedurally and over solver-modelable values, and weakened
   exactly when a value crossed a function boundary as a dynamic parameter or return. This is
   the exact disease Phase 2 of `docs/COMPLETION_BLUEPRINT.md` names (*"replace duplicated
   value-flow walkers with one total, lane-parameterized mechanism"*) and Phase 3 addresses on
   the security-label side (*"separate the security-label lattice from accept-biased type
   inference"*).

   **Status 2026-08-13.** REG-001 CLOSED (PR #16, `898c31ff`) — the modelability cascade now
   registers the return of any int-typed call as a solver var, so downstream precondition
   obligations are checked. REG-003 CLOSED (PR #15, `2eaf6cd2`) — capability linearity now
   tracks consumption across parameter boundaries. REG-002 MITIGATED (item 6 above): an audit
   trail (`ANUBIS_Z3_ONLY_LOG`) and a fail-closed refusal path (`ANUBIS_REQUIRE_NATIVE_PROOFS=1`)
   let the operator either enumerate the z3-trust surface or refuse it entirely; the full
   in-process UNSAT-certificate replay is named as the remaining residual and is a genuine
   Phase 4 architectural item.

   The prior instance of the same class caught in this arc was the mutual-recursion identity
   walker DoS (PR #9): identity walker A's fix did not propagate to the sibling walker family
   because the family lost depth across a helper's depth-0 wrapper. Same shape, different
   walker.

### Phase 1.5 — sealed VZ + metal-prove workflow jobs are explicitly OUT-OF-CI (2026-08-12)

Two jobs in the GitHub Actions workflows target label sets that no GitHub-hosted
runner will ever match: `sealed-vz-gate-suite` in `.github/workflows/ci.yml` calls
`runs-on: [self-hosted, macOS, ARM64, tart-vz]`, and the entire `metal-prove.yml`
workflow calls `runs-on: [self-hosted, macOS, ARM64, metal]`. If no self-hosted
runner with matching labels is registered, both queue and never execute; if one
is registered on a machine that lacks the underlying hardware (Tart/VZ substrate;
Metal-capable GPU), the job runs but produces no meaningful evidence.

**This is deliberate.** The substantive evidence for both lanes is produced OFFLINE,
by running the same scripts the workflow steps would call, on a machine the
operator physically controls, inside a disposable Tart guest per
`docs/language/POC_KIT.md`:

- **Sealed VZ battery** — `bash scripts/run_seal_checklist.sh --profile core --bin <pin> --out <dir>`
  produces `seal_verdict.json` (declared-verdict-line scoring, corpus-completeness
  check, refuses overall PASS on any instrument-precondition miss) and per-gate
  `.score.json` receipts. This is what a Phase 1 finalization root is composed of,
  e.g. `out/phase1_finalization_51f4_r2_20260731T230000Z/`.
- **Metal prove gate** — `bash scripts/run_metal_prove_gate.sh` produces
  metal-parity evidence when Metal hardware is present; on hosts without Metal
  it refuses rather than silently degrading. Cross-check against CPU via
  `bash scripts/check_metal_parity.sh` (see `docs/METAL_BACKEND.md` for the full
  pipeline).

**The absence of an in-CI green check for these jobs is intentional. Presenting a
permanently-skipped job as passing would be worse than not running it.** GitHub
Actions reports both jobs as `queued` indefinitely; the workflow file's
`runs-on` clause is the honest label; the offline evidence is the honest source.

The substantive off-CI evidence lands attached to Releases (source manifest bound
by pin SHA, seal verdict JSON, per-gate score JSONs, independent-review verdict)
so that a third party can reverify against a tagged commit without depending on
GitHub's CI infrastructure.

Registering a self-hosted runner with matching labels remains an OPEN option
per `docs/COMPLETION_BLUEPRINT.md` phase 1.5 criterion 6; it is not a
requirement for the current evidence pipeline to be honest. Any such runner
registration must first document its untrusted-PR-run policy, its sandbox
scope, and its credential surface, per the security concerns in
`docs/language/POC_KIT.md`.

### Phase 5 closed — builtin surface, and the instrument that was measuring it (2026-07-28)

**Commit `1a19479`.** 213 builtins, every cell classified by the PAIR (`check`, `run`):

| class | count | meaning |
|---|---|---|
| `FAIL_CLOSED_OK` | **179** | `check` refused — the fully honest cell |
| `RUN_REFUSES` | **11** | `check` accepted, run refused with a structured `ANUBIS_*` code |
| `RUNS` | **23** | ran; correctness not asserted by this matrix |
| `CHECK_FA_CRASH` | **0** | — |
| `RUN_FAILS_UNSTRUCTURED` | **0** | — |

Reproduce: `bash scripts/classify_builtin_surface.sh <names> docs/evidence/builtin_surface_matrix.tsv`

**The 13 cells previously filed as `CHECK_FA_CLEAN` were mislabelled by this repo's own
instrument.** Its header asserted a stronger promise than the published one — *"a program that dies
at runtime is one `check` should have rejected"*. Totality is not in the promise sentence, and a run
that stops with a structured refusal did exactly what its second clause requires: it refused.
Grading those as violations puts standing pressure on the next person to weaken the runtime until
the board looks green. Nothing was forgiven in the fix: panics remain `CHECK_FA_CRASH`, and a
non-zero exit with no `ANUBIS_` code is `RUN_FAILS_UNSTRUCTURED`.

**Two of the thirteen were real and are closed.** `break`/`continue` outside any loop passed `check`
and refused only at run time. `compiler/src/middle/loopctl.rs` rejects them with
`ANUBIS_LOOP_CONTROL_OUTSIDE_LOOP`; it matches all 15 `Stmt` variants with **no wildcard arm**,
extending to statements the compile-time totality `carrier.rs` gives expressions. Deliberately
conservative: a `break` inside a lambda is **not** reported — guessing there would cost a false
rejection, which is worse than the runtime refusal that already exists.

**Remaining 11 = published bounded residual** (`symbolic`, `sql`, `memcpy`, `argon2id_hash`,
`hybrid_open`, `declassify`, `proof_input_{bool,u32,u64}`, `proof_commit_{bool,u32}`): proof-lane and
native-lowering constructs with no run-lane implementation. Phase 5's own "Done when" permits an
honest bounded residual; this is it. **Not claimed:** that `check` is complete about runnability.

**Instrument defect, disclosed:** `anubis run` inside the classifier's `while read` loop inherited
the loop's stdin and consumed names out of the list — a 213-name list measured 92 rows, and the
missing 121 appeared not as failures but as if never asked about. Fixed with `</dev/null` on both
legs plus a row-count conservation check that refuses to report unless it answered about every name.
The published 177/13/23 was re-measured and found **correct** (the original run was backgrounded, so
its stdin was already `/dev/null`) — verified, not assumed.

**Enforcing change, verdict-diffed:** all **895** `.anb` under `tests/fixtures` + `examples`, pinned
baseline vs new binary — 473 accept / 422 reject on both sides, **0** accept→reject and **0**
reject→accept.

---

### Phase 6 — run_failclosed green as a WHOLE, and two ratchets that were lying (2026-07-28)

> **Historical result only.** This records what commit `5040f41` measured at that point in time;
> it is not a present-tense whole-runtime claim. The current inventory and gate verdict control.
> While any `OPEN`, `UNENUMERATED`, semantics-only, or uncovered non-run row remains, the strongest
> available verdict is `PASS_INSTRUMENTED`, not `PASS_RUNTIME_FAILCLOSED_WHOLE`.

**Commit `5040f41`.** `GATE: PASS_RUNTIME_FAILCLOSED_WHOLE` — 104 closed corpus, 11 permanent
controls, 5 graduated, 2 enforcement, 23 doc_ok IEEE; `blocks_whole_open=0 open=0 unenumerated=0`.

Two defects in this repo's own coverage ratchets, both introduced with the ratchet in Phase 3:

1. **One floor file for five corpora.** `run_runtime_fixtures.sh` is a shared runner invoked over
   five directories; a single floor ratcheted to the largest bucket (23) and then failed every
   smaller one — `passed=23/23 failed=0 rc=1`. Floors are now keyed by directory+glob.
2. **A ratchet pointed at the breakage instead of the ledger.** It ratcheted `open_count`; closing
   the last three residuals drove it 3 → 0 and the gate failed *for fixing what it tracks*. It now
   ratchets the TRACKED ledger, which must never shrink.

Poison-tested rather than trusted: dropping a ledger row → `FLOOR: FAIL`; reopening a
`BLOCKS_WHOLE_CLAIM` row → `claim_run_failclosed_as_a_whole: false` and the gate honestly degrades to
`PASS_INSTRUMENTED`; restore → whole claim returns.

**G24 promise coherence** (commit `646af26`, `scripts/run_promise_coherence_gate.sh`). Every
restatement of the headline promise must carry, near it, a scope qualifier **and** a pointer to
`docs/CLAIMS.md`; `CLAIMS.md` must itself keep the "no KNOWN defects" framing and a **non-empty**
open-issues section — an empty list would make every qualifier vacuous, since pointing at nothing
reads as "nothing is open". It caught a real drift on first run (`docs/HANDOFF.md:20` stated the
promise with no scope qualifier); fixed in the doc, not the gate. Watched to fail four ways
(self-test, framing removed, open-issues emptied, restatement count fallen).

### VM seal — SEALED on a fully green battery, 19/19 (2026-07-28)

`scripts/vm/run-slice.sh`, disposable tart guest cloned from `anubis-xcode` (8 vCPU; the host never
builds). Final run:

```
gate failures : 0
VM fixpoint   : 46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60
expected      : 46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60
  ✓ fixpoint matches baseline
PASS — all gates green, fixpoint unchanged.
```

**Historical 2026-07-28 receipt: all 20 gates `EXIT=0`** — cargo-test, tool-test, clippy,
build-rel, language 252 passed of 252, turing,
security 317/317 (**historical stamp, 2026-07-28**), stdlib 10/10 (guest lane), shadow,
seal (stage2 == stage3), dogfood,
effect/capset/type/taint self-host (0 disagreements each), stdlib-fc 104/104, native-auth,
docs-drift, walker, **formal**.

**`formal` had never executed in the guest before this.** It was exit **127 — toolchain absent**: the
golden image carried no lake/elan, so the 162 Lean theorems were only ever machine-checked on the
host. elan + Lean v4.32.0 are now installed in `anubis-xcode` and `FORMAL_GATE: PASS` there. An
absent toolchain reported as a failed proof sends the reader to debug the wrong thing, so
`run-slice.sh` now names exit 127 separately from a failed check.

**The re-baseline was held back twice before being taken.** This same digest was measured with the
battery at 17/19 and again at 18/19, and `EXPECTED_FIXPOINT_VM` was NOT written either time — that
file records the digest a **green** battery produces, and writing it on a red board puts a number
behind the word "sealed" that no green run ever made. It was written only once the board was green.

**Reproducible before it was written:** the identical digest came out of **four** independent
clone→boot→build cycles in throwaway guests (`.64.7`, `.64.3`, `.64.5`, `.64.6`). A digest seen once
is a measurement; seen four times across disposable guests, it is a fixpoint.

**Claimed:** stage2 == stage3 self-host reproduction, a reproducible VM binary fixpoint, and every
gate in the battery green in a disposable guest. **Not claimed:** trusting-trust closure, or that a
green battery means no defects — see "Green means no KNOWN defects" above.

---

### `std.testing` used the PROOF construct for RUNTIME assertions — CLOSED (2026-07-28)

**Commit `1e8f76b`.** `scripts/run_stdlib_gate.sh` went 7 pass/4 fail → **10 pass/1 fail**. The
survivor is `edges_all_modules`, and it is published rather than papered over.

`testing::assert_eq(a, b)` is `if a != b { assert(false); }`. Checked as a standalone function with
**free** params, `assert(false)` genuinely IS reachable — the checker is correct, and an earlier
reading of this as a false reject was wrong. The gap is that the helper states no precondition; the
principled fix, `requires(a == b)`, does not work today because the params are **untyped** and an
untyped equality is outside the modelable set, so the `requires` never becomes a solver assumption.

Demonstrated in both directions rather than asserted:

```
fn helper(a, b)           requires(a == b)   ->  ANUBIS_ASSERTION_DISPROVED (requires not modelable)
fn helper(a: u32, b: u32) requires(a == b)   ->  discharges; `anubis run` exits 0
```

**That diagnosis was wrong, and the correction is the finding.** The problem was never the type
system — it was the CONSTRUCT. `assert(c)` is a PROOF obligation ("prove `c` on every path, refuse to
build otherwise"); `panic(msg)` is a RUNTIME abort carrying no obligation. A test asserts a fact
about ONE concrete run; it does not claim the fact is provable for all inputs, and `assert` claims
exactly that. `std.testing`'s header has read "RUNTIME assertion helpers" since it was written — only
its body disagreed, which is this repo's recurring producer/consumer split appearing between a
module's docstring and its implementation.

Both repairs tried first (`requires(a == b)`, typing the params `u32`) were attempts to make an
obligation dischargeable that should never have existed. Switching the six helpers to `panic` gave up
nothing verified: the obligation was never dischargeable and never intended, and a caller who wants
the checker to PROVE something still writes `assert(...)` directly.

Enforcement verified in BOTH directions, because this change could silently turn every test
assertion into a no-op:

```
assert_eq(2 + 2, 5)  ->  rc=1, ANUBIS_PANIC "assert_eq: values differ"; the next line never runs
assert_eq(2 + 2, 4)  ->  passes; edges_all_modules runs to completion
```

**`stdlib gate: PASS (11 pass, 0 fail)`**, up from 7/4 at the start of this arc.

Two adjacent findings from the same gate, both fixed: `math_mul` had a genuine i64 wrap
(`4294967295^2` ≈ 1.8e19 vs a 9.2e18 ceiling) and now carries the checker's own counterexample-derived
bounds; and the crash-PoC step was calling `run --allow-research` **on the host**, which the runtime
refuses — it now routes through `anubis vz exploit` into a disposable guest with the crash op sealed
into the receipt chain (`seq=2`), guest discarded. Where nested virtualization is unavailable that
lane records an explicit SKIP naming the reason: a crash PoC that never ran isolated is not evidence.

---

**Counts reconciled** (commit `ae74be4`): language **247/247** (raised from a historical 244/244),
native corpus **898** (raised from 888)
across `AGENTS.md`, `README.md`, `CLAIMS.md`. Two of the nine flagged stamps were **historical**
records bound to commit `c7643e5` and pin `anubis-cf98ccebb4c1`; their numbers were left untouched
and their existing historical framing moved onto the stamp's own line, because renumbering a past
measurement to clear a gate would assert that a past run observed something it did not. That took
live-checked stamps 37 → 35, the coverage floor caught it, and the floor was lowered in the same
commit with the reason — the mechanism working, not being worked around.

---

### `anubis run` fail-closed — the bounded residual (2026-07-27)

Adopted verbatim from the agent that measured it, in preference to any whole-surface green number:

> **`anubis run` fail-closed is instrumented for the collection-first-argument matrix and the sealed
> domain/wrong-type cells under `tests/fixtures/stdlib/*should_fail_closed.anb` (86 fixtures after
> A–L + M–Z merge + elevation corpus), plus DOC_OK locks under `tests/fixtures/stdlib/doc_ok/` (23 fixtures). It is NOT
> fail-closed over the full 213-builtin surface: crypto/hash/KDF/random/x25519, unenumerated
> arity/IO/capability matrices, and intentional soft conversions (`int`/`float`/`parse_*`, string
> auto-stringify, IEEE NaN/inf on real floats, position-predicate −1, empty sum/product monoids)
> remain outside that claim.**

Coverage is a three-way union, stated so it is auditable rather than assumed:

| Slice | Names | Status |
|---|---:|---|
| A–L | 107 | sealed, 15 fail-closed fixtures |
| M–Z non-crypto | 87 | sealed, 16 fail-closed fixtures |
| crypto / hash / KDF / random / x25519 / `pwn.anb` | 19 | **UNMEASURED** |

**213** is derived by command from `run.rs` (the deduplicated union of `emit_builtin_call`, its
inline `matches!`, `is_proof_input_builtin`, `is_poc_kit_builtin`, `is_non_run_builtin`), not from the
README's "~150".

31 builtins were returning a plausible WRONG value at `rc=0` rather than refusing — `factorial("5")`
→ `120`, `pow("2", 3)` → `8`, `len(42)` → `0`, `substr("hello", -1, 2)` → `he`,
`times("2", |i| i)` → `[0, 1]`. That is the failure mode that corrupts a proof instead of stopping
it: the value reaches a `requires`/`ensures` and makes the contract hold for the wrong reason. All 31
now fail closed; documented leniency is unchanged and locked by must-stay-PASS fixtures, verified by
the distinction `sqrt("x")` FAILS while `sqrt(-1.0)` still returns NaN.

9. **Capability double-spend across a function boundary — NOT A DEFECT (withdrawn 2026-07-27).**
   I recorded this as open earlier the same day and I was WRONG. The probe put `@verified` on the
   callee (`spend`) rather than on the function holding the causal spend (`f`). Verification is
   PER-ITEM (`compiler/src/middle/mod.rs:2809-2814`): capability linearity counts ordinary argument
   occurrences in both lanes, but privileged-builtin CAUSAL SPEND is enabled only in the verified
   lane. So the default check correctly saw one consume, not two.

   With `@verified` on `f`, the same program rejects with `ANUBIS_CAPABILITY_REUSE` on the current
   pinned compiler. The corrected fixture is
   `cap_causal_then_userfn_corrected_rejects.anb`, now in the corpus.

   Caught by the AUDITOR agent, which refused to write the fix I asked for and produced the deciding
   `file:line` instead. Recorded rather than deleted: a false OPEN item in this list is a fabrication
   in the direction of alarm, and the list is only useful if wrong entries are retracted as visibly
   as right ones are added.

10. **Contract `requires` through a fn-value carrier — CLOSED 2026-07-27.**
    `fn app(g){print(g(-1));} app(f)` with `f` declaring `requires(x > 0)` accepted and printed -1.
    Closed by a policy-neutral identity summary that discharges the callee's precondition when the
    candidate resolves to a SINGLETON user function, and defers otherwise.

    Singleton-only is deliberate and is the SECOND attempt. A full set-valued policy was built,
    applied, measured and REVERTED: it closed the gap and passed all four hand-written guards while
    flipping NINE `_accepts` fixtures — including `anvil_threat_hunter`, a demo application. Every
    flip was `ANUBIS_CONTRACT_CALLEE_UNKNOWN`: the policy rejected an unresolved dynamic callable
    even where no contract had been proven. Fail-closed on unknown is right for a LABEL and wrong for
    a contract, where "unknown callee" is the normal state of higher-order code.

    A real namespace bug surfaced with it and is fixed: `recv.method(args)` was recorded as applying
    a function-valued formal merely because the receiver expression mentioned a formal, which
    accounted for the three method-shaped flips.

    Verdict-diff measured at this close on the then-current pin: security 311/311; language
    244/244. Zero accept→reject flips.

### The singleton contract policy — documented residual (2026-07-27)

Item 10 shipped as SINGLETON-only after the set-valued attempt was reverted. Scored against the
shipped binary by the adversary, from predictions written before it existed:

| behaviour | status |
|---|---|
| single named callee through any carrier | DISCHARGED |
| 10-hop alias chain | resolves to the function; no Empty-vs-Unknown bug |
| mutual return cycle | terminates in ~6ms; no checker hang |
| multi-name `if`/`match` join | **DEFERS** — accepts without discharging |
| container-of-join, wide match | **DEFERS** |

The deferrals are the intended under-approximation, not an oversight: a join of two contracted
functions with different preconditions cannot be discharged against one arbitrarily chosen name, and
fail-closing on it would reject idiomatic higher-order code — the exact failure that flipped nine
fixtures in the first attempt.

A `check` ACCEPT on a deferred join is a FAIL-OPEN DEFERRAL, not a proof that the contract holds.
That distinction is the honest reading of item 10's closure.

**Runtime residual, separate from the checker:** `run` of a mutual-return cycle LOOPS. The checker
correctly terminates and defers; the runtime does not. A program `check` accepts whose execution
never terminates is a check/run divergence of a different kind, and is being characterized.

11. **Nested-call argument `requires` discharge — CLOSED 2026-07-27.**
    `need(neg())` where `neg` declares `ensures(result < 0)` and `need` declares `requires(y > 0)`
    accepted and printed -5. Closed: the inline call-argument form now routes through the same
    reasoning that already handled the `let`-bound form.

    The distinction that makes it safe, and the same one that governed item 10:

    | argument | verdict |
    |---|---|
    | `neg()` with `ensures(result < 0)` — CONTRADICTS the precondition | REJECT |
    | `opaque()` with NO `ensures` — unknown | **ACCEPT** |
    | `pos()` with `ensures(result > 0)` — satisfies | ACCEPT |

    An unknown value is NOT assumed to violate. Only a declared postcondition that contradicts the
    precondition rejects. Getting that backwards is what flipped nine fixtures in the first spine
    attempt.

    Verdict-diff measured at this close on the then-current pin: security 311/311; language
    244/244. Zero accept→reject flips.

12. **The bare-builtin carrier defeats the LETHAL TRIFECTA detector — CLOSED (2026-07-27).**

    Fixed by making builtin identity a TAG carried on the value rather than a name read at the call
    node. Tags (`TaintSource`, `SecretSource`, `EgressSink`, `IntegritySink`, `Capability(..)`) form
    a monoid; names are only the SEED; join is UNION, so a value that is `input` on one branch and
    pure on the other keeps the tag. `Known(∅)` proves a value carries no gate class and `Unknown`
    means unresolved — an unknown value is not assumed to violate, so a builtin bound but never
    applied still ACCEPTS. Measured at this close: security 311/311 with the held RED now rejecting and zero
    accept→reject flips among the 308 that predate it; language 244/244 (historical stamp, measured at
    this close); compiler lib 731/731. Commit
    `c7643e5`.

    **With this, all five NAME-keyed leaking surfaces from the 19-surface census are closed.**

    Original text:

    ```anubis
    fn go(g) uses(net.send) { let u = g(); send("h", 80, u); }
    fn main() uses(net.send) { go(input); }     // ACCEPTED
    ```

    `input` is a taint source. Passed as a first-class value it loses that identity, laundering BOTH
    `ANUBIS_LETHAL_TRIFECTA` and `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`.

    **Severity, corrected 2026-07-27:** this is a CHECK fail-open, NOT a runtime-witnessed bypass.
    `run` refuses the program with `ANUBIS_UNSUPPORTED_NATIVE_LOWERING: builtin input cannot be used
    as a first-class value`. My first write-up implied the trifecta could actually be exercised; it
    cannot be, on this runtime. What breaks is `check` — which is the promise NEXUS and this document
    rely on, so it remains open — but the distinction between "the checker certifies a policy
    violation" and "an attacker gets the data" is exactly the kind this file exists to keep straight.

    The alias form `let g = input; g()` is the same static hole (also check-ACCEPT).

    **Not a new defect** — the known bare-builtin gating gap reaching a third surface. The
    discriminator proves the carrier machinery itself is sound and only BUILTIN identity is lost:

    | probe | verdict |
    |---|---|
    | `go(secret_read, dirty)` — both USER fns | REJECT |
    | `go(dirty)` — user-fn taint source | REJECT |
    | `go(input)` — BUILTIN taint source | **ACCEPT** |

    Recorded separately because the SEVERITY differs from the `write_file` cases already counted: a
    laundered trifecta is the failure the dual-use design exists to prevent. The user-fn control is in
    the corpus (`trifecta_via_userfn_carrier_rejects`); the RED fixture is held out until fixed.

    Fixing it needs the builtin-effect-tag resolution the adversary identified — the user-function
    identity spine structurally cannot serve it, because a set of user functions has nothing to put
    in it for `input`.

13. **`run` aborted the process on a mutual-return cycle that `check` ACCEPTS — CLOSED (2026-07-27).**
    Fixed: a stack-bytes guard (768 MiB of the 1 GiB worker stack) turns it into an attributable
    `ANUBIS_RECURSION_LIMIT` trap that exits non-zero, instead of `fatal runtime error: stack
    overflow, aborting`. `check` still does not prove termination — that is unchanged and is not
    claimed — but a non-terminating accept now FAILS CLOSED like every other runtime trap rather
    than killing the process without a diagnostic. The zkVM guest lowering is deliberately left
    unguarded rather than guarded on a stack size this repo does not measure.

    Original text:
    Measured: genuine infinite recursion after a `check` ACCEPT, ~100% CPU, RSS flat at ~6MB over 7s
    (so a stack-overflow abort does not rescue it). The checker correctly terminates in ~6ms and
    DEFERS; the runtime does not.

    A check/run divergence of a different kind from the information-flow ones: the program does not
    produce a forbidden value, it produces no value ever. Recorded because "check passed" carries an
    implicit expectation that the program is runnable, and a non-terminating accept is a distinct
    failure from a leaking accept.

14. **AGGREGATE PATH SEEDERS do not charge gate tags — PARTIALLY CLOSED, residual named (2026-07-28).**

    The tag resolver (item 12) closed local bind, param, join, return and identity-forwarder. It did
    NOT close the aggregate paths. These are **true accepts with runtime witnesses** — not check-only
    fail-opens, which makes this strictly worse than item 12 was:

    | shape | `check` | `run` |
    |---|---|---|
    | `let xs = [write_file]; xs[0](path, x)` | ACCEPT | rc=0, **file written** |
    | `push(fs, write_file); fs[0](path, x)` | ACCEPT | rc=0, **file written** |
    | `Box { g: write_file }; b.g(path, x)` | ACCEPT | rc=0, **file written** |
    | the same three carrying `input` instead | ACCEPT | check fail-open |

    **Discriminator:** `let g = write_file; g(...)` REJECTS. The identical call through a list
    element, a pushed element, or a struct field does not. The difference is the container, and
    nothing else.

    **Status 2026-07-28 — five shapes CLOSED, a bounded residual NAMED.** Closed in `00380b8` and
    `b3a8c2f`: index apply now charges container-seeded tags; `push`/`insert` seed on mutation,
    including of a composite expression and of a local carrying field paths; struct-field apply has
    parity; and a place assignment (`xs[0] = write_file`) no longer leaves the overwritten path
    asserting `Known(∅)` — the one member of this family that did not merely fail open but
    positively misclassified, and so was fixed first.

    Verified against a battery the adversary pre-registered BEFORE those diffs existed: chains c01
    and c04 now REJECT; **all eight over-rejection guards still ACCEPT**, including
    `uses(fs.write)` + list apply, which it named as the likeliest casualty.

    **Still open**, from an auditor parity matrix of every container operation against the value
    lane (8 differing rows, ranked, with file:line, at
    `scratchpad/fleet_20260726/auditor_round10.md`):

    | # | row | failure mode |
    |---|---|---|
    | 2 | pattern-destructuring bind, incl. enum payload | binder seeds `Unknown` — fail-open deferral |
    | 3 | container returned from a function | caller sees empty path map |
    | 4 | container passed as an argument then indexed | params start Unknown; no container-path arg summary |
    | 6 | element extractors (`get`/`first`/`last`/`pop`/`remove`) | builtin result Unknown |
    | 7 | collection transforms (`slice`/`concat`/`map`/`filter`/…) | no pass-through table |
    | 8 | map result carriers (`get`/`values`/`entries`/`merge`) | map extraction discards paths |

    Rows 6–8 may be ONE mechanism rather than three: the value lane covers all of them with a single
    conservative rule — an unsummarized builtin result carries any labelled argument — and the tag
    resolver has no equivalent. That is being tested rather than assumed.

    **c05 no longer ACCEPTS (re-measured 2026-07-29 at `4b83507b`).** This entry previously read
    "one chain, c05 (map→struct→push→field), remains ACCEPT and may fall to row 8". It now exits 1.
    Checked rather than inferred from the exit code — `anubis check` on
    `scratchpad/fleet_20260726/adversary/r12/c05_map_struct_push.anb` reports
    `ANUBIS_EFFECT_FORBIDDEN_IN_MODE: safe mode file_write (via callee \`uses(fs.write)\`)`, a real
    enforcement verdict and not a type error on a malformed fixture (the failure mode that wasted a
    round on the auditor's earlier red guard).

    **This does NOT close row 8, and the distinction matters.** The catch is in the EFFECT lane under
    safe mode; row 8 is about the TAG lane discarding paths through map extraction. A chain that one
    lane refuses tells you nothing about whether the other lane carries the label — that is the
    "both-accept = benign symmetric blind spot" reasoning run in reverse. Row 8 stays open until the
    tag lane is measured directly. Recorded now because leaving a published ACCEPT standing against a
    measured REJECT is the same fabrication as the converse.

    Every remaining row is a fail-open DEFERRAL, not a misclassification.

    **Not a new keying kind.** The adversary's exhaustion judgment survives this, refined: aggregate
    path seeders are an *implementation residual* of the SET/tag mechanism, not a sixth keying
    family. Recorded so the distinction between "the mechanism is wrong" and "the mechanism is not
    wired everywhere" stays visible.

    Found by scoring 14 predictions that were fixed in writing BEFORE the tag resolver existed:
    12 HIT, 1 MISS (this one — the list clause), 1 PARTIAL. The miss is recorded in the row where it
    was predicted rather than reinterpreted after the fact.

15. **Research-lane gate immunity is ACCIDENTAL, not designed — OPEN as a boundary (2026-07-27).**

    Asked for a judgment rather than a measurement, the offensive lane returned the uncomfortable
    answer: the dual-use lane's immunity to the builtin-carrier class holds because gated builtins
    have **no runtime lowering** in `emit_builtin_call`, not because any predicate, test or comment
    prevents them from becoming first-class values.

    Nothing would notice it breaking. Adding one gated builtin to `emit_builtin_call` — an ordinary,
    reasonable-looking change — would open the carrier, and neither the MODE gate (`--allow-research`
    is declaration-site: it selects which rules run) nor the taint, effect or capability walkers
    would catch it. It would surface only in the next security hunt.

    Recorded because an accidental property documented as a design guarantee is exactly the overclaim
    this arc exists to close. **Not enforced, not probed, not tested** — that is the honest status,
    and it should stay written that way until a predicate and a regression barrier exist.

16. **The dual-use surface — module gap CLOSED, ~77% unprobed by security examination
    (2026-07-28).**

    Three rounds characterized one narrow vertical: `emit_builtin_call` → `var_as_value` → carrier
    immunity → MODE-as-carrier → the item-15 barrier. Then four rounds (R16–R19) closed the
    module-level gap: all 8 previously-untested offensive modules now have tests.

    **SUPERSEDED 2026-07-29 — the surface grew by 10 modules at `4b83507b`.** The R20 figures below
    described a 24-module surface. Re-derived by command against the current tree:

    | quantity | R20 | now | how |
    |---|---:|---:|---|
    | offensive modules (excl. `mod.rs`) | 24 | **37** | `ls tools/anubis/src/offensive/*.rs` |
    | modules with ≥1 test | 24 | **36** | `grep -l '#\[test\]'` — the one without is `protocol.rs` |
    | `#[test]` functions | 142 | **215** | `grep -h '#\[test\]' *.rs \| wc -l` |
    | `pub fn` | 137 | **170** | `grep -h '^pub fn ' *.rs \| wc -l` |

    **The derived percentages below are STALE and are not restated here.** "~77% unprobed by security
    examination" was an estimate over 137 pub fns with a hand-judged numerator; re-deriving it needs
    the same judgment applied to 170, which is a measurement someone has to make rather than a number
    to scale. Recorded as stale rather than silently rescaled — rescaling an estimate by a ratio is
    how a guess acquires false precision. The hard counts above are current; the percentage is not.

    **REPLACED 2026-07-29 by a mechanical metric — the REFUSAL-PROBE count.** The old figure could
    not be re-derived by anyone else, because "security-relevant" was a judgment held in one agent's
    head. This one has a stated, reproducible criterion: a test is a **refusal probe** if its body
    contains `expect_err`, `unwrap_err`, `is_err()`, `assert!(!`, or an `ANUBIS_[A-Z_]+` error code —
    i.e. it asserts the system *refuses* something, rather than that a feature works. Measured
    against **HEAD, not the working tree** (`git show HEAD:<file>`), because a published count must
    describe what a clone gets — the mistake `489f5826` already corrected once.

    | quantity | at HEAD `8c31d958` |
    |---|---:|
    | offensive modules with code (excl. `mod.rs`) | 37 |
    | `pub fn` | 171 |
    | `#[test]` functions | 219 |
    | **refusal probes** | **79 (36.1% of tests)** |
    | **modules asserting NO refusal anywhere** | **10** |

    The 10: `crypto` (11 pub fns / 2 tests), `evasion` (7/6), `exploit` (2/1), `infrastructure`
    (5/5), `lolbas` (1/3), `opsec` (2/5), `payloads` (5/6), `postex` (4/5), `privesc` (6/6),
    `reporting` (4/4).

    **What this says that the old percentage did not.** For an offensive platform the safety story
    *is* the refusals — authorization gating, PLAN_ONLY defaults, isolation, scope enforcement. A
    module whose tests never assert a refusal has its guard surface untested no matter how many
    tests it has: `privesc` has 6 tests and 0 of them check that anything is denied. **Six of the
    ten are from the 10-module slice in `4b83507b`** — that slice added functionality tests and no
    guard tests, which is exactly the gap item 19a then found by hand in `malleable.rs`. This metric
    would have pointed at it in advance.

    Re-derive with the criterion above; it is a dozen lines of `git show` + regex and needs no
    judgment call.

    **Prior R20 state, retained for the derivation below:**
    - 24/24 offensive modules have at least one test (was 16/24).
    - 142 test functions across `tools/anubis/src/offensive/` (92 pre-existing + 50 authored R16–R19).
    - 137 pub fns across 24 modules.
    - 7 security probes authored R16–R19: scope enforcement (recon), false-coverage finding (purple),
      credential guard + PLAN_ONLY (phish), inject auth + plan-only default (persistence), output
      safety (rop). Pre-existing security-relevant tests exist in scope (8), isolation (4),
      run_capability (8), engagement (6), receipts (4) — not audited for probe quality.

    **Under-probed modules** (test:pub-fn ratio < 0.5): crypto (2/11), dns_codec (3/14),
    engagement (6/18), vz (7/15), isolation (4/7), exploit (1/3) — 68 pub fns, most unexercised.

    The original "91% unprobed" was derived from the module-level gap. The corrected figure by
    lens: **0% modules untested** (gap closed), **~50% pub fns unexercised** (est. 69/137 covered
    by test count ratio — imprecise without `cargo tarpaulin`), **~77% unprobed by security
    examination** (est. ~32 of 137 pub fns covered by security-relevant tests: 7 authored + ~25
    pre-existing in scope/isolation/run_capability/engagement/receipts). Line-level coverage
    unknown.

    **Predicted class — NAME-KEYED DISPATCH — partially confirmed.** The `catalog_round_trips_
    through_map_action` test (attck.rs) closes the ATT&CK technique surface. Persistence mechanism
    names and malleable profile fields remain unprobed predictions.

17. **`build` and `run` research CONSENT gap — SOURCE/VZ-CLOSED on the bounded dirty tree;
    unlanded and unshipped (2026-07-31).**

    All product callers that can lower or execute a whole program (`build`, `run`, `prove`, and
    exact/interpreted `repl`) now consult `require_program_research_boundary` after resolving and
    classifying the complete program. A Research/Exploit build without `--allow-research` rejects
    with `ANUBIS_BUILD_RESEARCH_REQUIRES_ALLOW`; supplying the capability engages the same
    disposable-VZ boundary as `run --allow-research`, before native lowering or artifact emission.
    `lower_to_native` receives an explicit mode-derived caller capability and no longer infers
    permission from `ir.has_research` or taint metadata. A redundant flag on a Safe program does not
    enable research lowering. The signed compiler execution helper independently requires an
    explicit Anubis VZ marker, and test-only raw execution helpers are no longer public APIs.

    Poison/accept guards: `research_build_requires_explicit_consent_and_vz_before_lowering`,
    `whole_program_callers_share_the_same_mode_derived_research_boundary`, and
    `research_block_local_field_access_and_ordinary_twin_both_lower`. At the deciding technical
    epoch, compiler library **771/771**, language **258/258**, security **327/327**, stdlib
    fail-closed **104/104**, PCA **19/19**, and the independent direct/carrier/dead-branch
    falsification matrix **9/9** passed. The current source-matching disposable-guest receipts are
    recorded in `docs/evidence/PHASE_1_COMPLETION_2026-07-31.md`.

    The first source-bound host seal attempt (`out/phase1_host_seal_20260730T133327Z`) is retained as
    a failed receipt, not promoted: security **327/327**, language **258/258**, stdlib fail-closed
    **104/104**; the current native-authoritative corpus is **926 files**, while that failed receipt
    graded 916 files with 0 mismatches; the measured builtin inventory was
    **213 builtins**, while check/run parity and the documentation-coverage floor were RED. Phase 1
    repairs those observed blockers and must rerun.

    The audited source-bound rerun at `out/phase1_host_seal_audited_20260730T154003Z` mechanically
    returned `SEAL_PASS` with 18/18 declared gates on pin `anubis-4dc5a51df23b`. It is **not promoted
    to a whole-tree seal**: native-authoritative enumerated **926 files** from the live disk while the
    docs gate enumerated **916 tracked files**. Five untracked `.anb` files explain the difference;
    silently narrowing either side or staging unrelated showcase work is forbidden. The discrepancy
    was a technical HOLD pending trust-surface sign-off.

    **Superseding technical closure receipt:** immutable compiler pin
    `vm/pins/anubis-51f4a964347a` (SHA-256
    `51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027`) was verified at source
    epoch `0281e8034022fc62f4f853906a33173bc0286e9ae9a0e07b26d761a495962b03`.
    Disposable guest `anubis-run-23962` completed all 22 named VM gates with zero failures,
    unchanged fixpoint, strict validator PASS, and verified deletion at
    `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z`; disposable guest
    `anubis-offensive-gate-41607` passed 34/34 with a manifest-bound strict-validator PASS and
    verified deletion at `out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z`. The
    post-metrics old/new diff covered 921 files with zero flips and zero timeouts. The finalization
    receipt at `out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md` proves the required
    source-current refreshes and exact 20/20 host seal, and its read-only review records `APPROVE`
    with no blocking finding and zero source writes. The source/VZ condition and bounded Phase-1
    activation predicate are therefore closed for the receipt's exact frozen source tree. Nothing
    here establishes landing, release, shipping, or universal soundness.

18. **The tag-lane DEFECT FACTORY — CONVERGED on pin `anubis-dacf4a164a02`; a user-fn carrier class
    is OPEN in its place (2026-07-28). The "still widening" judgment below was FALSIFIED by the
    adversary that made it — read this item top to bottom, the history is the point.**

    Asked for a judgment on whether item 14's class is converging, the adversary said **NO**, and
    named why. Three recurring shapes account for every finding since the tag resolver landed:

    | shape | what it is |
    |---|---|
    | **Unknown by DESTRUCTION** | a join or write that *produces* `Unknown` by discarding a known operand, rather than defaulting to it because nothing was known. `Unknown` charges nothing, so this deletes evidence while looking conservative. |
    | **Synthetic key invisible to the reader** | a producer records a slot under a prefix the consumer's fallback filter does not scan (`_p` vs `_w`). |
    | **Composite projection, root only** | an assignment or bind stores the RHS's root tag and never projects its nested field paths. |

    Measured instances, all **ACCEPT + file written** under a green check, all found *after* the
    first member of the class was closed: `merge_fn_alias_over` annihilating a branch arm's tag
    (`Unknown ⊔ Known = Unknown`); `_w0` unreadable by the `_p` fallback; composite place assign at
    both a literal and a symbolic index.

    **Every control-flow join site measured fail-open — 8 of 8 (2026-07-28,
    `scratchpad/fleet_20260726/adversary_round16.md`).** Asked to enumerate rather than sample, the
    adversary produced a runtime witness (green `check` **and** a file actually written) for every
    join construct in the language:

    | site | construct | witness |
    |---|---|---|
    | J-if | `Stmt::If` then/else | j01, j02 |
    | J-if-&& | `if a && b` | j09 |
    | J-if-\|\| | `if a \|\| b` | j10 |
    | J-match | statement `match` | j03, j12 |
    | J-iflet | `Stmt::IfLet` | j04 |
    | J-while | `while` body vs snap | j05 |
    | J-loop | `loop` | j07 |
    | J-for | `for` | j06 |

    The discriminator is what makes this a mechanism and not a list: `j08`, where the path exists on
    **both** arms, REJECTS; `j01`, where the key is introduced on a **subset** of arms, ACCEPTS and
    writes the file. The bug is absence-treated-as-`Unknown`, not the join itself.

    **The same round bounded its own claim.** The value-lane twin was measured, not assumed: v01–v05
    (if/match/while push of a secret, then egress) all **REJECT** — secret sticks to the container
    root through the value merge and the egress fires. The join fail-open is a **tag-lane** property.
    Do not narrow the value seeders on these grounds.

    **The clearest evidence the class is still widening (2026-07-28,
    `scratchpad/fleet_20260726/auditor_round12.md`).** After the gate-tag join was repaired in
    `merge_fn_alias_over` (missing key contributes `Known(empty)`, `mod.rs:5973-5982`), the
    **identity** join **fifteen lines above it in the same function** still converted a missing key
    into `Unknown` (`mod.rs:5955`), and `FnIdentitySet::union` annihilates on `Unknown` while
    `into_singleton` returns `None` for it — so the destroyed key charged nothing. A witness whose
    field identity exists on only the `then` arm passed `check` (rc=0) and violated at `run`. Fixed
    by `FnIdentitySet::union_present` (`mod.rs:238`), which treats **absence** as neutral while
    preserving `union`'s annihilator for a genuinely unresolved value — deliberately not the
    one-token `unwrap_or` swap, so ordinary resolver joins keep their meaning. **Unscored** pending a
    post-patch pin.

    Shape 1 survived its own fix by one lane, in the same function. That is what "factory" means
    here, and it is why this item stays open until the convergence test below actually runs.

    **Falsifiable convergence test, adopted as stated:** close the `_p`/`_w` parity, the merge
    `unwrap_or(Unknown)`, composite place-assign projection, and the c05 extract-to-let — then run a
    full residual re-sweep. Converged means **zero ACCEPT+file**, with only over-rejections and
    documented leniency remaining, and the auditor's matrix shedding rows for two consecutive rounds
    without new ranked ones appearing.

    Recorded because "we fixed three things" is not the same claim as "the class is closed", and
    this arc has repeatedly found the second to be false right after the first was true.

    ---

    **CONVERGENCE TEST RUN — the criterion above was MET (2026-07-28,
    `scratchpad/fleet_20260726/adversary_round17.md`, pin `anubis-dacf4a164a02`).**

    The adversary that judged this class "still widening" pre-registered its predictions in writing
    before measuring the rebuilt binary, then retired its own judgment:

    | criterion (as stated above) | result |
    |---|---|
    | eight join sites | **8/8 REJECT** — j01–j07, j09, j10, j12 all flipped from ACCEPT+file |
    | `_p`/`_w` synthetic parity (m22, m23) | **REJECT** |
    | composite place-assign projection (p02, p04–p09) | **REJECT**; table completed with p13, no padding |
    | c05 extract-to-let | **REJECT** |
    | zero ACCEPT+file on the named sweep | **MET** |
    | over-rejection guard G1–G8 | **all ACCEPT** — no false reject bought |
    | m01/m02 monotone over-charge | unchanged, the documented fail-closed cost |

    Scored 19 pre-registered cells, **no MISS**. "This falsifies *still widening* for that factory."

    **What replaced it — a USER-FN carrier class, OPEN (i01–i05, ACCEPT + file written).** A user
    function carrying an effect (`uses(fs.write)`) reaches an application site through `push` or a
    field place-assign and is not charged, in a `main` that declares no such capability.

    The root cause is **not** the join algebra, and the discriminator proves it: **`i02` has no
    branch at all.** `push` of a *user fn* does not seed chargeable identity evidence, while the
    list-literal twin (`[leak]`) and the builtin twin (`push(write_file)`) both do — a walker-parity
    gap on the producer side, not a merge defect. Stated consequence, adopted verbatim: *fixing only
    `unwrap_or(Unknown)` → empty will NOT close i02.*

    `FnIdentitySet::union_present` (item text above) was therefore necessary but is **not**
    sufficient. Lead measurements on the post-fix pin `anubis-cf98ccebb4c1`: security **311/311**,
    language **244/244** on `anubis-cf98ccebb4c1`, `cargo test --release -p anubis` **200 passed / 0
    failed** — so the fix
    bought no regression, and no claim that it closed the carrier class. Re-scoring of i01–i05 on
    that pin is in flight.

    **Status: the named tag factory is closed on a named pin; the callable story is NOT converged
    while i02 writes a file under a green check.** Both halves are the claim.

    ---

    **THE MUTATION/PLACE-STORE CLASS IS NOW CLOSED — i02 no longer writes its file (2026-07-28,
    pin `anubis-b24f8202f4fc`).** The witness that defined this item — a user fn carrying
    `uses(fs.write)` reaching an application site through `push`, in a `main` declaring no such
    capability — went `check rc=0 ACCEPT` + file written → **`check rc=1`, `run rc=1`, file
    ABSENT**.

    **It closed by fixing the CONSUMER, not by adding a producer.** Two earlier attempts were
    inert: `union_present` repaired the join algebra (necessary, insufficient — the adversary
    predicted that and scored 5/5), and a mutation-producer seed compiled and did nothing. The
    diagnosis: `fn_identities_of` is read by contract discharge (`mod.rs:6505`) while the
    effect/taint sink path never queried `field_fn_identities` — **the producer was writing a map
    the enforcing consumer never read.** That is this file's disease sentence verbatim, ninth
    instance. The fix re-enters resolved user-fn identities at the existing `CallExpr` apply site
    through `analyze_expr_effect`; it adds **no 27th walker** and does **not** merge label and tag
    monoids.

    **Graded independently by the adversary that found the class**, not by its author — 13 rows
    flipped ACCEPT+file → REJECT: `p2_free_push`, `p2_free_insert`, `i01`–`i05`, `c02` (list
    insert), `c05` (map place-assign), `c18` (struct-field place-assign), `e03` (map symbolic-key
    assign), `e04` (loop push), `e14` (list index assign), `e15` (return-empty-then-push), `e17`.
    Verdict: *"CLOSE IS REAL."* Corpus measured at this close: security **311/311**, language **244/244**, stdlib
    **104/104**, zero accept→reject flips.

    **Still OPEN, and this close does not touch them:**

    | residual | state |
    |---|---|
    | container as **PARAM** | **OPEN** — confirmed across list, map AND struct: all three ACCEPT+file as params while all three REJECT as returns. A class, not a row. |
    | **extraction** (`pop`/`remove`) | OPEN — loses identity from an already-correct literal |
    | **inline forwarder composition** (`[identity(leak)]`) | OPEN |
    | `gu06`/`gu06b`/`gu06c` concrete-refresh | **over-rejection UNFIXED** — assigning a pure callable over a gated one at a concrete index still REJECTs; the adjudicated concrete-REPLACE rule did not fire |

    **A disagreement is on the record rather than reconciled:** the auditor reports its own six
    round-14 witnesses did NOT flip on the same binary where the adversary's equivalent shapes did.
    One fixture set is malformed — the auditor's previous "red guard" applied a container ROOT and
    could only ever type-error. Being resolved; recorded here because burying an instrument
    disagreement cost this fleet a full round once already.

19. **The purple report makes FALSE ATT&CK coverage claims — THREE OF FOUR CLOSED, residual named
    (2026-07-28).**

    **Status by instance, re-measured 2026-07-28.** The headline defect and two of the three
    look-alikes are closed in tree; one look-alike is closed by this entry's own change; the
    `malleable.rs` finding is a different disease and stays open under its own name.

    | instance | state | evidence |
    |---|---|---|
    | `map_action` false pos / false neg | **CLOSED** | `attck.rs` `word_match()` replaces bare `contains` for `ls`/`pwd`/`cat`; the dead loop is gone; `catalog_round_trips_through_map_action` asserts every non-`not_claimed` technique is reachable from its own `aop_surface`, and named regression tests pin the three witnesses below |
    | `listener.rs` unvalidated module names | **CLOSED** | `is_valid_agent_module()` filters `modules::catalog()` to `side == "agent"`; three tests, incl. over-rejection and operator-only rejection |
    | `modules.rs` catalog ↔ `agent.rs:run_module` | **CLOSED** (this entry) | three parity tests in `agent.rs`, both directions plus a stale-exemption guard — see below |
    | evidence `kind` underscores vs `aop_surface` hyphens | **CLOSED** | `map_action` keys on delimiter-free substrings (`engage`, `recon`, `exploit`, …), so both spellings reach the same IDs; `map_action_engage_init_maps_to_t1583` pins `engage-init` **and** `engage_init`. Verified across the catalog's underscore names: `recon_scan`, `exploit_run`, `persist_launchagent`, `lateral_ssh`, `lateral_smb`, `string_scramble`, `xor_pack`, `malleable_profile`, `phish_plan`, `lolbas_catalog`, `inject_plan`, `vz_exploit`, `vz_fuzz` all map |
    | `malleable.rs` `transform` validated-but-never-read | **SUPERSEDED by 19a below** — closed as "never read" in `fded3c85` (the listener now applies it), and closing it that way opened a worse defect | see 19a |

    **The catalog↔dispatch instance was worse than first recorded, and the record was wrong about
    why.** It was filed as "independent enumerations, agree today, no validation either way". The
    first half is right; the second understates it. `run_module` does not live in this crate — it
    sits inside the `r###"…"###` beacon-source template in `agent.rs`, so it is *text* until the
    generated agent is compiled. No amount of type-checking could ever have bound it to the catalog.
    And the listener's fix made the asymmetry sharper rather than safer: the listener validates a
    task name against the catalog, so a published-but-undispatchable module is ACCEPTED by the
    operator-side check and then answered `unknown module` by the beacon. The gap is a false
    *capability* claim rather than a false *coverage* claim, and it points the same way — the
    operator is told something works that does not.

    Closed by three tests that read the same rendered template that gets compiled into the beacon,
    so they cannot drift from what ships: every `side == "agent"` catalog entry has a dispatch arm;
    every dispatch arm is either published or carries a written reason in
    `UNPUBLISHED_DISPATCH_ALIASES` (one entry today — `exit`, an alias for `die`); and the exemption
    list itself is guarded against going stale. Executed 2026-07-28 on commit `9155bab3` via
    `cargo test -p anubis --bin anubis offensive::agent::tests -- --nocapture` — **7 passed, 0
    failed**, plus three companion poison tests carrying the RED evidence on the parity predicates
    and the arm extractor.

    ---

    **The original finding, retained.** `attck.rs` holds a catalog of 20 techniques, each carrying an `aop_surface` string
    (the PRODUCER), and `map_action()` maps an observed action to technique IDs (the CONSUMER). They
    enumerate independently. Between them sits a **dead loop** that computes the catalog lookup and
    discards it (`let _ = tech.id;`), with a comment recording that the dynamic lookup was abandoned
    for "a concrete mapping table for reliability" — a hand-coded `contains()` keyword chain that
    does not cover the catalog.

    | direction | witness | effect |
    |---|---|---|
    | **false positive** | `map_action("attck_catalog")` → `["T1083"]`, because `"attck_catalog".contains("cat")` | the purple report claims *File and Directory Discovery was exercised* when the operator only ran the ATT&CK **documentation** module |
    | **false positive** | `"listen HTTP/DoH/mTLS"` matches `"ls"` inside `"mtls"` | a C2 listener is reported as file discovery |
    | **false negative** | `map_action("engage-init")` → `[]` | T1583 shows as a detection GAP even when the engagement performed it |

    `purple.rs:57` builds coverage from these mappings, so both directions corrupt the
    operator-facing debrief. A false coverage claim in a purple-team report is worse than a missing
    one: it asserts an adversary behaviour was tested when it was not.

    **Three more instances of the same shape**, none currently breaking: `modules.rs` catalog vs
    `agent.rs:run_module` (independent enumerations, agree today, no validation either way);
    `listener.rs` queues any string as a module name unvalidated; and evidence `kind` strings use
    underscores (`engage_init`) while `aop_surface` uses hyphens (`engage-init`) — so even reviving
    the dead loop would not match.

    **Fix shape: a PARITY TEST, not a shared enum.** Too many independent string sources (evidence
    files, CLI commands, ATT&CK IDs, agent module names) to unify; a round-trip test asserting every
    catalog technique is reachable through `map_action` fails today for T1583 and T1059 and would
    catch future catalog additions. Substring collisions need word-boundary matching rather than
    bare `contains`.

    *The prescription above was followed exactly, and it held for all three closed instances —
    including the one it was not written for. Where the producer and consumer are two enumerations
    of strings, the parity test is the binding; the shared enum was never available, because one of
    the four consumers is a string template and another is a JSON file on disk.*

    Found by an agent scoring its own pre-registered prediction **1 of 3** and reporting both
    refutations plainly: `persistence.rs` has no dispatch surface at all, and `malleable.rs` is a
    typed struct — though its `transform` field turns out to be validated-but-never-read, with the
    listener hard-coding its headers independently. A different disease, recorded here so it is not
    lost.

19a. **The malleable `transform` source repair landed in `889d9a7c`; post-repair guest
    compatibility remains unsealed (`cli: harden offensive isolation and evidence identity`,
    2026-07-29).**

Landing-status: LANDED commit=`889d9a7c`

    Closing 19's `transform` residual by wiring `apply_transform` into the listener
    (`fded3c85`, pushed) removed a dead field and introduced two defects the dead field did not
    have. **Inert is harmless; one-directional is a silent break.** Commit `889d9a7c` contains the
    repair. A guest receipt can verify that repair only when its binary was built from a tree
    containing `889d9a7c`; landing the source did not upgrade older evidence.

    | # | defect | witness |
    |---|---|---|
    | 1 | **No inverse exists anywhere.** `listener.rs` transforms every beacon response (`encode_response`); nothing reverses it. `grep -rn 'unapply\|untransform\|decode_transform' tools/anubis/src` returns nothing. The beacon's own base64 (`agent.rs` B64 encode/decode) is the CRYPTO ENVELOPE, a different layer. | a profile with `transform: "base64"` or `"prepend_junk"` makes the listener emit a body the beacon cannot parse — **silent C2 failure**, surfacing only once an operator writes a non-default profile |
    | 2 | **`validate()` never constrained `transform`**, and `apply_transform`'s `_ =>` arm returns identity. | `"base64 "` (trailing space) or `"xor"` are accepted and silently shape nothing while the operator believes the profile is live — the fail-open-on-declaration pattern closed for unknown attributes in `ec65724` |
    | 3 | **`load_from_engage` ended in `.ok()`**, collapsing a REJECTED profile to `None`. | the listener came up unprofiled and reported nothing wrong. Consequence for any fix: **adding validation alone is inert**, because the only consumer swallows the error |

    Defect 3 is the one worth carrying forward as a rule. It is the same producer/consumer split
    as the rest of item 19, one layer up: enforcement was added at the producer while the single
    consumer discarded the result. **When adding a check, verify the consumer does not swallow
    it** — otherwise the check is decorative and the board still reads green.

    **Fix shape (implemented in `889d9a7c`; bounded seal recorded below):** the same parity shape
    used for the ATT&CK catalog — `KNOWN_TRANSFORMS` + `TRANSFORMS_WITH_BEACON_INVERSE` (only
    `none` today), `validate()` rejecting the two classes with distinct errors
    (`ANUBIS_MALLEABLE_TRANSFORM_UNKNOWN` / `_NO_INVERSE`), `load_from_engage` returning
    `Result<Option<_>>`, and a parity test asserting every KNOWN transform has a non-identity arm.

    **Verification boundary, stated exactly.** The negative-control matrix (5 transform values,
    each producing its correct distinct error) and the loader regression establish that invalid
    profiles are rejected and the listener consumer does not collapse the error to `None`.

Historical receipt `vm/pins/anubis-242902cfefc0` records head `0f407853`; it predates `889d9a7c`
and cannot verify later repairs.
[receipt-scope: pre-fix-only; head: 0f407853; authority: none]

    That candidate's SHA-256 is
    `242902cfefc02838eb530925d633b784beeb994ace6b7039ad915c1afd8e31ff`, recorded at
    `2026-07-29T13:51:27Z`. Its VM and **34/34** offensive receipts do not verify the transform
    repair or post-repair guest compatibility. A post-fix guest run was observed on 2026-07-30, but
    its exported report bundle was not internally hash-consistent; that RED receipt remains below
    as history. A fresh 2026-07-31 strict-validator receipt now supersedes it for the bounded dirty
    technical epoch. The direct loader tests remain the bounded source oracle.

20. **The 48-command offensive expansion bypassed the VZ isolation guard — source fix landed in
    `889d9a7c`; POST-FIX GUEST VERIFICATION CLOSED for the bounded dirty technical epoch.**

Landing-status: LANDED commit=`889d9a7c`

CLAIMS-20-receipt-status: CLOSED
CLAIMS-20-receipt-identity: MATCH

Historical receipt `vm/pins/anubis-242902cfefc0` records head `0f407853`; it predates `889d9a7c`
and cannot verify later repairs.
[receipt-scope: pre-fix-only; head: 0f407853; authority: none]

    **Historical pre-fix warning; current policy remains guest-only.** Before this fix, no
    `credential-*`, `privesc-*`, `discovery-*`, `collection-*`, `evasion-*`, `exfil-*`, or
    `postex-*` command could be run on the host safely: they executed there and sealed the results
    into the engagement. The fix makes those paths refuse; it does not authorize host execution.

    `isolation::require_vz_offensive(action)` is the guard behind this repo's most load-bearing
    operational claim — *"Anything research-gated or crash-capable runs inside a disposable tart
    guest… Calling a host run 'isolated' is FABRICATION."* Measured at `26f800a6`:

    | | count |
    |---|---:|
    | call sites of `require_vz_offensive` in `main.rs` | **11**, all pre-existing (`listen`, `agent-generate`, `task-queue`, `exploit-run`, `persist-launchagent`, `inject-plan`, `lateral-ssh`, `lateral-smb`, `pack-xor`, `string-scramble`, `recon-scan`) |
    | handler call sites for the 10 modules added in `4b83507b` | **48** |
    | of those, VZ-gated | **0** |

    **Demonstrated, not inferred.** On the host, with a valid engagement:

    ```
    $ anubis discovery-system-enum --engage <eng>
    { "executed": true, "hostname": "<real host>", "id": "uid=501(...) gid=20(staff) groups=...",
      "df": "<full host disk layout>", "attck": ["T1082","T1033"], ... }
    ```

    No refusal. Compare `listen`, which refuses with `ANUBIS_OFFENSIVE_HOST_FORBIDDEN` before it
    even loads the engagement. Thirteen `Command::new` sites across `privesc`, `discovery`,
    `collection`, `evasion`, `postex` run on the operator's own machine — `sudo -l -n`, `crontab -l`,
    `ps aux`, `sqlite3` against the TCC privacy database, `osascript` for login items, clipboard
    capture, `find` for "sensitive file discovery", SSH-key and environment credential scans.

    **This is a broken promise, not a hidden backdoor** — each command is explicitly invoked with
    an explicit `--engage`. The defect is that the platform states it will refuse and does not, and
    that `run_offensive_platform_gate.sh` reports `isolation: tart-disposable-guest` while this
    surface is unguarded.

    **Gate by DEFAULT, not by enumeration.** A `Command::new` grep finds 17 host-touching
    functions and **misses `discovery::system_enum`** — the one proven above to execute — because
    it shells out through a `run_cmd` helper. An enumerated allow-list built from that grep would
    have shipped the demonstrated hole. Every new-module handler should take the guard unless it is
    provably pure (`payloads` encoders, `reporting` formatters, the `*-plan` emitters), and the
    exemption should be a written list checked by a test, the same shape as item 19's parity fix.

    **The refusal-probe metric in item 16 predicted this.** Six of the ten modules that assert no
    refusal anywhere are exactly these. A module that never tests a denial is a module whose guard
    surface nobody has looked at; here nobody had, because there was no guard.

    **Fix — implemented in `889d9a7c`; current post-fix receipt is manifest-bound and strict-validator
    green (2026-07-31).** The affected
    handlers call `require_vz_offensive` before `load_engagement`, using the action label the handler
    already seals into the receipt chain, so the guard and the receipt name the same thing. At that
    commit, `main.rs` contains **60 literal guard call sites**. A raw text count is 61 because the
    parity test itself contains the string `require_vz_offensive(\"{action}\")`; that line is an
    assertion about calls, not a call. Frozen pin `vm/pins/anubis-4ca5b6f21917` independently emits
    **60** entries in `offensive-doctor --json` at `isolation.host_forbidden_aop`. Three parity tests
    in `main.rs` bind the policy going forward:

    - `every_offensive_handler_requires_a_vz_guest` — reads its own source via `include_str!`, so it
      cannot drift from what ships. **Proven RED before green**: deleting the single
      `discovery_system_enum` guard fails it with `CLAIMS-20: 1 offensive handler(s) … ["DiscoverySystemEnum"]`.
    - `exemptions_name_real_handlers_and_carry_a_reason` — the exemption list is **empty**; the AOP
      surface is guest-only as a class, and any future exemption must be named with a reason.
    - `legacy_offensive_commands_remain_gated` — pins the 11 pre-existing guards so a refactor
      cannot quietly drop the older half.

    **The instrument caught itself, which is why it is trustworthy.** The first extractor matched
    only lines ending `=> {` and saw **86 of 128** arms — the 42 single-expression arms
    (`=> run_x(a),`) would have been skipped silently, so a future offensive command written in that
    form would have passed unchecked. A hardcoded `arms.len() > 100` sanity assert caught it; the
    check is now self-calibrating (parsed arms must equal `Commands::` header count).

    Demonstrated before/after on the real binary — the exact command from the witness above:

    ```
    before: { "executed": true, "hostname": "<real>", "id": "uid=501(...)", "df": "<host disks>" }
    after:  Error: ANUBIS_OFFENSIVE_HOST_FORBIDDEN: `discovery_system_enum` … never on the host.
    ```

    Also verified refusing: `privesc-sudo-audit`, `collection-clipboard`, `evasion-security-enum`,
    `postex-persistence-enum`, `credential-env-scan`, `infra-c2-check`, `infra-redirector-plan`,
    `payload-cyclic`, `report-attck-coverage`. Host suite 345/345, `main.rs` clippy-clean.

    **Historical 2026-07-30 post-fix observation — RED receipt identity mismatch; retained, not
    promoted.** Immutable pin
    `vm/pins/anubis-a6f7f05fd132`, SHA-256
    `a6f7f05fd132ed7ad9891b2884acf15e80625ba3f7f967939cbf808804320793`, is source-matched to tree
    hash `658f3ebaa4274b168f61519beac9dfcd3560d07a3aa653e68cc287521df400ca` and contains `889d9a7c`.
    Disposable guest `anubis-offensive-gate-82951` ran the complete offensive battery **34/34**;
    `doctor_t17` and `t9_doctor_surfaces` passed, the transported binary hash matched the pin, the
    export manifest reported `secret_scan: PASS`, and teardown was recorded separately as
    `torn_down`. However, `export_manifest.json` binds the 475-byte guest report embedded in
    `guest_stdout.log` (SHA-256
    `85dde2273bb08c02334352768582566f09e68d5d091755e229b3e4b4b89c8504`), while the checked-out
    host-augmented `report.json` is 520 bytes (SHA-256
    `17592b9c7bd8330c435179344189218898f9e02e79a6ae625c48cc1a50ae9997`). Those are different
    objects. Artifacts:
    `out/phase1_offensive_complete_v4_20260730T191631Z/{guest_stdout.log,report.json,isolation.json,export_manifest.json,teardown_status.txt}`.
    Therefore the bounded 34/34 guest observation and separate teardown record stand, but that
    bundle is not the closure receipt.

    **Superseding 2026-07-31 strict receipt — CLOSED / MATCH for the bounded technical epoch.**
    Immutable compiler pin `vm/pins/anubis-51f4a964347a`, SHA-256
    `51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027`, was source-verified at
    technical epoch `0281e8034022fc62f4f853906a33173bc0286e9ae9a0e07b26d761a495962b03`.
    Disposable guest `anubis-offensive-gate-41607` completed **34/34** checks, including both doctor
    cases, under `isolation=tart-disposable-guest`; the transported binary hash matched the pin and
    teardown recorded `torn_down`. The strict validator accepted exactly the allow-listed files in
    `out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z`: `report.json` is 4,157 bytes at
    SHA-256 `1f93c22c8b9cd37124b50680e3b1bad70dade178b362060736019616023b18ee`,
    `export_manifest.json` is SHA-256
    `d1ef0a556f512af59eeb801ff817a20ae8596fabe07969fe534e3ecc33c00b71`, and
    `offensive_verdict.json` is PASS with no errors at SHA-256
    `05f2a561cad7c6e72c9738bccc751e319fb0e1377c6065c77014256a00cbfa99`. Independent revalidation
    `/tmp/anubis-phase1-offensive-51f4-postmetrics-revalidation-20260731T185200Z.json` produced the
    identical verdict object and hash. This supersedes the RED mismatch above; it does not erase
    it. The external finalization receipt at
    `out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md` proves the exact
    VM/offensive/921-row-diff/host-seal predicate, and its independent review records `APPROVE` with
    no blocking finding and zero source writes. Bounded acceptance is therefore activated for the
    frozen source tree named above. The live tree has moved beyond that epoch and remains a distinct
    landing/release question; no host execution, release, shipping, or universal-soundness claim is
    made.

21. **SIX items published as CLOSED are OPEN — 30 true accepts with runtime witnesses, survived
    adversarial refutation (2026-07-29). This is the largest open item on the list.**

    **Evidence consequence (2026-07-30):** PCA schema v2 deliberately removed the independent
    `taint_clean` boolean. The v1 producer set it to `true` merely because the bounded typechecker
    returned `Ok`, which contradicted the live witnesses below. PCA v2 records that bounded
    typecheck result without upgrading it into a total-flow theorem. Its parser denies unknown claim
    fields, so a rehashed v2 object cannot smuggle the retired field back in; missing `pca.json` and
    retired v1 claims also fail semantic verification rather than downgrading to hash-only success.

    **Method.** Eleven agents, each told to FALSIFY rather than confirm: six assigned one
    published-CLOSED item, five hunting the named-open residuals. ~192 probes, all written outside
    the repo, all run against `./target/release/anubis`. **Every finding was then handed to an
    independent verifier instructed to REFUTE it and to default to refuted when uncertain.**
    43 raw findings → **31 survived refutation, 30 of them TRUE ACCEPTS** (12 were refuted and are
    discarded). The verifiers judged most as *not* covered by any published item; one is recorded as
    outright falsifying the item it was tested against.

    **All six falsification surfaces came back BROKEN. Not one closure held.**

    | item | published as | actual |
    |---|---|---|
    | B4 | Function-identity carrier CLOSED `0eb5977` | **BROKEN** — 9 TA |
    | 10 | `requires` through fn-value carrier CLOSED | **BROKEN** — 6 TA |
    | 11 | Nested-call argument `requires` CLOSED | **BROKEN** — 1 TA + 2 deferrals |
    | 12 | Bare-builtin carrier / trifecta CLOSED `c7643e5` | **BROKEN** — 5 TA |
    | 1 | Composition residuals D1–D6 closed | **BROKEN** — 15 TA |
    | 2 | check/run divergence (R) CLOSED | **BROKEN** — 1 TA |

    **The single mechanism behind most of it: PLACE-ASSIGNMENT.** Boundary item B4 closed the *read*
    carriers
    and left every *write* carrier open. All of these `check` green and `run` prints the secret:

    ```anubis
    struct Box { f: u64 }
    fn key() -> secret<i64> { return 42; }
    fn main() { let b = Box { f: plain }; b.f = key; let g = b.f; print(g()); }
    ```

    …and the same for `fs[0] = key`, `m["k"] = key`, `o.i.f = key`, whole-struct pass after the
    write, the integrity dual (tainted fn → `shell`), and secret → `net.send`. It is **not**
    interprocedural-only. This is the write-lane re-exposing latent unsoundness in a lane that was
    audited only for reads — a pattern this repo has already hit in the field-write trilogy.

    **The most alarming single witness:** wrapping item 10's *own corpus fixture* in `if 1 == 1 { }`
    flips its verdict REJECT → ACCEPT with a runtime witness. The fixture that certifies the closure
    is defeated by a trivially-true conditional.

    **Item 12 was closed in the SOURCE direction only.** The sink direction still launders: a bare
    `write_file` / `shell` passed as a callable parameter writes attacker-controlled content on a
    green check, and *all four* duals of the pinned REJECT fixtures ACCEPT.

    **Item 1's D1–D6 closure does not survive contact with containers.** 15 true accepts: for-loop
    binder over records, `xs[0].k`, `m["a"].k`, untyped parameter, declared `list<S>` parameter,
    factory with no declared return type, lambda-bound factory, `mk()[0].k`, closure stored in a
    struct field then applied, index at the root of a nested path, push-then-read-back,
    `while` with a variable index — plus taint-lane duals reaching `fs.write` and `net.send`.

    **What this meant for the completeness question.** The engineering board was green — language
    252/252, security 317/317 (**historical stamp, 2026-07-29**), stdlib 104/104,
    native-authoritative 906/0, formal PASS, VM seal
    22/22 fixpoint unchanged. **None of that is the language promise.** Against the unscheduled
    universal research aspiration, the answer is **NO**, and it is not close: 30 programs pass
    `check` and violate at runtime. `docs/language/ROADMAP.md` already says freestanding
    "Phase N DONE" is false as a soundness claim; this is the measurement behind that sentence.

    **The lesson is about the CLOSURES, not the defects.** Six independent items, each closed by a
    real fix with a real fixture, each still open — because the fix was verified against the
    positions its fixture happened to occupy. A corpus stops offering counterexamples long before a
    class is closed, and **a green board is exactly when that is least visible.** Closing these
    one carrier at a time will reproduce the same outcome; the fix has to be positional totality
    (the `carrier.rs` / `loopctl.rs` shape from Phase 2), applied to the write lane and to
    call-site discharge.

    **ROOT CAUSES, located by the verifiers in source.** The refuters did not stop at reproducing —
    each read the compiler and named the line. This turns the list above into a fix map, and it is
    a much smaller set of mechanisms than the 30 witnesses suggest:

    | # | site | mechanism | explains |
    |---|---|---|---|
    | 1 | `middle/mod.rs:17910-17935` `collect_unconditional_param_contract_stmts` | descends only into `If`/`While` **cond**, `For` **source**, and nothing for `Loop` — **bodies are never walked**, so `fn_param_contract_apps` (:3684) stays empty and `discharge_carried_call_requires` (:7630) folds an empty vector to `true` | item 10's `if 1==1` / for / while / loop / match-arm / if-let class |
    | 2 | `middle/mod.rs:17964` | `Expr::Call` rebuilds the callee as `Expr::Var(name)` and records an application only if that NAME is a formal (`:17859-17869`) | the local-alias defeat |
    | 3 | `middle/mod.rs:762-777` | `Expr::CallExpr` with a `FieldAccess` callee → `FnIdentitySet::Unknown` unless the field is in `method_returns_param`/`method_sole_return` | struct-field-stored fn applied as `b.h(-1)` |
    | 4 | `middle/mod.rs:10100` vs `~10141` (`Stmt::Assign`) | the `Expr::Var` target branch refreshes identity via `fn_identities_of`; the **non-`Var` place-assign branch sets only `tainted`/`taint_source`** and never refreshes identity | the whole place-assignment class (boundary item B4) |
    | 5 | `middle/mod.rs:22765` (`walk_block_secret`) | non-`Var` `Stmt::Assign` only sets `b.secret`, never touches `field_closures`/`field_fn_identities` — **the stale initializer wins** | `b.f = key` still reading as `plain` |
    | 6 | `middle/mod.rs:10311-10336` | **does** retain-and-insert the assigned identity into `field_fn_identities` — **and the consumer on that path never reads it** | — |
    | 7 | `middle/mod.rs:22460` | the non-`Var` `Stmt::Assign` arm calls `expr_taint_source_m`; `container_element_taint` is called **only** from container-LITERAL arms (`:23295-23338`), never from the place handler | taint duals of the write class |
    | 8 | `middle/mod.rs:8514-8552` `place_struct_type` | matches `Var`, `FieldAccess`, `Call`, `CallExpr`, then **`_ => None` — there is no `Expr::Index` arm**; `declared_field_type` (:8468) short-circuits on that `None` | `xs[0].k`, `m["a"].k`, `mk()[0].k`, index-at-root |
    | 9 | `middle/mod.rs:8520` | `Expr::Var(v) => scope.get(v)…ty` is `None` for an **unannotated formal** | `fn leak(s) { print(s.k) }` |
    | 10 | `middle/mod.rs:3706` | an unannotated fn gets the **EMPTY STRING** return type; the `Call` arm filters `!t.is_empty()` → `None` | factory with no declared return type |

    **Row 6 is this project's disease stated verbatim, in its own compiler.** A producer computes
    `field_fn_identities` on the place-assign path and the consumer on that same path ignores it.
    Not a missing analysis — a written label that nothing reads.

    **Row 6, refined by source reading 2026-07-29 — it is worse than "the consumer ignores it".**
    The producer at `:10311-10336` writes `field_fn_identities` into the **effect** walker's scope.
    The **confidentiality** walker `walk_block_secret` (`mod.rs:22697-23100`) builds its own
    `ScopeBinding`s and, across those 400 lines, mentions `field_closures` and
    `field_fn_identities` **exactly once each — both as `BTreeMap::new()` initializers**
    (`:22877`, `:22880`). It never populates them and never reads them.

    ```
    awk 'NR>=22697 && NR<=23100' compiler/src/middle/mod.rs | grep -n 'field_fn_identities'
    #  -> 22880:  field_fn_identities: BTreeMap::new(),      (the only occurrence)
    ```

    So the secret lane does not hold a **stale** function identity — **it holds none at all**, and
    every `b.f = key` / `let g = b.f; g()` resolution in the confidentiality direction is decided
    against an empty map. That is why the entire §3.1 class leaks toward `print`/`net.send` while
    the capability lane (which does carry identity) rejects the same shapes — the discriminator the
    verifiers observed and could not explain from the write site alone.

    **Consequence for the fix:** this is not "wire the consumer to the producer." The
    confidentiality walker needs the identity lane *at all* — either by sharing the effect walker's
    scope or by running the same `fn_identities_of` / `collect_container_fn_identities` maintenance
    on `Stmt::Let` and both `Stmt::Assign` arms. Both `Assign` arms in that walker currently update
    only `b.secret` (`:22748-22783`). Estimating this as a small patch would be wrong.

    **Row 1, refined by source reading 2026-07-29 — the exclusion is DELIBERATE, and a naive fix
    over-rejects.** `collect_unconditional_param_contract_stmts` (`:17883-17956`) does not *fail* to
    walk bodies; it **names them as ignore-patterns**: `Stmt::If { then: _, else_: _ }`,
    `Stmt::While { body: _ }`, `Stmt::For { body: _ }`, `Stmt::WhileLet { body: _ }`, and
    `Stmt::Loop { body: _ } => {}`. Its expression twin does the same:
    `Expr::IfLet { then: _, else_: _ }` walks only the scrutinee and `Expr::Lambda { body: _ }`
    ignores the body — while plain `Expr::Block` **is** recursed (`:18074`). The rule is exact:
    *plain blocks are walked; anything guarded or deferred is excluded.* The function name states
    the premise — it collects only **unconditionally executed** applications.

    **That premise is the defect.** For a *precondition obligation*, a call that MIGHT execute
    carries exactly the same obligation as one that does; collecting only unconditional
    applications is fail-open by construction. The DIRECT lane already gets this right — it calls
    `discharge_calls_in_expr(ctx, assumptions, scope, expr)` per statement with the branch guard in
    scope, which is precisely why the verifiers measured the direct twin **rejecting** inside
    `if 1 == 1 { }` and **accepting** inside a dead `if 1 == 2 { }`. The carrier lane bypasses that
    machinery with this weaker pre-collection.

    **So the fix is not "add the missing arms."** Walking the bodies without carrying the path
    condition would reject the dead-branch case that the direct lane correctly accepts — trading a
    false accept for a false reject. The correct change is to make the carrier lane **reuse the
    direct lane's path-condition-aware discharge** rather than pre-collect with a separate
    "unconditional" walker. Recorded before anyone starts, because the mechanical reading of row 1
    leads straight to an over-rejection regression.

    **Row 8 is the Phase-2 discipline not having reached this walker.** `carrier.rs` and
    `loopctl.rs` were made total so a new variant fails to compile; `place_struct_type` still ends
    in `_ => None`, and `Expr::Index` fell straight through it. The fix shape already exists in the
    repo — it was simply never applied here.

    **Row 8 — FIXED for annotated containers, clean-VM sealed, and landed in `03210603`
    (2026-07-29).**
    `place_struct_type` (`mod.rs:8514`) gained an `Expr::Index` arm resolving the base's element
    type via a new `ty::container_element_type` (`list<T>` → `T`, `map<K,V>` → `V`, generic-depth
    aware so `map<string, list<S>>` yields `list<S>`). It returns `None` for any unrecognised
    spelling, so **the arm can only ADD a declared qualifier that was previously dropped, never
    remove one.**

    Before → after on the same program, same binary path:

    ```anubis
    struct S { k: secret<i64> }
    fn main() { let xs: list<S> = [S { k: 7 }]; print(xs[0].k); }
    // before: check rc=0, run printed 7
    // after : ANUBIS_SECRET_EXFILTRATION: secret `declared field `k` of type `secret<i64>``
    //         flows to egress `print` without declassify()
    ```

    | verification | result |
    |---|---|
    | compiler lib | **766/766** — source-current W1 suite, including recursive malformed-slot tests |
    | tool unit suite | **351/351** plus all integration harnesses green |
    | security corpus | **327/327** — includes the ten annotated list/map/generic/parameter fixtures |
    | language corpus | **258/258** |
    | stdlib fail-closed | **104/104**, `timed_out=0` |
    | native-authoritative | current corpus **926 files**; this W1 receipt graded 916 files, 0 mismatches, 0 disagreements |
    | formal | **162 theorems / 15 modules**, machine-checked; no `sorry`/`admit`/free `axiom` |
    | immutable candidate | `vm/pins/anubis-281e0e846948`, SHA-256 `281e0e84…5262`; source-tree verification PASS |

    Fixtures added, including the over-rejection guard the project's rules require:
    `secret_field_via_annotated_list_index_rejects.anb`,
    `secret_field_via_annotated_map_index_rejects.anb`, and
    `public_field_via_annotated_list_index_accepts.anb` (an unqualified field read through the same
    index carrier must still ACCEPT — it does).

    **Bounded honestly: the UNANNOTATED form is still OPEN.** `let xs = [S { k: 7 }]` infers the
    bare type `list` with no element parameter (`mod.rs:21310`
    `Expr::ArrayLiteral => Some("list")`), so `container_element_type("list")` is `None` and the
    qualifier is still dropped. Four of §3.3's fifteen witnesses use the annotated or map form and
    are closed; the unannotated ones are not. **Closing them requires element-type inference for
    array literals, which is a different change and is not claimed here.** A unit test pins
    `container_element_type("list") == None` precisely so this boundary cannot be mistaken for
    closure.

Historical receipt `vm/pins/anubis-242902cfefc0` records head `0f407853`; it predates `889d9a7c`
and cannot verify later repairs.
[receipt-scope: pre-fix-only; head: 0f407853; authority: none]

    **Seal provenance correction.** Disposable guest `anubis-run-79352` ran that historical
    candidate, whose head precedes the annotated container fix in `03210603`. It cannot verify Row
    8.

    The bounded source-matched W1 receipt below,
    pin `vm/pins/anubis-58ba4abc0a63`, is the evidence for the annotated subset; the unannotated
    residual above remains OPEN.

    **These are TRUE ACCEPTS, not deferrals, and the verifiers proved it rather than asserting it.**
    Discriminators run: the DIRECT call inside the same `if 1 == 1 { }` REJECTS (rc=1), and the
    direct call inside a DEAD `if 1 == 2 { }` ACCEPTS — so the direct lane is path-condition aware,
    while the carrier lane accepts **both** live and dead branches. That is blindness, not
    deliberate under-approximation. The capability-lane twin of the identical shape REJECTS, so the
    hole is specific to the contract lane. Instrument checked too: the binary post-dates the last
    commit touching `mod.rs`, and `compiler/` was clean.

    Raw probes and verifier transcripts:
    `subagents/workflows/wf_849c0fb2-478/` (journal.jsonl + per-agent jsonl). 73 agents, 0 errors,
    ~58 min. Findings are recorded here as MEASURED and root-caused; **no fix is claimed and none
    has been attempted** — every site above is in `compiler/src/middle/**`.

    **Bounded W1 receipt (2026-07-29, commit `03210603`).** This closes only the annotated
    container-place / typed-parameter subset; item 21 remains open for the named unannotated and
    function-value classes. The source-matched pin is `vm/pins/anubis-58ba4abc0a63`, SHA-256
    `58ba4abc0a636d909aa72e4f8df06d6e2adcad3ae378396a4c62a63f106a25bf`.

    | Gate | Exact observation |
    |---|---|
    | compiler library | **766/766 PASS** |
    | CLI/tool package after `889d9a7c` | **357/357 PASS** plus every integration-test binary |
    | security | **327/327 PASS** |
    | language | **258/258 PASS** |
    | stdlib fail-closed | **104/104 PASS** |
    | native-authoritative | current corpus **926 files**; this W1 receipt graded 916 files, 0 mismatches |
    | formal inventory | **162 theorems / 15 modules**, gate PASS |
    | builtin inventory | **213 builtins**; inventory only, not whole-surface runtime proof |

    *Instrument note against myself:* the workflow's own post-processing returned empty
    `raw_falsify`/`falsify_summary` arrays because I wrote a self-contradictory filter
    (`r.verdict === undefined && r.surface`). The `surviving_findings` array and the journal were
    unaffected and carry the full record, but the summary I first read was produced by a broken
    reducer — recorded because a silently-empty aggregate is exactly the failure this file tracks.

### HISTORICAL, SUPERSEDED BY ITEM 21 — carrier class judged EXHAUSTED as a callee-identity class (2026-07-27)

The following dated judgment was falsified on 2026-07-29 by item 21's place-assignment,
control-flow, container, and call-site witnesses. It is retained only as historical evidence of the
claim that failed. After 19 surfaces audited, two rounds of pre-registered predictions scored, and four of five leaking
surfaces closed, the adversary's judgment — requested explicitly as a judgment, not a measurement:

**The carrier class is exhausted as a CALLEE/VALUE-IDENTITY class.** Its definition: enforcement that
looks up a security obligation using the identity of a *callable*, and loses it when that callable is
stored, passed, joined or returned.

| keying | surfaces | carrier-vulnerable |
|---|---|---|
| **NAME** (string at the call node) | `requires` (closed), `ensures` composition (closed), sink/effect/export/trifecta name tables, bare-builtin (item 12) | yes, unless identity is recovered |
| **SET / summary** | effect rows, taint/secret labels, fn identities (singleton), gate tags | immune — the summary travels with the value |
| **TOKEN** | capability linearity, NE seal | different disease entirely |

**A falsification attempt was then run against it and FAILED** — stronger support than the original
judgment was. The hunt looked for enforcement keyed on something other than a callable's identity
and found four such kinds, none of which reopens the class:

| other keying | examples | why it is not a carrier |
|---|---|---|
| **TYPE** | `secret<T>` / `tainted<T>` formals | value-label declaration, dual to the flow SET |
| **ATTRIBUTE / MODE** | `@verified`, `@safe`, `@research`, `--allow-research` | selects which rule pack runs, not which function a value IS |
| **ORDER / CFG position** | declassify before vs after egress; path conditions | label flow through the CFG |
| **FILE / path** | package evidence binding, engagement allow-lists | artifact trust, not call identity |

So the three-row table above is **not** the claim that all Anubis enforcement is NAME/SET/TOKEN on
callables. That stronger claim is FALSE, and these four are why. The bounded claim is only about the
carrier class — enforcement that keys on a callable's identity — and within it no surface was found
that is neither name-keyed, set-keyed nor token-keyed.

Named falsifiers, kept live: a true accept about WHICH FUNCTION RUNS under a carrier that is not
reducible to a known name/tag/summary/token hole; enforcement keyed on dynamic string dispatch, a
callee's import path, or a vtable slot index; or a type-keyed rule that is really callable identity
in disguise — an `fn`-typed field obligation reimplementing name lookup.

**Two agents converged independently on the sequencing**: one resolver first, then wire the defensive
policy consumers to its exact/singleton result. The auditor declined to build a third
builtin-identity mechanism on the grounds that it would overlap the effect-tag resolver already being
designed — the second time in this arc that declining to patch was the more useful output.

### The promise sentence — SCOPED, with a mandatory refuse-tier (2026-07-28)

The completion promise was:

> `anubis check` PASS ⇒ Anubis found no way to violate contracts, effects, capabilities or
> information-flow — **and everything it could not decide, it refused rather than assumed.**

**The second clause was false as written, and the code said so in its own comments.**
`compiler/src/middle/mod.rs:273-274` states policy: *"Default-lane policy deliberately defers
Unknown; it never invents a gate."* `:6789` — *"defers (fail-open, the documented residual)"*.
`:7015` — *"DEFER the whole block (fully fail-open)"*. A deferral **is** an accept: the program
compiles, runs, and does whatever it does.

The distinction that resolves it, adopted from the adversary's round-24 judgment:

| | `check` PASS + **silence** | PASS + **visible residual** | non-zero |
|---|---|---|---|
| what the user believes | certified | knows the residual | must fix or opt in |
| clause 2 | **BROKEN** | honest **iff** the sentence scopes it | **held** |

#### The promise is scoped to SAFE MODE, and the mode matrix says exactly how much that means

An authorized mode attribute does not relax the monoids a little. Measured 2026-07-28 by firing
six minimal probes — undeclared `write_file`, `print(secret)`, `sink(taint)`, `shell`, undeclared
`send`, `assert(false)` — into each mode with its authorization present:

| mode | write | secret | taint | shell | net | `assert(false)` |
|---|---|---|---|---|---|---|
| **safe** (no attribute) | REJECT | REJECT | REJECT | REJECT | REJECT | REJECT |
| `@verified` | REJECT | REJECT | REJECT | REJECT | REJECT | REJECT |
| `@proof` (cpu and metal-hybrid) | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | REJECT |
| `@research` (authorized) | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | REJECT |
| `@audit` (authorized) | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | REJECT |
| `@fuzz` (authorized) | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | REJECT |

Inside an authorized `@proof`/`@research`/`@audit`/`@fuzz` frame the effect, secret, taint and
capability monoids are **not charged at all**. Only `assert` survives. `@verified` is a mark and
relaxes nothing — it is not in this class.

This is the design (research mode exists to permit what safe mode forbids), and it has a
consequence the fixture counts do not show: **14 shipping `EXPECT: PASS` fixtures live inside
those modes** (2 `@proof`, 10 `@research`, 1 `@audit`, 1 `@fuzz`). For each, no poison of the form
*"this value must not reach a sink"* or *"this effect must be declared"* can fail while the
fixture stays inside its mode. They remain poisonable by stripping the authorization or by a false
`assert` — except `metal_backed_proof_parity`, which asserts nothing and is therefore a total
tautology.

So the promise sentence is not merely *scoped* to safe mode as a matter of wording. Outside safe
mode there is no information-flow or capability claim to make, and a green board built partly from
in-mode fixtures is greener than the safe-mode evidence alone supports. The number to carry is 14.

**A silent deferral is an assumption of "no problem."** That is the whole defect — not the deferring.

**Measured cost of the alternative.** Making `check` refuse every undischarged obligation was
costed, not guessed: ≥15 unit tests in `compiler/src/lib.rs` *require* fail-open ACCEPT by name,
against ~485 on-disk fixtures — **order 10² currently-green accepts**, not five. Note the checker is
already partly there: `obligation_undecided_is_unsound` (`mod.rs:12535-12541`) **fails closed** on
solver UNKNOWN for `assert`/`ensures`/`requires@`/loop-invariants. The hole is *no obligation
emitted at all* — PASS with empty proof work.

**Adopted policy — three tiers:**

| tier | classes | clause 2 |
|---|---|---|
| **A — MUST REFUSE** | unknown lexer character; malformed tokens; Unknown-by-destruction in the security lanes; emitted solver UNKNOWN on an obligation | true |
| **B — deferred, NAMED** | unmodelable contract predicates; match-arm/lambda contract bodies; float/string model gaps; tag `Unknown` no-charge | true **only if named and visible** |
| **C — runtime-enforced** | explicit runtime residual | true if labelled |

**The scoped sentence, replacing the original:**

> `anubis check` PASS means: every obligation class listed as **proved** was discharged or the check
> failed; every class listed as **deferred** produced a **visible residual** — a diagnostic or a
> report field — not a silent accept; and the source bytes were **fully tokenized** (unknown
> characters refuse). Deferred classes are **not** "proved absent."

**A-tier item 1 is CLOSED (`5cf2e05`).** The lexer's character dispatch ended in `_ => {}` — an
unrecognized character was silently DELETED, so `check` certified a program that was not the program
on disk (a file containing `U+00A7` passed with rc=0). Because identifiers are ASCII-only, every
non-ASCII letter and non-ASCII whitespace such as `U+00A0` vanished the same way. It now emits a
token, the parser refuses, and the user gets a span with a caret. `@` remains deliberately dropped —
attributes lex as bare names — now explicit rather than an accident of the wildcard. Measured:
Measured at this close: security 311/311, language 244/244, stdlib 104/104, zero accept→reject flips.

Remaining A-tier items are open and named. The B-tier residual list is the enumeration in
`scratchpad/fleet_20260726/adversary_round24.md` with `path:LINE` per site and a "PASS with silence?"
column. **Do not publish the original sentence over the current behaviour** — it is the one claim a
stranger will quote back.

### Research mode — crash operations produce tamper-evident receipts (PHASE 4 CLOSED 2026-07-28)

Written by the offensive lane that measured it; adopted close to verbatim.

A green `receipt-verify` proves the chain is intact: every entry's HMAC-SHA256 links to the previous,
every MAC verifies with the engagement key, and no entry was inserted, deleted or reordered. The
chain is append-only and schema-versioned.

Crash operations (`vz exploit`, `vz fuzz`) now **require** `--engage` (`ANUBIS_VZ_ENGAGE_REQUIRED`).
Without an engagement directory they bail before cloning a guest, so the stub-identity path that
silently discarded evidence is unreachable. With `--engage` the disposable guest is scraped BEFORE
teardown (`scrape_disposable_guest`) and the action is sealed (`seal_vz_disposable_action` →
`seal_action`). The sealed receipt embeds the PoC path and SHA-256, guest identity, isolation model
(`tart-disposable-guest`), body success/failure, and scrape metadata.

**Measured 2026-07-28** in disposable guest `anubis-vz-ephemeral-3996` cloned from `anubis-xcode`,
confirmed torn down via `tart list`:

| step | count | tip |
|---|---:|---|
| `campaign-init` (control — writes a Markdown playbook) | 1 → 2 | `2092ed12` |
| `vz exploit --engage` (crash op) | 2 → 3 | `3e12992e` |
| `vz-c2-cycle` | 3 → 4 | full task results |

This inverts the defect the blueprint recorded: a Markdown file used to advance the chain while a
SIGABRT and 14 unique crashes left `receipt-verify` byte-identical. The crash op now produces **more**
evidence than the control. `vz-c2-cycle` exits 0 with all five recon modules returning on the first
poll, and the agent correctly reports `"os":"darwin"` — the blueprint's `"os":"linux"` lead was
measured and **refuted**.

**What a green `receipt-verify` does NOT prove:**

1. **That the claimed PoC actually executed.** The receipt names the file and its SHA-256; it does
   not embed a transcript or attest that the guest ran that file.
2. **That crash artifacts survived teardown.** Core dumps, ASAN reports and fuzz corpora stay in the
   disposable guest and are destroyed at `tart delete`. The receipt proves *an operation happened
   and was scraped*; it does not preserve the artifact.
3. **That the guest was freshly cloned and isolated.** `uptime: "up 25 secs"` is evidence of a fresh
   clone, not a cryptographic binding.

**Crash isolation is not an air-gap.** No zero-NIC claim without `native-preflight`.

Residual 2 — CLOSED (`bfbff02`, measured `2026-07-28`). `scrape_disposable_guest` hashes every
file in `/tmp/anubis-vz-evidence/` **in-guest** via `find -exec shasum -a 256 {} +` and seals
the digest manifest into the receipt. Three states are distinguishable: non-empty hash manifest
(artifacts existed and were hashed), empty string (evidence directory exists but contains no
files), and `ANUBIS_VZ_SCRAPE_*` marker (directory absent or SSH command failed). Empirically
verified in a disposable macOS guest: crash op receipt carries
`"artifact_digests":"d17848a9...  /tmp/anubis-vz-evidence/poc.log"`, a fresh guest with no
evidence directory returns `ANUBIS_VZ_SCRAPE_NO_EVIDENCE_DIR`, and an empty evidence directory
returns the empty string. Chain verifies (`receipt-verify --json → ok:true`). Raw crash data
never crosses the guest boundary. An auditor holding an exported artifact can verify it against
the sealed digest; the receipt proves what the scrape found, not what the guest actually did.

### Secret-selected constants — direct form CLOSED, nested form OPEN and it reconstructs N BITS (2026-07-28)

`Expr::If` discarded its condition in `expr_taint_source_m`, `expr_secret_source_m` and
`expr_param_flow`, while `Expr::IfLet` thirty lines above bound and consulted its scrutinee and
`analyze_expr_effect` — the effect lane — was clean. One shape, three functions, two lanes wrong.

```
let x = if secret_source(1) > 0 { 1 } else { 2 };   print(x);
   before:  check ACCEPT rc=0, run prints 1
   after:   REJECT, confidentiality AND integrity duals (w1, w1b, w5, w5b, w5c)
```

The implicit-flow lane never covered it: `reject_implicit_flow_under_secret_pc` fires on a public
root **assigned** under a secret PC, not on a `let` whose init is a secret-**selected** constant.
Closed in `92ff3d1`, both over-rejection guards holding, corpus 311/311 and 244/244 with zero
accept→reject flips. Locked in: `run_walker_completeness_gate.sh` now registers all three walkers,
and reverting `Expr::If.cond` in a scratch copy turns that gate RED.

**Composition, measured rather than assumed — the two halves differ and the difference is the
severity.**

| shape | composes to an n-bit secret? |
|---|---|
| **direct** value-`if` — CLOSED | **No.** A single program packing many bits is rejected as a whole: 3-bit accumulate, a `while` loop packing `acc*2+bit`, `((s>>i)&1)` extraction, arms indexed by a secret, and three sequential `print(if secret …)` all REJECT. The fix is not a one-bit patch on this form. |
| **nested** bare-`if`-in-block — **OPEN** | **Yes.** Reconstructs an n-bit secret **in one program**, ACCEPT + observed at runtime. |

**So the open residual must not be described as "a one-bit channel" — that understates it.**

```
let x = if true { if secret_source(1) > 0 { 1 } else { 2 } } else { 0 };   print(x);
   check rc=0 ACCEPT,  run prints 1
   direct twin:                     REJECT
   nested, result discarded:        ACCEPT   (correct non-observation)
   nested with `let z = …; z`:      REJECT   (statement seeder sees it)
```

**CLOSED at arbitrary depth (`1689a69`).** `parse_expr_block` treats a bare `if` as a STATEMENT
(`frontend/mod.rs:2616`, `parse_if_stmt` at `:3454`), so a nested final `if` is `Stmt::If` and never
an `ExprStmt` — the tail lookup found nothing, which is also why the `let`-bound variant already
rejected (its inner `let` goes through the statement seeder). Both label lanes now union the
condition with both arm values when extracting a block's value (`mod.rs:22216` secret, `:21533`
taint), and the recursion is structural so depth is not a special case.

| probe | before | after |
|---|---|---|
| `g_if06`, `g_if06_nested_observes` | ACCEPT | **REJECT** |
| depth-2 | ACCEPT, run printed `1` | **REJECT** `ANUBIS_SECRET_EXFILTRATION` |
| depth-3 | — | **REJECT** |
| `g_if05`, `g_if05t`, `g_nested_public` | ACCEPT | ACCEPT (controls held) |
| `w1`, `w5b`, `p2_free_push` | REJECT | REJECT (earlier closes held) |

Measured at this close: security 311/311, language 244/244, zero accept→reject flips.

**The repair reproduced its own defect once, and that is worth recording.** The first recursive
helper matched `Stmt::If { then, else_, .. }` and discarded `cond` — the identical `..` shape just
fixed in `Expr::If`, in the code written to fix it. Three occurrences of one shape in a single day.
It now breaks a gate rather than shipping: `run_walker_completeness_gate.sh` registers both helpers
under the `partial-` contract (*match what you like, but do not half-read what you matched*), and
re-planting the exact bug in a scratch copy turns that gate RED.

### VZ isolation — the tart marker is FORGEABLE; the native air-gap is STRUCTURAL (probed 2026-07-28)

Boundary item B1 has said "host-forgeable markers; operator is trust root" as an assertion. It was
probed, and the answer splits into two very different stories that share one word.

**The tart marker is trivially forgeable.** `seal_vz_disposable_action` (`vz.rs:1354`) writes
`"isolation": "tart-disposable-guest"` as a **hardcoded string literal**, set by the HOST
orchestrator — not derived from guest-side attestation, a guest co-signature, or any hardware
measurement. `seal_action` (`receipts.rs:165`) accepts any JSON payload and chains hashes and MACs
over it: **it protects the chain from post-hoc tampering, not the content from fabrication at write
time.** A host process with access to the engagement directory can produce a byte-identical receipt
with no guest involved at all.

So a green `receipt-verify` proves *this receipt was written by something holding the engagement key
and has not been altered since*. It does **not** prove a guest existed.

**The native posture is real and structural.** `anubis vz native-preflight` (`vz_native.rs`) derives
a `NativePosture` from the program's **proven effect set** and builds the
`VZVirtualMachineConfiguration` that posture implies, then validates it:

| proven effects | posture |
|---|---|
| net-free | `ZeroNicAirGap` — **zero network devices** |
| declares `net.send` | `PerHostnameEgress` — one NIC, allow-list |
| unbounded | `ZeroNicAirGap` — fail-closed on minimum knowledge |
| program fails `anubis check` | **refused** — no proof to confine from |

Zero `networkDevices` is not a firewall or a policy: the guest boots with no interface, so there is
no IP, DNS, TCP or UDP. `native_posture_is_fail_closed` (`vz_native.rs:403`) locks that lattice.
**This is the language deriving its own confinement from its own proof, and refusing to confine a
program it cannot prove.**

**The honest statement is therefore two statements.** Evidence sealed on the tart path carries a
marker an auditor must take on the operator's word. Confinement on the native path is enforced by
the hypervisor configuration and does not require trusting a string. Do not let the word
"isolation" carry the second claim's weight on the first claim's evidence.

**And the strong path is NOT the default one (2026-07-28).** `native-preflight` is a separate
backend: `VzCmd::NativePreflight` / `NativeBoot` dispatch to `vz_native`, while `VzCmd::Exploit` and
`VzCmd::Fuzz` go down the tart lifecycle. **There is no `--native` flag on `vz exploit` or
`vz fuzz`, no shared dispatch path, and no fallback from tart to native.** An operator running a
crash op gets the host-asserted marker; the hypervisor-enforced posture requires knowing to invoke a
different subcommand that does not run the crash op.

So the honest summary of research-mode isolation today is: *the weak claim is on the road everyone
travels, and the strong one is on a road you have to know exists.*

**Merging the two roads is FEASIBLE and not blocked (assessed 2026-07-28).** The obstacle looked
structural — staging a PoC into a zero-NIC guest cannot use SSH or rsync, because there is no
network device by construction. It is not structural. `VZVirtioFileSystemDeviceConfiguration`
exposes a host directory to the guest as a **virtio PCI filesystem device**, mounted with
`mount -t virtiofs`. That is not a network device and does not appear in `networkDevices`, so a
configuration with **zero NICs and one shared directory is structurally valid**: no IP, no DNS, no
TCP, no UDP, and still able to read files the host staged before boot. Every required binding was
checked present in the installed `objc2-virtualization 0.3.2`. Estimate: about a week, no wall in
Apple's framework.

That matters because it turns "the strong posture exists but nobody travels it" from a permanent
architectural fact into ordinary unfinished work — and a receipt sealed on that path could carry
`isolation_basis: hypervisor-enforced`, whose provenance is a **function of a checked artifact**
(the posture is derived from the program's proven effect set) rather than a string the host chose. Receipts now carry
`isolation_basis` (`host-asserted`) so the artifact says which one produced it — that field exists
precisely so this asymmetry cannot be inferred away by a reader.

**Attempts to anchor the tart marker were examined and rejected with a reason, not abandoned.** A
guest co-signature, a nonce challenge, and a clone-derived binding all fail for the same structural
reason: the host SSHes into the disposable guest with known credentials to stage, execute and
scrape, so any key the guest holds is readable over that channel. There is no trust boundary to
anchor to. A real co-signature needs a vTPM, a sealed enclave, or credentials the SSH user does not
hold — the CoW clone has none, and the host owns the base image. That is an impossibility result
under the current architecture, and it is why the answer is a label rather than a mechanism.

### Container-PARAM carrier — OPEN, and the boundary is the PATH, not the param (2026-07-28)

A user function carrying `uses(fs.write)` reaches an application site through a **container passed
as a parameter**, in a `main` that declares no such capability. The same container **returned** and
applied locally REJECTS. One identical value, two paths, opposite verdicts — this file's disease
sentence at an interprocedural boundary.

| shape | verdict |
|---|---|
| `app([leak])`, callee does `xs[0](…)` | **ACCEPT** |
| `app({k: leak})`, callee does `m["k"](…)` | **ACCEPT** |
| `app(Box{g: leak})`, callee does `b.g(…)` | **ACCEPT** |
| the same three RETURNED, applied in `main` | **REJECT** — all three |

**The discriminating pair narrows it to one hop.** These differ only in whether the callable sits
at a path inside the argument:

```
app(leak)     bare callable param, applied directly     check rc=1  REJECT   works
app([leak])   callable inside a CONTAINER param         check rc=0  ACCEPT   leaks
```

So the capability charge **already crosses the call boundary**; it fails only for a callable at a
PATH within the argument. Parameter bindings are seeded with empty `field_closures` /
`field_fn_identities` (`mod.rs:4466-4477`) while the return path projects them — and the applied
-parameter summary evidently records *"param 0 is applied"* rather than *"param 0's element at `[0]`
is applied"*, though `fn_applied_param_paths` (`:2709`) exists to hold exactly that.

**Three fixes have compiled and moved nothing** — a `param_sinks` extension (wrong consumer: the
witness violates a CAPABILITY, not a taint sink) and two projection attempts. Under repair.

**Seven further shapes were swept out**, none of them previously named. Re-measured 2026-07-28 on
pin `anubis-7941bf779ef7`, each with its must-stay-ACCEPT pure twin, because a leak-only score
cannot tell a correct fix from one that always charges:

| shape | leak | pure twin |
|---|---|---|
| nested `xs[0][0]` | rc=1 REJECT | rc=0 ACCEPT |
| `let f = xs[0]` then apply | rc=1 REJECT | rc=0 ACCEPT |
| call under `match`/`if` | rc=1 REJECT | rc=0 ACCEPT |
| `for f in xs` over a param | rc=1 REJECT | rc=0 ACCEPT |
| `let ys = xs` alias | rc=1 REJECT | rc=0 ACCEPT |
| `xs = [leak]` reassign-after-pure-arg | rc=1 REJECT | rc=0 ACCEPT |
| method-formal param | rc=0 **ACCEPT — still open** | — |

The adversary's prediction that most fall out of a projection **total over apply sites**, rather
than one aimed at the list-index form, held: six of seven closed, and they closed through five
producer edits (literal, param, loop binder, extraction, assignment) rather than seven fixes.

**Three of these seven had no witness in the corpus when this section first claimed they were
ACCEPT.** `nested`, `let f = xs[0]` and call-under-`match` were named from reading the compiler, not
from a run. They were built and measured for this revision and all three reject. An unbacked ACCEPT
in the trust anchor is the same defect as a gate that passes an empty corpus, and it sat here for
several rounds.

**`xs = [leak]` reassign-after-pure-arg is WITHDRAWN as a finding rather than closed.** Its
fixture declared `uses(fs.write)` on both the forwarder and `main`, so the write was authorized and
ACCEPT was the only verdict available — no compiler change could ever have made that file reject.
The properly-formed witness (declarations stripped) rejects. Third malformed fixture this session
after the root guard and the round-14 set, all with the same shape: **the test granted the thing it
was trying to catch.** An ACCEPT is evidence of a defect only once the fixture has been read and a
rejection confirmed available.

**The callable story is therefore not closed.** The residual grew rather than shrank once a tool
existed to look for it: a consumer-surface sweep on pin `anubis-566b336ca043` found **ten** shapes
still accepting a leak that writes a real file — element-alias, pattern-bind, return-then-param,
closure-param, method×for, direct_param_lambda, if_bind, method_formal, ret_lambda_param and
match_param_elem. Five were known; five were found by probing the consumer side after six producers
had been closed. `for_param_lambda` closed in the same window, and `while`, `pop`, `map`,
struct-field and path-let are closed for named free functions.

#### The pure twins for those ten prove NOTHING, and that is now measured rather than suspected

This file has warned for weeks that **a no-op fix passes every must-stay-ACCEPT guard**. That was
stated as a caveat. It is now a measurement.

`scripts/guard_reachability.sh` is the dual of the fixture preflight: it poisons a passing guard and
asks whether the analysis notices. A guard that accepts the pure program AND accepts the poisoned
one is not evidence of correct restraint — the analysis never reached the shape at all.

```
g_elem_alias_pure         BLIND   pure ACCEPT, poison ACCEPT
g_pattern_bind_pure       BLIND
g_return_then_param_pure  BLIND
g_closure_param_pure      BLIND
g_method_for_pure         BLIND
```

All five. So "the pure guard still ACCEPTS" — cited repeatedly this session as evidence that a fix
did not over-charge — **carries no information for an OPEN shape**. It reads as an all-clear and
means "we never looked." A pure twin only becomes evidence once its shape is CLOSED and the poisoned
twin rejects.

This does not retract the closes recorded above: each of those has a leak twin that REJECTS, which
is what makes its pure twin meaningful. It retracts the implied assurance for the ten still open.

#### What the shipping corpus actually is

The same sweep put the 317-fixture suite through the preflight. **Zero** `EXPECT: FAIL` fixtures are
dead: all 213 genuinely fail and all 213 hit their declared `ERROR_CONTAINS` needle. No w06b-class
fixture — one that grants the capability it tests — exists in the shipping corpus; the three found
this session were all in scratch adversary sets. A naive `uses_on_path` scan flags 108 of the 213,
and all 108 are dual-concern fixtures where the effect declaration is legitimate and the rejection
is for confidentiality. That heuristic is therefore correct for adversary launder claims and wrong
as a corpus gate, and is not used as one.

The 104 `EXPECT: PASS` fixtures have now had it, twice, and the two runs say different things.

**Entry-point poison: 104/104 REACHES.** An unauthorized effect helper called from `main` is
charged in every shipping PASS fixture, so the instrument is not dead on `main`. That is a weaker
result than it sounds and the adversary said so unprompted: it does not show that the property
path each fixture claims to guard was analyzed, only that *something* was.

**Path-aligned poison** — a poison that violates the same property along the same construct,
derived mechanically from the program text (strip a `declassify`, strip a `uses(...)`, turn a
pure callable in a list into a writer, break an `ensures`) — gives the honest picture:

| verdict | count | meaning |
|---|---|---|
| REACHES_PATH | 72 | the poison rejects: the guarded path really is analyzed |
| UNDERIVED | 25 | no strategy produced a twin — since hand-written: **24 REACHES, 1 tautology** |
| MODE_ALLOWS | 5 | `@audit`/`@research`/`@fuzz`: the poison is *authorized*, not unseen |
| OFFTOPIC_POISON | 1 | the derived poison does not touch the guarded path |
| MODE_OR_OFFTOPIC | 1 | either of the above; not separable mechanically |
| **BLIND_SHAPE** | **0** | **no safe-mode PASS fixture accepts a path-aligned poison** |

The 25 have since been hand-written — a named property and a poison violating it for each —
and they came back **24 REACHES_PATH and 1 tautology**. Combined: **104/104 of the shipping PASS
corpus is non-blind in safe mode**, every fixture carrying a property that something can violate.

**The one exception is a fixture that cannot fail.** `metal_backed_proof_parity` runs under
`@proof`, and inside that mode BOTH a secret sink and an undeclared write are accepted — only
leaving the mode rejects. Its property cannot be violated without changing the mode, so it tests
a tautology. It is counted as coverage today and earns nothing, and it is recorded here as one
rather than folded into the 104.

That distinction is the residual: a broad authorization mode (`@proof`, `@research`, `@audit`,
`@fuzz`) can make a fixture unfalsifiable inside itself. How many others do so is not yet known.
It is a question about the MODES, not about the fixtures — if a mode enforces nothing, no fixture
written inside it can be evidence of anything.

The boundary is itself a finding: **path-aligned poison derivation is ~75% mechanical and cannot
be finished mechanically.** Deciding whether a poison is off-topic requires knowing what the
fixture *meant*, and distinguishing "the shape was never walked" from "the poison is permitted
under this mode" requires reading the mode attributes. `MODE_ALLOWS` is the trap: five fixtures
accept a stripped `declassify` because their mode authorizes it, and scoring those as BLIND
would have invented five defects that do not exist.

### Phase 4 done-when: all five criteria met, with the residual named (2026-07-28)

| blueprint criterion | evidence |
|---|---|
| a crash op produces at least as much evidence as `campaign-init` | `clean` / `target_nonzero` / `target_signal` + `evidence_of`, measured at exits 0 / 42 / 139 |
| `receipt-verify` COUNT and TIP advance after a guest run | `count=1 tip=d3174939…` → `count=4`, tip advancing per run |
| artifacts present on the host after teardown | guest-computed `526f5e1d…` reproduced host-side, 0 VM processes |
| `vz-c2-cycle` returns results | `t1_encrypted_c2  PASS  (whoami over aop-2)` — listener + agent + task dispatch, `"ok":true` |
| the residual is stated | four named limits, below |

**`vz_c2_cycle` remains marked `PARTIAL` in the capability map, and that marking is correct.** The
criterion asks whether the C2 cycle returns results; it does, on the tart lane, with the gate's own
record above. `PARTIAL` marks something stronger and separate: there is no NATIVE-lane C2, so a C2
cycle cannot yet run inside the hypervisor-enforced zero-NIC posture. Meeting the criterion does
not upgrade the marking, and the marking is not evidence the criterion failed.

What an auditor still cannot conclude from a green run: the receipt chain proves INTERNAL
CONSISTENCY, not host honesty, remote attestation, artifact tamper-freedom, or classification
correctness. `crash_classification` is heuristic — `exit > 128` is read as signal death by Unix
convention, and a program can `exit(139)` deliberately.

### VZ networking: the whole lifecycle now reports OBSERVED state, or refuses (2026-07-28)

Source `e5649cb`, verified on one signed immutable pin
`ae5b0d692db9a0c67376621c2f1a7c009b80ef78b76c5e87e3ead851254dbc5e`
(`com.apple.security.virtualization=true`, mode 555).

| surface | behaviour now |
|---|---|
| request CLI | typed `off\|loopback\|nat`; `typo` / `unknown` parse-fail |
| Tart `off` / `loopback` | structural refuse — `ANUBIS_VZ_NET_STRUCTURAL` |
| Tart explicit `nat` | canonical default argv, no `--net-softnet` |
| inventory / status | **`unknown`** — Tart exposes no launch mode |
| generic exec | **`unknown`** |
| already-running start | **`unknown`** |
| fresh controlled start | `nat` — we launched it, so we know |
| stress metadata | `nat` |
| help / docs / catalog | no "network isolated" or "no egress" overclaims |

The rule underneath: **a label is honest only where the code OBSERVED or CAUSED the state.**
Everywhere else it refuses. This morning `--network off` reported isolation it did not have, and
`tart_reported_network()` returned a constant — the same defect twice, in opposite directions.

Verification on that pin: 251/251 binary tests · Lean formal gate PASS · Tart live matrix PASS ·
native guest-side `ZERO-NIC PROOF VERIFIED` · offensive stress 34/34 with SHA parity and
`torn_down` · receipt chain `LAB_REAL_HMAC` · disposable-guest cleanup confirmed.

**Explicit non-claims, which are what make the above readable:**

1. The delegated independent review (`deleg_f0a99855`) **did not pass** — it died on HTTP 429 after
   inspecting diffs. Local audit, tests and live evidence were used instead. A review that
   terminated is not a review that approved.
2. Legacy `vmctl` live enforcement is **UNAVAILABLE** — its installed launcher targets a missing
   executable. Covered by Rust mapping and unknown-status tests only, never marked PASS.
3. A stress guest's `git_head` can differ from the host pin source; the **binary SHA** is the
   provenance pin, not the commit.
4. Earlier pins are historical only.

### Native VZ: what a green `receipt-verify` does and does not establish (2026-07-28)

The native path (`anubis vz native-boot`) now runs the language inside a hypervisor-enforced
zero-NIC guest and chains the result. Measured end to end:

| link | evidence | negative control |
|---|---|---|
| zero-NIC | virtio-net `0x1af4:0x1041` ABSENT from the guest's own PCI bus | egress posture makes it APPEAR and the probe reports `eth0` |
| instrument live | virtio-console `0x1af4:0x1043` present in BOTH postures | a probe that cannot see a NIC is refused as `PROBE_BLIND` |
| staging | canary CONTENT HASH computed inside the guest | no `--staging-dir` gives mount 255, empty glob |
| the language ran | `anubis check` in-guest, exit 0 | — |
| crash vs death | `clean` / `target_nonzero` / `target_signal` + `evidence_of` | exit 0 / 42 / 139 each classify differently |
| chain advances | `count=1 tip=d3174939…` -> `count=4`, tip per run | — |
| artifacts survive | guest hash `526f5e1d…` reproduced host-side, 0 VM processes | — |

**What it does NOT establish**, and this is the part that matters:

> The receipt chain proves INTERNAL CONSISTENCY. It does not prove HOST HONESTY, REMOTE
> ATTESTATION, ARTIFACT TAMPER-FREEDOM, or CLASSIFICATION CORRECTNESS. The chain is as trustworthy
> as the host that captured it.

Plus one the receipt states about itself, beside the inference it qualifies: `crash_classification`
is heuristic — `exit > 128` is read as signal death by Unix convention, but a program can
`exit(139)` deliberately.

So the isolation is hypervisor-enforced and confirmed by the guest; everything DOWNSTREAM of the
guest passes through the host and inherits the host's trustworthiness. That is a smaller claim than
"the guest was isolated and we know what happened in it", and it is the one being made.

**This replaces a claim that was false this morning.** `--network off` on the Tart wrapper reported
`network: off` while running full shared NAT — four call sites labelled a NAT-connected guest as
isolated. Any evidence produced under that flag carried an isolation claim the hypervisor never
enforced. It now refuses and names the path that does enforce it.

### HISTORICAL, SUPERSEDED BY ITEM 21 — carrier class measured 40 of 41 closed (2026-07-28)

At this dated snapshot, the 41 published-open carrier routes stood at **40 CLOSED, 1 open**, with
25 must-stay-ACCEPT pure guards passing and zero over-rejection, security 317/317 (**historical
stamp, 2026-07-28**). The
element-materialization ladder was 25/25. Item 21 later falsified this as a current class-level
closure with place-assignment, control-flow, container, and call-site true accepts.

**The one residual in this historical snapshot was `rec_build_then_app`; item 21 names the larger
current open inventory.** Its mechanism was a summary-shape problem, not a missing rule:

```
fn go(n, acc) { if n <= 0 { acc } else { go(n - 1, acc + [leak]) } }
let xs = go(2, []); xs[0](…)          check PASSES
```

Traced to `fn_sole_return`. Its `.or_else` accepts an `If`/`Match` TAIL expression only when
`tv.len() == 1`, and `expr_tail_values` PEELS an `If` into both branch values — so `tv.len() == 2`
and `go` never enters the map at all. Every downstream resolver then has nothing to read.

Two rules were added and both work; neither can fire because the summary is absent:

- `fn_identities_carried_by_value` now handles `Expr::Binary`, so a container built by `+` resolves.
  The NON-recursive form of the same program (`return acc + [leak];`) closed on this alone.
- a recursive builder over-approximates by unioning every container literal in its return, bounded
  and gated on self-reference.

Loosening the `tv.len() == 1` guard would fix it, and that guard is read by four lanes — the
forwarder lane depends on unanimity, and this file's recorded history includes a case where
loosening `unanimous_forwarded_return` broke the lane it exists for. So it needs its own
full-corpus verdict-diff rather than a same-session edit.

Published as a bounded residual with the mechanism, the file, the exact predicate, and the reason
it was not changed — not as an unexplained red.

### Historical widening snapshot — 41 open routes before the historical 40/41 result above (2026-07-28)

This section records the discovery census that preceded the then-current 40-closed/1-open result
above. Both snapshots are superseded by item 21; neither is a present-tense count.

Ten carrier shapes were closed on 2026-07-28, each with a rejecting discriminator AND a guard that
rejects poison. Red inventory zero, both held fixtures rejecting, security 317/317 (**historical
stamp, 2026-07-28**). On that
evidence Phase 1 looked finished.

It is not. The blueprint's last criterion is *"a fresh adversarial hunt over the carrier surface
returns no new form"*, and the widest hunt yet run says:

```
round 1:  routes tried  65   expressible  65   MALFORMED 0   CLOSED 24   YES_OPEN 41
round 2:  routes tried 157   expressible 157   MALFORMED 0   CLOSED 71   YES_OPEN 86
```

The instrument held as the hunt scaled: MALFORMED stayed at ZERO across 157 probes, so every one
had a rejecting direct twin and no capability granted on its path. That column is what makes 86 a
finding rather than a pile.

Zero MALFORMED is what makes the number mean anything: each probe had a rejecting direct twin and
no capability granted on the path, so 41 ACCEPTs are 41 findings rather than 41 badly-built
fixtures. Evidence: `scratchpad/fleet_20260726/adversary/p15r1/`.

**They are not 41 independent defects.** Grouped:

| n | surface |
|---|---|
| 8 | higher-order builtins — `map` `filter` `reduce` `any` `each` `sort_by` `count` `fold` |
| 14 | container routes — `zip`/`flatten`/`reverse`/`drop`/`first` extraction, nesting, concat |
| 6 | method-shaped |
| 5 | loop shapes |
| 3 | option/let binders — `if let Some` / `while let Some` |
| 3 | map iteration |
| 2 | identity / generic pass-through |

**The first repair closed 7 of 41 — and I first reported it as 0, wrongly.** The HOF edit was
measured against a STALE BINARY; `publish_pin --verify` had passed at the time of the build but the
rescore ran against an older pin. Re-measured on a fresh pin:

```
R1 open (41) after the HOF edit:  CLOSED 7   still open 34
map_ho · each_ho · filter_ho · reduce_ho · any_ho · sort_by_ho · count_ho   all now REJECT
```

That is the fifth stale-binary measurement this session and the first that produced a WRONG
PUBLISHED NUMBER rather than a caught one. Recorded rather than quietly corrected, because the
lesson is that `--verify` passing at BUILD time says nothing about which binary a later command
resolved.

The lane question it appeared to answer is still answered, just by better evidence: the decider is
the **capability** charge (`apply_inherited_capability`, `mod.rs:26311`) reached through the
applied-parameter consumer, not the taint/secret blocks — those can only emit
`ANUBIS_INTERPROC_SINK`/`EXFILTRATION` and could never move a capability route.

The ten closed shapes were a SUBSET of the class, not the class. This repo has recorded the lesson
before, on this exact surface: *the higher-order surface is infinite whack-a-mole — fix the
abstraction, not the form.* Four hunt rounds each found more until one path-keyed closure
abstraction closed the class and roughly forty adjacent forms with it.

**Phase 1 is therefore REOPENED.** Nothing about the ten closes is retracted — each still rejects,
each still has a reaching guard. What is retracted is the inference from "the known list is
closed" to "the class is closed", which is precisely the inference the hunt criterion exists to
forbid.

### The float contract lane is NON-DETERMINISTIC at the solver budget (OPEN 2026-07-28)

`float_contract_monotonicity_accepts` proves `0 < x < 1 ⇒ x*x < x` in QF_FP Float64/RNE. Whether
it proves it depends on z3's search path, not on the program:

```
isolated, load 4.9   rc=1 rc=1 rc=1      31.2s 31.4s 31.3s
isolated, load 3.7   rc=0                (same pin, same fixture, minutes apart)
full corpus run      243/244 twice
```

`Z3_ARGS` is `["-in", "-smt2", "-t:10000", "-T:20"]` — a 10s soft and 20s hard budget per query,
hardcoded. The obligation sits close enough to that ceiling that the same binary on the same input
lands on either side of it.

**This makes `244/244` a number that is not reliably true**, and it has been cited as one. The
measured value on 2026-07-28 was 243/244 on two clean runs.

Two things are worth separating. The compiler's behaviour is CORRECT: it fails closed with
*"solver could not decide this contract within its time budget … an undecided postcondition is not
a proof"*, so a timeout surfaces as a red fixture and never as a false accept. The GATE's behaviour
is not acceptable: a corpus whose verdict depends on solver luck cannot be a seal input.

Not fixed here. Raising `Z3_ARGS` is a global change affecting every obligation in the language and
needs its own verdict-diff over the full corpus — exactly the discipline that would be violated by
tuning a constant to make one fixture green. The options, in order of preference: measure a
SUCCESSFUL run's actual time and set a budget with real headroom; restate the obligation as the
compiler's own error suggests; or give the FP lane its own budget separate from the integer lane.

### Semantic diagnostics carry NO location — the refusal has no address (OPEN 2026-07-28)

`anubis check` reports the failures that enforce the entire promise — the security lanes — as a bare
string with no file, line, or column, while a LEXER error in the same binary produces
`file:line:col`, the source line, and a caret. Measured:

```
lexer :  lex_stray.anb:4:1: error: expected statement
              4 | §
                | ^
semantic:  check failed: ANUBIS_SECRET_EXFILTRATION: secret `x` flows to egress `print` …
```

On a 500-line program the user is told a violation exists and left to find it. *"Everything it could
not decide, it refused rather than assumed"* is half a promise when the refusal has no address.

**The span is not missing.** `SemanticDiagnostic` carries `span: Option<(usize, usize)>`
(`mod.rs:96-100`) and `lsp_analysis::analyze_source` already converts it to line/col for the editor.

**A CLI-only fix was built, measured, and REVERTED — recorded because the reason is the finding.**
Rendering those diagnostics revealed that every semantic diagnostic reports **1:1**: the checker
emits the whole `ANUBIS_TYPECHECK` family as one diagnostic whose span is the start of file, not the
violation site. A caret pointing at `fn main() {` when the violation is at `print(x)` is worse than
no caret — it misleads with authority. Shipping it would have dressed a coarse span as precision.

The attempt also produced a defect worth recording: printing `analyze_source`'s RE-DERIVED
diagnostics *instead of* the authoritative `check_error` dropped `ANUBIS_ASSERTION_DISPROVED` from
two fixtures (security 311 → **309**). A consumer substituting its own recomputation for the
producer's answer — this file's disease sentence, committed by the lead, caught by the corpus before
commit.

**The real fix is compiler-side: give each semantic diagnostic the span of the construct that
violated.** The CLI rendering is then a few lines and is worth having. Until then this is a named
residual, not a papered-over one.

### Generics are a STRING HEURISTIC — two measured defects in opposite directions (OPEN 2026-07-28)

`is_generic` (`compiler/src/middle/ty.rs:258`) decides whether a type annotation is a generic
parameter — and therefore erased and compatible with anything — from the SHAPE OF THE STRING:

```rust
if t.contains('<') { return true; }
!t.is_empty() && t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase())
```

Both halves are wrong, in opposite directions. Measured on pin `anubis-791cfaf79812`, each with a
discriminating control:

| # | program | rc | should be |
|---|---|---:|---|
| 1 | `fn pick<Item>(a: Item) -> Item` … `pick(1)` | **1 REJECT** `ANUBIS_TYPE_MISMATCH: expects Item, got u32` | ACCEPT |
| 1c | `fn pick<T>(a: T) -> T` … `pick(1)` — identical but a 1-char param | **0 ACCEPT** | ACCEPT |
| 2 | `let x: Option<u32> = "hello"` | **0 ACCEPT** | REJECT |
| 2c | `let x: u32 = "hello"` | **1 REJECT** `ANUBIS_TYPE_MISMATCH` | REJECT |

**(1) OVER-rejection in the enforcing path.** `t.len() <= 2` means `T` and `U` are type parameters
but `Item`, `Key` and `Value` are not. A valid, running program is rejected *because its type
parameter has more than two characters*. The only difference between rows 1 and 1c is the length of
a name.

**(2) UNDER-rejection — a type-lane soundness hole.** Any annotation containing `<` is called
generic, hence erased, hence compatible with everything, so a concrete instantiation accepts a value
of an unrelated type. `Option<u32> = "hello"` type-checks; `u32 = "hello"` does not.

**Not fixed by a better heuristic.** Widening "generic" to any capitalised identifier would make a
concrete struct named `Point` erasable — trading an over-rejection for a worse under-rejection. The
sound fix consults the DECLARED parameter list (`ctx.fn_generics`, `mod.rs:2868`) rather than
guessing from the string, which means threading context into a leaf module. Recorded here rather
than half-fixed without the required compiler-lane ownership and full-corpus verdict diff.

Fixtures: `scratchpad/fleet_20260726/w19/generics/g1..g4`.

### Open — boundary honesty / process (stable IDs B1–B5; not silent overclaims)

B1. **VZ isolation is SAFETY, not SECURITY** — host-forgeable markers; operator is trust root.
B2. **Research elevation requires authorization** — bypass **CLOSED** (`e6ebfd2`); dual-use
   research remains intentional with explicit authorization, not a Safe free ride.  
B3. **Harness integrity + instrument fact — the two NAMED defects are closed; the CLASS is NOT
   (re-opened 2026-07-28 by the 67-script audit, `docs/HARNESS_INTEGRITY_AUDIT_2026-07-28.md`).**
   Language fixtures defaulted
   **DEBUG** while security graded **RELEASE**, so the two headline numbers described two different
   compilers. Both gates now use one instrument cascade and `audit_unified.sh` exports `ANUBIS_BIN`
   after the build, so CI cannot publish two builds as one number.

   Two further instrument defects were found and closed while verifying this: a fixture with no
   `EXPECT:` header was graded **expected-to-pass** (a headerless `*_rejects.anb` containing a real
   leak scored GREEN — demonstrated, not inferred), and the seal itself printed **SEAL_PASS** with
   two required gates SKIPPED, a constituent grading `/tmp/WRONG-BINARY`, and a recorded snapshot
   hash that did not match the artifact. Both are closed with microbenches that show each guard
   FIRING rather than merely present.

   Binaries are now published as source-and-binary-addressed read-only pins
   (`scripts/publish_pin.sh`) so neither a rebuild nor a new source epoch can mutate or rebind the
   instrument an agent is mid-measurement on. Ordinary pins remain bounded technical evidence;
   release publication requires clean `--release` plus closing `--verify-release`.

   **Re-opened as a named residual (2026-07-28).** All 48 `scripts/run_*.sh` plus 19 support gates
   (67 scripts, 11,605 lines) were audited against five questions, with every red claim handed to an
   independent verifier told to refute it: 35 confirmed, 11 refuted. `scripts/lib/gate_common.sh` is
   sound and its guards do fire — `parse_expectation` reads `EXPECT:` only from the leading comment
   block and rejects symlink/empty/duplicate/conflicting headers; `require_nonempty_corpus` refuses
   count 0; `finalize` refuses counters that do not sum. **The defect is ADOPTION: 13 of 48 gates
   call it.** Four findings, the first two reproduced by the lead:

   - **`run_docs_drift_gate.sh` reports PASS having tested nothing — DEMONSTRATED.** Its verdict
     reads only the failure counter (`:344 if [[ "$FAILS" -eq 0 ]]`); `$STAMPS_CHECKED` is printed
     in the headline and never asserted. Run against an empty scan root it prints
     `DOCS_DRIFT_GATE: PASS` / `Overall: PASS (0 stamps checked, 0 drift)` and exits 0. Not
     flag-only: `docs_drift_scan.py:165` skips a missing owned doc with a bare `continue`, so a
     rename produces the same vacuous green — and **2 of its 15 declared owned docs
     (`SPEC_1_0_FREEZE.md`, `TUTORIAL.md`) are absent and silently skipped today**, so every
     published stamp count already excludes them. The seal consumes this gate by matching
     `^DOCS_DRIFT_GATE: PASS` (`run_seal_checklist.sh:741`). `SCAN_RC` is captured at `:113` and
     never read.
   - **`run_shadow_diff.sh` is vacuous by construction** — it harvests `ANUBIS_SHADOW:` stderr lines
     and there are **zero** such emit sites in the compiler, so it has no failing input.
   - **`run_offensive_platform_gate.sh:423-427` records `t3_uds` PASS in BOTH branches** — one of
     the `34/34` cannot fail. Scoped: it is the only instance in that file; the adjacent `t3_dns`
     is its own control.
   - **Test-filter validation is by substring** — `run_keychain_se_gate.sh:14,17` filter on
     PREFIXES of the real test names, and libtest exits 0 when a filter matches zero tests. Same
     shape in `run_vz_apply_gate.sh` and three `run_dx_gate.sh` checks, which grade on the string
     `test result: ok`.

   **Status update 2026-07-29 — do not read the historical bullets above as current closure.**

   - **Docs drift: locally CLOSED and directly re-run.** The canonical gate now requires a non-zero
     tested count, enforces a 40-stamp coverage floor, and passes `--require-owned-files` for the live
     repository so a renamed/missing owned document is a failure rather than a skipped file. An
     empty scan root exits 1 with `tested nothing`; semantic drift writes a machine-readable FAIL
     report, symlinked parent directories are rejected, and the full test passes under
     `PYTHONOPTIMIZE=1`. The canonical self-test reported 40 stamps and zero drift.
   - **Docs-stamp floor recalibrated 42 → 35 (2026-07-29).** Seven archived or pin-bound
     observations were previously miscounted as live stamps. Their historical numbers remain intact
     and are now explicitly marked on-line; no live-owned document was removed and semantic claim
     scanning remains enabled for all of them.
   - **Shadow and `t3_uds`: source repairs are present; verification remains pending.** Shadow now
     has emit sites and labels a zero-diagnostic run `VACUOUS`; the UDS case no longer records PASS
     in both branches. Neither is promoted here to VM-sealed closure.
   - **Filtered Rust tests: source repair present, current-source build verification pending.**
     `assert_rust_tests_exercised` sums passed tests across libtest harnesses and rejects missing,
     failed, malformed-positive, truncated, or zero-test summaries. `assert_anubis_tests_exercised` likewise rejects `0/0`, partial,
     duplicate, malformed, and mixed valid-plus-malformed CLI summaries. Coverage floors require a
     canonical integer and update by temporary-file rename. The shared microbench is 22/22. Keychain
     uses exact names; VZ-apply and DX call the shared assertions. Synthetic VZ/DX zero-test runs are
     rejected, but no fresh project build was performed in this host lane.

   - **LSP harness lifecycle: locally CLOSED and pinned-binary re-run.** The reader has a real
     deadline; success requires shutdown response ID 3, server exit 0, no forced termination, exact
     capability/diagnostic/hover shapes, and stable binary identity. Protocol controls are 6/6 and
     the frozen-pin roundtrip exited 0 with `completion_ok=True` and `forced_termination=False`.

   - **Runtime evidence identity: source-verified; VM seal pending.** The current
     release build succeeds and the complete `anubis` binary suite is **351/351**, including filtered
     scope, symlink/special-file/unreadable-path refusal, and changing-tree detection. Fresh host
     runtime-probe/runtime-plan schema 1.1 evidence bundles both pass manifest verification and
     report matching complete walks; they explicitly remain non-atomic against adversarial
     flip-back mutation. The disposable-guest seal remains pending. The behavior in
     `docs/CLI.md` describes the source contract, not a current-source release verdict.

   **Instrument provenance is the most common defect:** the 2026-07-28 census found nine gates
   grading whatever sat at `target/release/anubis` with no freshness or digest check. VZ-apply and
   DX now resolve a published/explicit pin and record identity; the remaining population has not
   been re-enumerated here and stays open rather than inheriting a guessed count.

   Not claimed closed. The fix that would close the class is a shared coverage assertion in
   `gate_common.sh` that every gate calls with its own counter, enforced by the adoption check —
   patching the four individually is how this class survived to the audit that found it.

B4. **Function-identity carrier — CLOSED 2026-07-27 (`0eb5977`).** A function reference reaching an
   application site through a container built by `push`/`insert` rather than a literal
   (`let fs = []; push(fs, key); app(fs)`) accepted and printed the secret at runtime.

   Cause: the push seeder computed labels with `expr_secret_source_m` / `expr_taint_source_m`, which
   recognise a `Var` only when the LOCAL BINDING carries the label — sound for VALUES, but a bare
   reference to a top-level `secret<T>`-returning function has no local binding, so the pushed
   element read clean. It now uses `container_element_secret` / `container_element_taint`, the same
   helpers the literal twin (`[key]`) and eta twin (`push(fs, || key())`) already routed through.

   An earlier candidate fix was measured INERT and REVERTED rather than shipped: a synthesized
   `|| key()` captures nothing and has no effect, so the push resolver's capturing/effectful
   fallbacks both missed it. Dead code that looks like a fix is worse than a stated gap.

   `let fs = push(e, key); app(fs)` still accepts and is NOT a false accept: `push` returns `0`
   rather than the container, so the program panics before reaching any sink. That is a deferral on
   an unrunnable program — and separately a footgun, tracked as boundary item B5.

B5. **`push` expression-position return — CLOSED 2026-07-27.** `let ys = push(xs, 3); len(ys)` had
   `check` rc=0 and `run` rc=1 (panic): the expression lowering returned `AnubisValue::Int(0)`, a
   placeholder, so functional-style use silently bound a non-container. It now returns the container,
   matching its siblings (`pop`, `insert`, `remove` all return something meaningful). Statement-position
   `push(xs, v)` is lowered separately and unaffected; both are pinned by
   `push_expression_returns_container_doc_ok.anb`. Opened and closed the same day.

### Resolved this arc (stable IDs R1–R8; do not re-open without new evidence)

R1. ~~Published security red inventory (the eleven + D/research witnesses in corpus)~~ **EMPTY**
   this pass — live zero known-red.  
R2. ~~D1/D2/D3 field qualifier through CALL result~~ **RESOLVED** `f9fc7a7`.
R3. ~~D4 enum-payload qualifier at match binder~~ **RESOLVED** `c9415b7`.
R4. ~~Research-block authorization bypass~~ **RESOLVED** `e6ebfd2` — bare `@research` no longer
    elevates Safe.  
R5. ~~Unknown attributes silently ignored~~ **RESOLVED** `ec65724` fail-closed.
R6. ~~Four mechanisms for the original eleven~~ **RESOLVED** (M1, M2-reg, M2-B, M3).
R7. ~~Declared returns / R1 fields / (R)+PCA / stdlib 45/45 / capset 5/5~~ **RESOLVED**.
R8. ~~P0 mul hang / Tier 1 / self-host harness~~ **RESOLVED**.

**Status vocabulary:** freestanding **REAL** / production-grade / fully proven / "roadmap
complete" stamps are banned unless the same line cites a re-runnable command + observation (or a
dated seal path that is not re-read as current). A claim is (a) re-runnable with command +
observation, (b) sealed under a dated artifact path, (c) **partial** with the gap named
(**named residual**), or (d) **not claimed**. Aspirational work is a named residual — never a
silent DONE.

## Portable 1.0 surface (grounded)

**Read with § Known open issues.** Rows marked **CLAIMED** below are *true for their cited
command/fixture shape* (or a dated seal). They are **not** a claim that Safe-mode soundness is
total: the open false-accept / walker-parity class (item 1 above) is an explicit **named residual
on every taint / secret / capability / effect row** until OPUS5's queue is empty and re-hunted.

| Claim | Evidence (command + observation) | Boundary |
|-------|----------------------------------|----------|
| Evidence-native compiler/toolchain | `cargo build -p anubis` (workspace); CI sealed suite on branch | Not a claim about every possible target triple |
| Safe taint enforcement | security **327/327** (lead) / red list empty live; original D1–D4 fixture shapes reject; taint selfhost **0 disagreements** | **PARTIAL as total** — item 21 reopens broader composition/carrier routes; green = **no KNOWN defects**, not no defects. Stdlib **104/104** |
| Declassification policy | declassify accept/reject fixture pairs under `tests/fixtures` / security fixtures | Lab policy surface, not a full IFC type system; shell declassify accept is check-policy only (`run` non-run by design — CLAIMS open §2) |
| Solver correctness (supported int fragment) | **lead-verified:** `bash scripts/run_native_authoritative_gate.sh` → **PASS, 882 files, 0 mismatches** | Division deferred; var×var mul claimed; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` |
| Wrap-safety VCs (AoRTE-lite) + CEX possible fix | **CLAIMED 2026-07-25; free×free closed 2026-07-25** | On modelable ints: auto wrap-safety for `+`/`-`, **var×const `*`**, and **free×free `*`** via **offline interval product** (no SMT smul hang): bounded factors → prove; unbounded → `ANUBIS_WRAP_RISK` + possible fix; opt-out `ANUBIS_WRAP_SAFETY=0`; unit `cargo test -p anubis-compiler --lib wrap_safety` → 6+; see [`SPARK_VS_ANUBIS.md`](SPARK_VS_ANUBIS.md) | Residual: free `ensures(result == x*y)` posts can still be slow under native-authoritative (separate from wrap-safety); compound factors only offline-proved for simple `bvadd`/`bvsub`/const/var shapes |
| Implicit secret→public (PC) + explicit secret→public (Safe) | **CLAIMED 2026-07-25 for cited fixtures; PARTIAL as total IFC** | Method formals + declared returns + R1 + original D1–D4 call/match fixture shapes; **security 311/311** lead / red list empty | Residual: full PC-join plus item 21's reopened place-assignment, control-flow, container, and call-site routes |
| Symbolic-index secret-capturing closure application | **CLAIMED 2026-07-25** | `arr[idx](…)` with non-literal `idx` fail-closed when container holds secret/taint-capturing element (j1 twin of `let g = arr[i]`); unit `symbolic_index_secret_capturing_list_application_fails_closed`; clean symbolic still accepts | Residual: full PC-join; untyped formals still interproc |
| Nested container closure application (`outer[0][0]`, `b.fs[i]`, bind + mid-bind) | **CLAIMED 2026-07-25** | Nested Index/FieldAccess CallExpr + **bind** (`let g = outer[i][0]; g(0)`) + **intermediate mid-bind** (`let mid = outer[0]; mid[0](0)` re-keys `field_closures`; symbolic mid union-projects first segments fail-closed); unit `nested_container_closure_application_fails_closed` (apply + bind + mid lit/sym/clean); clean nested still accepts | Residual: full PC-join not claimed |
| if-expr-built containers seed `field_closures` (incl. nested `Stmt::If` + let-inner) | **CLAIMED 2026-07-25** | `collect_container_closures` walks `Expr::If`/`Match`/`Block`; nested bare `if` as `Stmt::If`; unit `nested_container_closure_application_fails_closed` | Residual: full PC-join not claimed |
| `push`/`insert` seed capturing lambdas into `field_closures` (free + method) | **CLAIMED 2026-07-25** | `apply_container_mutation_taint` seeds pushed/inserted lambdas; concrete path miss fail-closes via `any_capturing_field_closure`; free `push(arr, lam)` + method `arr.push(lam)`; unit cases in `nested_container_closure_application_fails_closed` | Residual: full PC-join; HO rebind beyond push/insert seed |
| Verified causal capability spend | **CLAIMED 2026-07-25 for cited units; PARTIAL as total Safe capability** | Verified privileged builtins require a **live matching-kind** token at the effect (`cap_acquire("kind")` → effect spends it); wrong kind / no token → `ANUBIS_EFFECT_UNAUTHORIZED`; double-spend → `ANUBIS_CAPABILITY_REUSE`; **ambient interproc caller-pays** (units `interproc_caller_pays_*`); fixtures `cap_causal_*` | Safe declaration-gated (`uses`); Cluster F inheritance closed for its mechanism — **other capability false accepts remain** (CLAIMS open §1); map/struct-field linear-closure residual |
| Non-exportable linear capabilities (shared visitor + store-then-project + interproc container stores + peel-of-param + deep HO linear closures) | **CLAIMED 2026-07-25** | Local mint + export sinks → `ANUBIS_CAPABILITY_EXPORT`; causal spend without token-as-arg OK; `cap_export` peels; **interproc** formals + headers; **closure capture** export-seal; **store-then-project** + **interproc formal-container mutation**; **peel-of-param**; **deep HO linear closures** (named + **list containers** `arr[0](…)`; free Live caps MOVE into binding/container; double apply / use-after-move → `ANUBIS_CAPABILITY_REUSE`; units `linear_closure_*`); dual matrix; `cargo test -p anubis-compiler --lib middle::capability::tests` | Residual: map/struct-field linear-closure containers |
| Keychain / Secure Enclave bind for NE caps (macOS) | **CLAIMED 2026-07-25 (signed Keychain path)** | Soft: `__anubis_cap_ne_soft:…` (`ANUBIS_KEYCHAIN_CAPS=0`); Keychain: `__anubis_cap_ne_kc:…` via Security.framework; SE: `__anubis_cap_ne_se:…` when `ANUBIS_KEYCHAIN_SE=1` and hardware allows; `keychain_se_probe` 0/1/2; **signed path** `compile_sign_and_run_source` (codesign with Apple Development or ad-hoc + safe CLI entitlements, no restricted SE key that AMFI-kills); unit `keychain_se_signed_run_binds_keychain` requires `kc:`/`se:` under Development identity; gate `bash scripts/run_keychain_se_gate.sh`; entitlement derive for packaging profiles | **Permanent residual / not claimed:** a signed CLI Keychain bind and optional SE handle do not establish hardware-isolated, nonexportable storage. Restricted-SE provisioning, hardware-isolation evidence, notarized/App Store packaging, and zkVM SE binding remain outside the claim |
| Native CDCL Unsat RUP certificate | **re-run 2026-07-25:** `cargo test -p anubis-solver lrat` → **16 passed**; `check_proof` required for every `NativeVerdict::Unsat` | Pure independent RUP; division deferred |
| Native solver as compiler **default** (no env) | **CLAIMED 2026-07-25** | `native_authoritative()` default ON; soak `out/native_default_flip_soak_20260725/`; decision `out/native_default_flip_seal_20260725/DECISION.md`; gate PASS post-flip |
| Native authoritative **var×var mul** | **CLAIMED 2026-07-25** | `mulVar_correct` in BitBlast.lean + schoolbook `blast.rs::var_mul` + fragment admits; `run_native_authoritative_gate.sh` PASS |
| Native authoritative **division** (`bvsdiv`/`bvsrem`) | **partial CLAIMED 2026-07-25** | **Const/const** `/` `%` fold to a single `(_ bv… 64)` (native-authoritative, matches wrapping_div); nonneg + power-of-two → proven `bvlshr`/`bvand`; general free/signed non-pow2 still deferred (native declines; z3 may decide) |
| check → confine → run vertical | **re-run 2026-07-25:** `bash scripts/run_check_confine_run_gate.sh` | Net-free showcase; applied confinement + Safe run |
| Evidence bundle + tamper detection | package gate path `scripts/run_package_gate.sh` (seal history); unit evidence/tamper tests | Re-run package gate for live CI claims |
| RISC0 receipt path (in-process) | prove/verify path + A15 gate history; shape + `Receipt::verify` API | Hosted Metal proving **not claimed** |
| Metal parity (local Apple Silicon) | local Tier-2 parity history in A15 / doctor | Not hosted GPU prove |
| Language core (fixtures + repro) | **258/258** on pinned instrument; `scripts/run_language_fixtures.sh` | Seal must set `ANUBIS_BIN` to same binary as security (CLAIMS §7); default is still DEBUG `cargo run` |
| Backend portability / doctor / CLI | `anubis doctor`; DX gate history 15/15 | — |
| Ordinary `anubis run` Safe subset | SPEC_1_0 frozen surface; e.g. hello fixtures; vault contacts `run` EXIT=0 post-PTAH | Research/exploit needs `--allow-research` + VZ where required; **proof/shell constructs are non-run by design** (CLAIMS open §2 (B)); (R) preflight false-rejects **closed**; *check ≠ run for proof/shell* is a named product residual, not a checker gap |
| Phases 0–10 "DONE / At DoD" as total soundness | **not claimed as current** | Historical narrative in `docs/language/ROADMAP.md` | **Named residual:** published reds empty ≠ Class D / D1–D6 closed; green board is not COMPLETE |
| Program-wide mode aggregation + explicit Safe enclaves | **under Command 2026-07-25** + research auth bypass closed 2026-07-27 | Highest privilege wins; bare `@research` **without authorization REJECTS** (`e6ebfd2`). Lean lattice + Rust tests |
| Honest automatic rejection evidence | **under Command 2026-07-25:** `cargo test -p anubis --test safe_mode_program_gate` | Failed `check` auto-emits and `build --evidence` emits artifact-free `FAIL` bundles; PCA tier is `rejected`, not a proof claim |
| Runtime planning (probe) | plan surfaces exist (`runtime-plan`); **plan-only** | Plan-observed exec enforcement **deferred** |
| In-repo package / PCA ecosystem | package gate history; `import` + evidence deps | Public package registry **not claimed** |
| Third-party / multi-party reproduction | Phase 9 witness docs: [`phase9_independent_witness/`](language/phase9_independent_witness/) | Two recorded strangers + hashes; not infinite multi-party |
| DDC toolchain diversity | DDC gate history 34/34 + Phase 9 hashes | Residual: same-author C sources (not TT-total) |
| GitHub hosted witness | `scripts/audit_unified.sh --profile hosted` → 28 passing host-verifiable gates plus exactly `G9=EXTERNAL`, G14 non-executing host isolation witness, verdict `HOSTED_PASS` | Not a full seal. Only a separately approved operator-run disposable Tart/VZ lane may claim G9 PoC execution and the full G14 34-check battery; no persistent public-repository runner is authorized |
| A+ front door (2026-07-24 A15 re-seal) | **sealed:** `out/a_plus_a15_frontdoor_20260724-154145/gate_report.json` → pass=15 fail=0 skip=0; G14 VZ **34/34** tart guest | Re-run `bash scripts/audit_a_plus.sh` for a new seal date |
| A+ hostile audit package | **sealed:** `implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` + STEP_STATUS | Independent of freestanding maturity adjectives |
| Lean formal core | **lead-verified:** `bash scripts/run_formal_gate.sh` → `FORMAL_GATE: PASS`; every theorem machine-checked; no sorry/admit/axiom in core | Lean 4.32.0; no Mathlib; **162 theorems / 15 modules** (comment-stripped) |
| Pure-Anubis formal SAT kernel demo | **re-run 2026-07-25:** `bash scripts/run_formal_kernel_gate.sh` → `FORMAL_KERNEL_GATE: PASS` (kernel + hard tests + independent Python oracle 12/12) | Demo / education surface; not the production native SMT (`solver/`) |
| `http_get` / `http_post` native `run` | **re-run 2026-07-25:** `cargo test -p anubis-compiler http_` → 3 passed | Cleartext TCP; HTTPS via host `curl` (system TLS TCB) |
| VZ slice-2 apply (tart args + applied artifact) | **re-run 2026-07-25:** `bash scripts/run_vz_apply_gate.sh` → `VZ_APPLY_GATE: PASS` | Applied schema separate from sealed `anubis.confinement.v1` |
| VZ apply mount posture fail-closed | **CLAIMED 2026-07-25** | Engagement `--dir` filtered by proven mount posture: `none` → `ANUBIS_APPLY_MOUNT_DENIED`; `read-only` forces `:ro`; unit + gate mount-deny | Residual: live tart boot not required for gate |
| VZ apply network fail-closed (hostname staged) | **CLAIMED 2026-07-25** | Dual of mounts: host-only refuses `--allow-host`/`--allow-open-nat`; `net.send` defaults to host-only (not open NAT); `--allow-host` DNS-pins + records; `--allow-open-nat` explicit residual; gate net-deny | Superseded for Softnet path by row below when softnet on PATH |
| VZ apply Softnet CIDR from DNS-pinned allow-host | **CLAIMED 2026-07-25** | With `softnet` on PATH: `--allow-host` → tart `--net-softnet-block=0.0.0.0/0` + `--net-softnet-allow=<ip>/32`; mode `hostname-softnet`; without softnet → `hostname-policy-staged` host-only fallback; applied field `dns_pin_residual=rebind_after_pin` + HARD RESIDUAL notes; unit `cargo test -p anubis softnet_dns_pin` + `vz_apply` | **HARD residual sealed:** Softnet `/32` is apply-time DNS pin only — post-pin DNS rebind not enforced (not L7). Re-`vz apply` after DNS change. Not Keychain; live tart boot not in gate |
| Effect-derived entitlement / sandbox profile | **CLAIMED 2026-07-25** | `anubis entitlements <file.anb>`; `package::entitlements` derive + seal `entitlement_profile.json` + `program.entitlements` plist; re-derive on PCA verify (`ANUBIS_ENTITLEMENT_DRIFT`); when source uses `cap_acquire_nonexportable`, derives `keychain-access-groups` + `com.apple.developer.secure-enclave` (still `apple_enforced_claim: false`); unit `nonexportable_cap_derives_keychain_and_se_keys` | Residual: OS enforcement only after codesign; path-level sandbox rules **not claimed** |
| Hostname egress policy (DNS pin / deny-all) | **re-run 2026-07-25:** `cargo test -p anubis vz_egress` → pass | Policy compiled; live fd pump at native-boot |
| Require-Metal prove (operator-run local Apple Silicon) | **historical re-run 2026-07-25:** `ANUBIS_REQUIRE_METAL=1 bash scripts/run_metal_prove_gate.sh` → **METAL_PROVE_GATE: PASS** (Gate11 overall_verdict=PASS, metal-hybrid) | Not a hosted-CI claim. A source-current release receipt requires a separately approved operator-run rerun; no self-hosted public-repository runner is authorized |
| VZ native-boot + egress pump | **landed** `anubis vz native-boot --kernel …` | Needs signed binary + bootable kernel; pump enforces DNS-pinned policy |
| Author-diversity architecture lane | **re-run 2026-07-25:** `bash scripts/run_author_diversity_gate.sh` → PASS | TT-total **not claimed** (same-human residual) |
| Hosted CI Metal **proving** | **not claimed** | Needs Apple Silicon GPU runners |
| “Production-grade” / industry-ready blanket | **not claimed** as a freestanding stamp | 1.0 freeze is scoped (SPEC_1_0 + showcases); residuals in freeze §5 |
| General-purpose language (all features forever) | **partial** | 1.0 freeze scoped; residuals in SPEC_1_0 §5 |

### Session proof log (2026-07-24)

Recorded under `out/never_oversell_prove_20260724/`:

```text
python3 tools/host_exec_guard.py   # allow exit 0; malware/destructive denylist exit 2
cargo test -p anubis-solver lrat   # 16 passed; 0 failed (re-run 2026-07-25)
bash scripts/run_native_authoritative_gate.sh
  # NATIVE_AUTHORITATIVE cert suite: PASS
  # equivalence 539 files mismatches=0 disagreements=0
  # NATIVE_AUTHORITATIVE_GATE: PASS
bash scripts/run_formal_gate.sh    # FORMAL_GATE: PASS
jq gate_report.json                # pass=15 fail=0 (A15 frontdoor seal on disk)
```

## Independent reproduction (Phase 9)

| Party | Commit | Selfhost | Repro | DDC |
|-------|--------|----------|-------|-----|
| Stranger 1 | `4b19c48` / witness set | 9/9 | 6/6 | 34/34 |
| Stranger 2 | `7c5bf06` | 9/9 | 6/6 | 34/34 |

Agreed hashes (Phase 9 witness date only): binary fixpoint `9030e24b…`, macOS repro `c94fd5b1…`, Linux hermetic `6211f8c9…`, DDC output `3830edc6…`.  
**The post-2026-07-26 registry host value was re-sealed in VM on 2026-07-29:** 22/22 gates, fixpoint `46ddce14…ba60` matching `scripts/vm/EXPECTED_FIXPOINT_VM`; see the dated status rows above. The Phase 9 hashes remain evidence for their witness date only and are not the current fixpoint.  
See [`language/phase9_independent_witness/WITNESS.md`](language/phase9_independent_witness/WITNESS.md) and [`WITNESS_2.md`](language/phase9_independent_witness/WITNESS_2.md).

### Essence spine (identity re-check)

```bash
bash scripts/run_essence_spine_gate.sh          # full (incl. native + formal)
ESSENCE_SPINE_FAST=1 bash scripts/run_essence_spine_gate.sh   # flagships + IFC only
```

**2026-07-25:** secret-PC + secret→public (incl. method formals); **Verified causal capability spend** at privileged effects. Safe = declaration-gated; Verified = live matching-kind token at use.

## Forbidden overclaims

- Freestanding **REAL** / “production-grade” / “fully proven” without a re-runnable command on the same claim
- “Trusting-trust closed” / “backdoor-free”
- “Hosted Metal proving”
- “Public package registry”
- Native solver **default flip residual** (closed 2026-07-25 — default-authoritative; not listed as open)
- Infinite multi-party coverage beyond recorded witnesses
