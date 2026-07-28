# Anubis Claims (1.0 freeze — evidence-first)

See `MATURITY_CLAIM_MATRIX.md` for historical gate rows. Living freeze:
[`docs/language/SPEC_1_0_FREEZE.md`](language/SPEC_1_0_FREEZE.md) ·
[`docs/language/SEMVER_1_0_POLICY.md`](language/SEMVER_1_0_POLICY.md).

## Known open issues (2026-07-27)

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

### Currently green (re-stamped 2026-07-27 GROK-MAAT round 8 — not a total-soundness claim)

**Tip commits:** `ec65724` (unknown attr fail-closed) · `e6ebfd2` (research auth bypass) ·
`c9415b7` (D4) · `f9fc7a7` (D1/D2/D3). Live instrument: `./target/release/anubis` (mtime
2026-07-27 02:14 this pass).

| Surface | Observation | Repro / boundary |
|---|---|---|
| **Security fixtures** | Lead gate **317/317 PASS**. Live disk inventory **317** `.anb`; **published red list EMPTY** (0 `EXPECT: FAIL` still check-PASS this pass) | Green ≠ no bugs. Re-enumerate command below. |
| **Language core** | **244/244 PASS** — but see the float-lane residual below; measured 243/244 twice on 2026-07-28 | pin `ANUBIS_BIN` (§6) |
| **Stdlib fail-closed** | **104/104 PASS** | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/…` |
| **Capset selfhost** | **5/5 PASS** | `bash scripts/run_capset_selfhost_gate.sh` |
| **Taint / type / effect selfhost** | **0 disagreements** each | lead-verified |
| **Formal gate** | **PASS** — every theorem machine-checked; **no `sorry` / `admit` / free `axiom`** | `bash scripts/run_formal_gate.sh`; Lean **162 theorems / 15 modules** (comment-stripped) |
| **Native authoritative** | **PASS over 888 files, 0 mismatches** | `bash scripts/run_native_authoritative_gate.sh` |
| **Unified gate suite** | **22/22 PASS** at commit `4e7ee94` — 0 failed, 0 skipped, 0 external, `tree_state: clean` | `bash scripts/audit_head.sh --rev <sha>` — grades a COMMIT in a throwaway worktree, not the live tree |
| Research elevation | Bare `@research` **without** authorization → REJECT | Live: `research_block_without_authorization_rejects.anb` EXIT=1 |
| Unknown attributes | **Fail closed** | Live: `unknown_attribute_rejects.anb` EXIT=1 |
| Ordinary Safe `run` | Vault contacts EXIT=0 post-PTAH | Proof/shell non-run by design (§2 B) |
| VM seal of post-registry fixpoint | **Pending** | Do not publish host fixpoint as sealed |

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
   position) with one carrier still open — see item 7. Green board does not invent completeness.

2. **check/run divergence — (R) CLOSED; (B) residual named.**  
   **(A)=0, (B)=7, (R)=3**; (R)+PCA **CLOSED**. **(B)** non-run by design — do not equate
   `check` PASS with ordinary `run` for shell/symbolic.

3. **Self-host registry — HOST-FIXED; VM seal pending.**  
   Do not publish post-drift host fixpoint as sealed.

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
    accept→reject flips among the 308 that predate it; language 244/244; compiler lib 731/731. Commit
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
    resolver has no equivalent. That is being tested rather than assumed. One chain, c05
    (map→struct→push→field), remains ACCEPT and may fall to row 8.

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

    **Measured state (R20 — re-derived, not assumed):**
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

17. **`build` and `run` disagree on research CONSENT — mechanism gap, outcome currently agrees
    (2026-07-28).**

    `anubis run` requires an explicit `--allow-research` and refuses otherwise
    (`ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW`). `anubis build` has no such flag and INFERS the same
    permission: `ir.has_research || (ir.mode != Safe && !ir.taint_labels.is_empty())`.

    The second disjunct is **dead code** under current typecheck semantics — `has_research` is
    already set for any function with `mode != Safe`, so it never changes the outcome. Both paths
    therefore agree today, and the finding is LOW severity.

    It is recorded anyway because it is a live maintenance hazard: if `has_research` is ever narrowed
    to track only `@research` blocks, that dead disjunct silently reactivates and auto-enables the
    research lane — `target_run`, host command execution — in a BUILT binary for programs the `run`
    path would have refused, with the user never passing a consent flag at any point. The comment at
    the site says the two paths should match; the mechanism is inference on one side and explicit
    consent on the other.

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
    language **244/244**, `cargo test --release -p anubis` **200 passed / 0 failed** — so the fix
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

19. **The purple report makes FALSE ATT&CK coverage claims — OPEN, operator-facing (2026-07-28).**

    `attck.rs` holds a catalog of 20 techniques, each carrying an `aop_surface` string
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

    Found by an agent scoring its own pre-registered prediction **1 of 3** and reporting both
    refutations plainly: `persistence.rs` has no dispatch surface at all, and `malleable.rs` is a
    typed struct — though its `transform` field turns out to be validated-but-never-read, with the
    listener hard-coding its headers independently. A different disease, recorded here so it is not
    lost.

### The carrier class — judged EXHAUSTED as a callee-identity class (2026-07-27)

After 19 surfaces audited, two rounds of pre-registered predictions scored, and four of five leaking
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

Boundary item 4 has said "host-forgeable markers; operator is trust root" as an assertion. It was
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

### Carrier class: 40 of 41 CLOSED — the one residual, named precisely (2026-07-28)

The 41 published-open carrier routes stand at **40 CLOSED, 1 open**, with 25 must-stay-ACCEPT pure
guards passing and zero over-rejection, security 317/317. The element-materialization ladder is
25/25.

**The residual is `rec_build_then_app`, and it is a summary-shape problem, not a missing rule:**

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

### The callable false-accept class is WIDER than the ten shapes — 41 open routes (OPEN 2026-07-28)

Ten carrier shapes were closed on 2026-07-28, each with a rejecting discriminator AND a guard that
rejects poison. Red inventory zero, both held fixtures rejecting, security 317/317. On that
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
than half-fixed by the person who found it while the type lane's owner is mid-repair elsewhere.

Fixtures: `scratchpad/fleet_20260726/w19/generics/g1..g4`.

### Open — boundary honesty / process (not silent overclaims)

4. **VZ isolation is SAFETY, not SECURITY** — host-forgeable markers; operator is trust root.  
5. **Research elevation requires authorization** — bypass **CLOSED** (`e6ebfd2`); dual-use
   research remains intentional with explicit authorization, not a Safe free ride.  
6. **Harness integrity + instrument fact — the two NAMED defects are closed; the CLASS is NOT
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

   Binaries are now published as content-addressed read-only pins (`scripts/publish_pin.sh`) so a
   rebuild cannot mutate the instrument an agent is mid-measurement on.

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

   **Instrument provenance is the most common defect:** nine gates grade whatever sits at
   `target/release/anubis` with no freshness or digest check, bypassing the pin mechanism above.

   Not claimed closed. The fix that would close the class is a shared coverage assertion in
   `gate_common.sh` that every gate calls with its own counter, enforced by the adoption check —
   patching the four individually is how this class survived to the audit that found it.

7. **Function-identity carrier — CLOSED 2026-07-27 (`0eb5977`).** A function reference reaching an
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
   an unrunnable program — and separately a footgun, tracked as item 8.

8. **`push` expression-position return — CLOSED 2026-07-27.** `let ys = push(xs, 3); len(ys)` had
   `check` rc=0 and `run` rc=1 (panic): the expression lowering returned `AnubisValue::Int(0)`, a
   placeholder, so functional-style use silently bound a non-container. It now returns the container,
   matching its siblings (`pop`, `insert`, `remove` all return something meaningful). Statement-position
   `push(xs, v)` is lowered separately and unaffected; both are pinned by
   `push_expression_returns_container_doc_ok.anb`. Opened and closed the same day.

### Resolved this arc (do not re-open without new evidence)

7. ~~Published security red inventory (the eleven + D/research witnesses in corpus)~~ **EMPTY**
   this pass — live zero known-red.  
8. ~~D1/D2/D3 field qualifier through CALL result~~ **RESOLVED** `f9fc7a7`.  
9. ~~D4 enum-payload qualifier at match binder~~ **RESOLVED** `c9415b7`.  
10. ~~Research-block authorization bypass~~ **RESOLVED** `e6ebfd2` — bare `@research` no longer
    elevates Safe.  
11. ~~Unknown attributes silently ignored~~ **RESOLVED** `ec65724` fail-closed.  
12. ~~Four mechanisms for the original eleven~~ **RESOLVED** (M1, M2-reg, M2-B, M3).  
13. ~~Declared returns / R1 fields / (R)+PCA / stdlib 45/45 / capset 5/5~~ **RESOLVED**.  
14. ~~P0 mul hang / Tier 1 / self-host harness~~ **RESOLVED**.

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
| Safe taint enforcement | security **317/317** (lead) / red list empty live; D1–D4 closed; taint selfhost **0 disagreements** | **PARTIAL as total** — green = **no KNOWN defects**, not no defects. Stdlib **104/104** |
| Declassification policy | declassify accept/reject fixture pairs under `tests/fixtures` / security fixtures | Lab policy surface, not a full IFC type system; shell declassify accept is check-policy only (`run` non-run by design — CLAIMS open §2) |
| Solver correctness (supported int fragment) | **lead-verified:** `bash scripts/run_native_authoritative_gate.sh` → **PASS, 882 files, 0 mismatches** | Division deferred; var×var mul claimed; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` |
| Wrap-safety VCs (AoRTE-lite) + CEX possible fix | **CLAIMED 2026-07-25; free×free closed 2026-07-25** | On modelable ints: auto wrap-safety for `+`/`-`, **var×const `*`**, and **free×free `*`** via **offline interval product** (no SMT smul hang): bounded factors → prove; unbounded → `ANUBIS_WRAP_RISK` + possible fix; opt-out `ANUBIS_WRAP_SAFETY=0`; unit `cargo test -p anubis-compiler --lib wrap_safety` → 6+; see [`SPARK_VS_ANUBIS.md`](SPARK_VS_ANUBIS.md) | Residual: free `ensures(result == x*y)` posts can still be slow under native-authoritative (separate from wrap-safety); compound factors only offline-proved for simple `bvadd`/`bvsub`/const/var shapes |
| Implicit secret→public (PC) + explicit secret→public (Safe) | **CLAIMED 2026-07-25 for cited fixtures; PARTIAL as total IFC** | Method formals + declared returns + R1 + D1–D4 call/match places; **security 311/311** lead / red list empty | Residual: full PC-join; composition shapes may remain (D5/D6 family) |
| Symbolic-index secret-capturing closure application | **CLAIMED 2026-07-25** | `arr[idx](…)` with non-literal `idx` fail-closed when container holds secret/taint-capturing element (j1 twin of `let g = arr[i]`); unit `symbolic_index_secret_capturing_list_application_fails_closed`; clean symbolic still accepts | Residual: full PC-join; untyped formals still interproc |
| Nested container closure application (`outer[0][0]`, `b.fs[i]`, bind + mid-bind) | **CLAIMED 2026-07-25** | Nested Index/FieldAccess CallExpr + **bind** (`let g = outer[i][0]; g(0)`) + **intermediate mid-bind** (`let mid = outer[0]; mid[0](0)` re-keys `field_closures`; symbolic mid union-projects first segments fail-closed); unit `nested_container_closure_application_fails_closed` (apply + bind + mid lit/sym/clean); clean nested still accepts | Residual: full PC-join not claimed |
| if-expr-built containers seed `field_closures` (incl. nested `Stmt::If` + let-inner) | **CLAIMED 2026-07-25** | `collect_container_closures` walks `Expr::If`/`Match`/`Block`; nested bare `if` as `Stmt::If`; unit `nested_container_closure_application_fails_closed` | Residual: full PC-join not claimed |
| `push`/`insert` seed capturing lambdas into `field_closures` (free + method) | **CLAIMED 2026-07-25** | `apply_container_mutation_taint` seeds pushed/inserted lambdas; concrete path miss fail-closes via `any_capturing_field_closure`; free `push(arr, lam)` + method `arr.push(lam)`; unit cases in `nested_container_closure_application_fails_closed` | Residual: full PC-join; HO rebind beyond push/insert seed |
| Verified causal capability spend | **CLAIMED 2026-07-25 for cited units; PARTIAL as total Safe capability** | Verified privileged builtins require a **live matching-kind** token at the effect (`cap_acquire("kind")` → effect spends it); wrong kind / no token → `ANUBIS_EFFECT_UNAUTHORIZED`; double-spend → `ANUBIS_CAPABILITY_REUSE`; **ambient interproc caller-pays** (units `interproc_caller_pays_*`); fixtures `cap_causal_*` | Safe declaration-gated (`uses`); Cluster F inheritance closed for its mechanism — **other capability false accepts remain** (CLAIMS open §1); map/struct-field linear-closure residual |
| Non-exportable linear capabilities (shared visitor + store-then-project + interproc container stores + peel-of-param + deep HO linear closures) | **CLAIMED 2026-07-25** | Local mint + export sinks → `ANUBIS_CAPABILITY_EXPORT`; causal spend without token-as-arg OK; `cap_export` peels; **interproc** formals + headers; **closure capture** export-seal; **store-then-project** + **interproc formal-container mutation**; **peel-of-param**; **deep HO linear closures** (named + **list containers** `arr[0](…)`; free Live caps MOVE into binding/container; double apply / use-after-move → `ANUBIS_CAPABILITY_REUSE`; units `linear_closure_*`); dual matrix; `cargo test -p anubis-compiler --lib middle::capability::tests` | Residual: map/struct-field linear-closure containers |
| Keychain / Secure Enclave bind for NE caps (macOS) | **CLAIMED 2026-07-25 (signed Keychain path)** | Soft: `__anubis_cap_ne_soft:…` (`ANUBIS_KEYCHAIN_CAPS=0`); Keychain: `__anubis_cap_ne_kc:…` via Security.framework; SE: `__anubis_cap_ne_se:…` when `ANUBIS_KEYCHAIN_SE=1` and hardware allows; `keychain_se_probe` 0/1/2; **signed path** `compile_sign_and_run_source` (codesign with Apple Development or ad-hoc + safe CLI entitlements, no restricted SE key that AMFI-kills); unit `keychain_se_signed_run_binds_keychain` requires `kc:`/`se:` under Development identity; gate `bash scripts/run_keychain_se_gate.sh`; entitlement derive for packaging profiles | Residual: **App Store / notarized .app packaging**; restricted `com.apple.developer.secure-enclave` provisioning UX (CLI signed path omits that key deliberately); zkVM guest soft-only |
| Native CDCL Unsat RUP certificate | **re-run 2026-07-25:** `cargo test -p anubis-solver lrat` → **16 passed**; `check_proof` required for every `NativeVerdict::Unsat` | Pure independent RUP; division deferred |
| Native solver as compiler **default** (no env) | **CLAIMED 2026-07-25** | `native_authoritative()` default ON; soak `out/native_default_flip_soak_20260725/`; decision `out/native_default_flip_seal_20260725/DECISION.md`; gate PASS post-flip |
| Native authoritative **var×var mul** | **CLAIMED 2026-07-25** | `mulVar_correct` in BitBlast.lean + schoolbook `blast.rs::var_mul` + fragment admits; `run_native_authoritative_gate.sh` PASS |
| Native authoritative **division** (`bvsdiv`/`bvsrem`) | **partial CLAIMED 2026-07-25** | **Const/const** `/` `%` fold to a single `(_ bv… 64)` (native-authoritative, matches wrapping_div); nonneg + power-of-two → proven `bvlshr`/`bvand`; general free/signed non-pow2 still deferred (native declines; z3 may decide) |
| check → confine → run vertical | **re-run 2026-07-25:** `bash scripts/run_check_confine_run_gate.sh` | Net-free showcase; applied confinement + Safe run |
| Evidence bundle + tamper detection | package gate path `scripts/run_package_gate.sh` (seal history); unit evidence/tamper tests | Re-run package gate for live CI claims |
| RISC0 receipt path (in-process) | prove/verify path + A15 gate history; shape + `Receipt::verify` API | Hosted Metal proving **not claimed** |
| Metal parity (local Apple Silicon) | local Tier-2 parity history in A15 / doctor | Not hosted GPU prove |
| Language core (fixtures + repro) | **244/244** on pinned instrument; `scripts/run_language_fixtures.sh` | Seal must set `ANUBIS_BIN` to same binary as security (CLAIMS §7); default is still DEBUG `cargo run` |
| Backend portability / doctor / CLI | `anubis doctor`; DX gate history 15/15 | — |
| Ordinary `anubis run` Safe subset | SPEC_1_0 frozen surface; e.g. hello fixtures; vault contacts `run` EXIT=0 post-PTAH | Research/exploit needs `--allow-research` + VZ where required; **proof/shell constructs are non-run by design** (CLAIMS open §2 (B)); (R) preflight false-rejects **closed**; *check ≠ run for proof/shell* is a named product residual, not a checker gap |
| Phases 0–10 "DONE / At DoD" as total soundness | **not claimed as current** | Historical narrative in `docs/language/ROADMAP.md` | **Named residual:** published reds empty ≠ Class D / D1–D6 closed; green board is not COMPLETE |
| Program-wide mode aggregation + explicit Safe enclaves | **under Command 2026-07-25** + research auth bypass closed 2026-07-27 | Highest privilege wins; bare `@research` **without authorization REJECTS** (`e6ebfd2`). Lean lattice + Rust tests |
| Honest automatic rejection evidence | **under Command 2026-07-25:** `cargo test -p anubis --test safe_mode_program_gate` | Failed `check` auto-emits and `build --evidence` emits artifact-free `FAIL` bundles; PCA tier is `rejected`, not a proof claim |
| Runtime planning (probe) | plan surfaces exist (`runtime-plan`); **plan-only** | Plan-observed exec enforcement **deferred** |
| In-repo package / PCA ecosystem | package gate history; `import` + evidence deps | Public package registry **not claimed** |
| Third-party / multi-party reproduction | Phase 9 witness docs: [`phase9_independent_witness/`](language/phase9_independent_witness/) | Two recorded strangers + hashes; not infinite multi-party |
| DDC toolchain diversity | DDC gate history 34/34 + Phase 9 hashes | Residual: same-author C sources (not TT-total) |
| GitHub hosted witness | `scripts/audit_unified.sh --profile hosted` → 14 host-verifiable gates plus `G9=EXTERNAL`, G14 non-executing host isolation witness, verdict `HOSTED_PASS` | Not a full seal. Only default `audit_a_plus.sh` on the dedicated Tart/VZ runner may claim G9 PoC execution and the full G14 34-check battery |
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
| Hosted Metal prove (local AS + self-hosted job) | **re-run 2026-07-25:** `ANUBIS_REQUIRE_METAL=1 bash scripts/run_metal_prove_gate.sh` → **METAL_PROVE_GATE: PASS** (Gate11 overall_verdict=PASS, metal-hybrid) | Stock GHA still cold-verify; hosted claim needs self-hosted Metal runner labels |
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
**Post-2026-07-26 registry work deliberately re-baselined the self-host binary; that new host value is unsealed — do not cite it as a seal.** Re-seal under VM before any new public fixpoint claim.  
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
