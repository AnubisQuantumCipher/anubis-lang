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
| **Security fixtures** | Lead gate **311/311 PASS**. Live disk inventory **311** `.anb`; **published red list EMPTY** (0 `EXPECT: FAIL` still check-PASS this pass) | Green ≠ no bugs. Re-enumerate command below. |
| **Language core** | **244/244 PASS** | pin `ANUBIS_BIN` (§6) |
| **Stdlib fail-closed** | **104/104 PASS** | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/…` |
| **Capset selfhost** | **5/5 PASS** | `bash scripts/run_capset_selfhost_gate.sh` |
| **Taint / type / effect selfhost** | **0 disagreements** each | lead-verified |
| **Formal gate** | **PASS** — every theorem machine-checked; **no `sorry` / `admit` / free `axiom`** | `bash scripts/run_formal_gate.sh`; Lean **162 theorems / 15 modules** (comment-stripped) |
| **Native authoritative** | **PASS over 882 files, 0 mismatches** | `bash scripts/run_native_authoritative_gate.sh` |
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

    Verdict-diff on the pinned binary: security 311/311; language 244/244. Zero accept→reject flips.

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

    Verdict-diff on the pinned binary: security 311/311; language 244/244. Zero accept→reject flips.

12. **The bare-builtin carrier defeats the LETHAL TRIFECTA detector — CLOSED (2026-07-27).**

    Fixed by making builtin identity a TAG carried on the value rather than a name read at the call
    node. Tags (`TaintSource`, `SecretSource`, `EgressSink`, `IntegritySink`, `Capability(..)`) form
    a monoid; names are only the SEED; join is UNION, so a value that is `input` on one branch and
    pure on the other keeps the tag. `Known(∅)` proves a value carries no gate class and `Unknown`
    means unresolved — an unknown value is not assumed to violate, so a builtin bound but never
    applied still ACCEPTS. Measured: security 311/311 with the held RED now rejecting and zero
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

16. **The dual-use surface is 91% UNPROBED — stated, not implied (2026-07-28).**

    Three rounds on that lane characterized one narrow vertical: `emit_builtin_call` →
    `var_as_value` → carrier immunity → MODE-as-carrier → the item-15 barrier. That vertical is
    well-understood. **Everything else is not**, and the measured figure is ~24,200 of ~26,700 lines
    (`tools/anubis/src/offensive/` alone is 28 files / 9,964 lines, most of it untouched:
    `listener.rs`, `engagement.rs`, `scope.rs`, `isolation.rs`, `receipts.rs`, `domain_packs.rs`,
    `dns_codec.rs`).

    Written down because "unprobed" recorded beats "probed" assumed. A green board on a lane where
    91% was never examined is a statement about the 9%.

    **Predicted class for the remainder, from the agent that probed the 9%: NAME-KEYED DISPATCH.**
    An enumeration in one place, a consumer in another, strings as the connecting key, no shared
    enum enforcing parity. That is precisely the shape of the C2 task-parser defect already found and
    fixed. Named candidates: ATT&CK technique IDs, persistence mechanism names, malleable profile
    fields. A prediction made before probing, so it can be scored.

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

18. **The tag lane has a standing DEFECT FACTORY — three shapes, still widening (2026-07-28).**

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

### Open — boundary honesty / process (not silent overclaims)

4. **VZ isolation is SAFETY, not SECURITY** — host-forgeable markers; operator is trust root.  
5. **Research elevation requires authorization** — bypass **CLOSED** (`e6ebfd2`); dual-use
   research remains intentional with explicit authorization, not a Safe free ride.  
6. **Harness integrity + instrument fact — CLOSED 2026-07-27.** Language fixtures defaulted
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
| Safe taint enforcement | security **311/311** (lead) / red list empty live; D1–D4 closed; taint selfhost **0 disagreements** | **PARTIAL as total** — green = **no KNOWN defects**, not no defects. Stdlib **104/104** |
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
