# Anubis — Completion Ledger (2026-07-28)

**Generated from a 9-agent read-only survey of the whole repo** (8 areas: roadmap phases, language
promise, compiler gaps, frontend/type system, stdlib/runtime, formal verification, solver lanes,
tooling/UX), then synthesized. 104 items. Every item carries `path:LINE`.

This file is a SURVEY RESULT, not a status authority. The authorities remain `docs/CLAIMS.md` and
`docs/language/ROADMAP.md`. Where this file and those disagree, they win and this gets corrected.

---

## Honest assessment

NOT close to complete against its own promise sentence — but much closer than that verdict sounds, because the gap is ONE policy decision, not a backlog.

The strongest evidence FOR: this is a real, deep artifact, and I verified its headline numbers myself rather than taking the docs' word. 311 security fixtures, 104 stdlib fail-closed fixtures, 162 Lean theorems across 15 modules with zero sorry/admit/axiom, a 213-builtin inventory that re-derives EXACTLY with zero names in either direction, a zero-dependency native SMT solver whose deferral set (Ashr/SignExtend/div/rem) is refused by a total match with no wildcard arm and covered by nested negative tests, and solver bounds with a theorem-shaped test proving they can only ever weaken to a deferral. Zero `todo!()` and zero `unimplemented!()` in the compiler. An independent stranger reproduced the fixpoint and the formal gate from a clean clone. That is not a paper language.

The strongest evidence AGAINST, and it is decisive: the promise says "everything it could not decide, it refused rather than assumed," and the checker's own source comments say the opposite in five places — "defers (fail-open, the documented residual)", "DEFER the whole block (fully fail-open)", and at mod.rs:273-274 as stated policy: "Default-lane policy deliberately defers Unknown; it never invents a gate." A deferral IS an accept. This is not a bug someone will find; it is the design, written down. And it starts in the LEXER: I read compiler/src/frontend/mod.rs:1577 and the final arm of the character dispatch is `_ => {}` — an unrecognized character is silently DELETED, no token, no diagnostic, and `check` then certifies a program that is not the program on disk. The front end assumes before any security analysis runs.

Three things sharpen this rather than soften it. First, there is one class that is not merely a deferral but an observable false accept: a user function carrying `uses(fs.write)` reaching an application site through `push` or a field place-assign is not charged, and the witness WRITES A FILE under a green check. That is the promise being false, not under-scoped. It is half-fixed in the uncommitted working tree right now. Second, the instruments are compromised in ways I reproduced by hand: the docs-drift gate's verdict reads only `$FAILS` while its stamps-checked count is decorative and its scan return code is assigned and never read; the shadow-diff gate that runs on EVERY VM slice has zero possible input (0 of 23 `ctx.emit` sites pass `true`); one offensive check records PASS in both branches of its if. I also found a fourth defect the 67-script audit missed — the drift scanner treats the substring "as of " as proof a stamp is historical, which is exactly how LANGUAGE.md's 86/86 survived green against 104 fixtures on disk. Third, the repo's own definition of done — an end-of-arc VM seal — has never been started, and the battery it would run is missing the formal, package and DX gates entirely, so even a successful seal would not cover the 162 theorems it advertises.

What makes me call this close anyway: the lead has already found the right shape for the hard part. Commit 5bb0b38 censused the value-flow walkers at 26, not the blueprint's 7, noticed the count is RISING because every parity fix spawns a twin, correctly rejected monoid unification as the wrong target, and identified the pattern that actually works — the one already proven in solver/src/fragment.rs, where a total match with no wildcard makes a new variant fail to COMPILE rather than ride through as authoritative. Transfer that to the shared Expr descent with lane hooks as required trait methods and the whack-a-mole ends structurally. That is a large but bounded, well-specified piece of work with a working precedent in the same repository.

So: the engineering is near-done and unusually honest with itself. The promise sentence is not. Either land items 1-3 and then seal, or scope the sentence to what the seal actually proves. Do not ship the current sentence over the current behavior — it is the one claim a stranger will quote back, and today the code contradicts it in its own comments.

---

## Critical path — each blocks the next

1. Land the in-flight mutation/param callable-carrier fix (uncommitted in
   compiler/src/middle/mod.rs apply_container_mutation_taint + analyze_stmts place-assign) with
   i01-i05 fixtures and a full-corpus verdict-diff showing 0 accept->reject flips. Until this
   lands there is a KNOWN live false accept: check PASS + a file actually written. Nothing
   downstream is worth sealing over it.

2. Make construct coverage TOTAL, not unified. Per the lead's own 5bb0b38 finding, the denominator
   is 26 walkers, not 7, and it is rising by design. Transfer the solver/src/fragment.rs
   pattern: one shared Expr descent with NO wildcard arm and lane hooks as REQUIRED trait
   methods, so the 27th carrier fails to COMPILE rather than silently deferring. Monoids stay
   separate. This is what ends the whack-a-mole; without it item 1 is the fourth patch of the
   same shape this session.

3. Fix the instruments before trusting any number they print. Three verified defects gate
   everything: run_docs_drift_gate.sh's verdict reads only $FAILS while STAMPS_CHECKED is
   decorative and SCAN_RC (assigned :113) is never read; the DATED_LINE 'as of ' substring in
   scripts/lib/docs_drift_scan.py:46 silently exempts any LIVE stamp (this is how
   LANGUAGE.md:546's 86/86 survived a green gate); run_shadow_diff.sh has zero possible input (0
   of 23 ctx.emit sites pass true) yet runs on every VM slice. A gate that cannot fail is not
   evidence.

4. Run the end-of-arc VM SEAL — full battery on a freshly pinned binary inside a tart guest via
   scripts/vm/run-slice.sh. HANDOFF section 10 defines done as exactly this and it has never
   been started. It must come AFTER 1-3: sealing now would stamp a board with a known-open
   false-accept class and instruments that print PASS without measuring.

5. Correct the promise sentence and the four docs that contradict the sealed board. The sentence as
   written ('everything it could not decide, it refused rather than assumed') is falsified BY
   DESIGN in the checker's own comments ('defers (fail-open, the documented residual)'). Either
   scope it honestly to what the seal proves, or do not ship it.

---

## Ranked work

### 1. Close the user-fn callable carrier via mutation and param (in flight, uncommitted)

**lane** `compiler` · **effort** MEDIUM

**Why:** The only class in the repo with a RUNTIME WITNESS: check PASS and a real file written by a
program whose main declares no such capability. Every other open item is a deferral; this one is
the promise being observably false. The fix is already half-written in the working tree
(seed_identities in apply_container_mutation_taint, field_fn_identities place-assign
retain/insert in analyze_stmts) but the three named axes are not all covered: mutation, extract
(pop/remove loses identity), inline identity composition, plus the param gap.

**Acceptance:** i01-i05 all reject under `anubis check`; the i02 witness no longer writes its file; three new
fixtures cover extract (pop/remove), inline identity composition in a list literal, and the
param path; full-corpus verdict-diff reports 0 accept->reject flips.

**Evidence:** git diff HEAD shows the partial fix live in compiler/src/middle/mod.rs (+37) and run.rs (+48),
uncommitted. docs/CLAIMS.md:499-516 'a USER-FN carrier class, OPEN (i01-i05, ACCEPT + file
written)' and 'the callable story is NOT converged while i02 writes a file under a green check'.
FnIdentitySet::union_present at mod.rs:238 is 'necessary but not sufficient' per CLAIMS.md:509.

### 2. Total construct coverage: one shared Expr descent with no wildcard arm, lane hooks as required trait methods

**lane** `compiler` · **effort** LARGE

**Why:** The durable fix, and the lead has already established its correct shape. The blueprint's '~7
parallel value-flow walkers' was a 4x undercount (measured: 26), and the count RISES with every
parity close because each fix spawns a twin walker rather than collapsing one. Four rounds of
findings this session -- tag join vs identity join, literal producer vs mutation producer,
return path vs param path -- are ONE shape: two walkers that should agree and do not. Monoid
unification is wrong (taint is boolean join-OR, tags/identities are set monoids with Unknown,
effects are rows, caps are linear). Construct-coverage unification is mandatory.

**Acceptance:** Adding a new variant to `enum Expr` produces a COMPILE ERROR in every lane that must handle it,
demonstrated by a deliberate throwaway variant on a scratch branch. No `_ =>` arm remains in the
shared descent. One `seed_element` API replaces the five hand-synced implementations.

**Evidence:** commit 5bb0b38 census: 23 primary propagators in middle/mod.rs + 3 sibling modules with their
own walk_expr + analyze_stmts as place-assign owner. The pattern to transfer already exists and
is proven: solver/src/fragment.rs is_proven_authoritative is a total match with no wildcard, so
a new Term variant fails to compile rather than riding as authoritative (verified: 0 fp/float
mentions in that file, and the deferral set Ashr/SignExtend/Udiv/Urem/Sdiv/Srem has negative
tests at fragment.rs:259-307).

### 3. Kill the fail-open default in the taint query walker (expr_taint_source_m ends in `_ => None`)

**lane** `compiler` · **effort** SMALL

**Why:** A query walker whose silent default is 'not tainted' is a fail-open default in the LABEL lane --
the exact defect class item 2 exists to make structurally impossible, sitting in the highest-
consequence lane. It is named in the repo's own most recent commit and not yet fixed. Small
enough to land ahead of the large refactor and it de-risks it.

**Acceptance:** mod.rs:21723's wildcard is replaced by an exhaustive match; a fixture whose taint source reaches
a sink through the previously-unhandled arm rejects; verdict-diff 0 accept->reject flips on the
311 security fixtures.

**Evidence:** commit 5bb0b38 body, verbatim: 'expr_taint_source_m ends in _ => None at mod.rs:21723 - a query
walker whose silent default is "not tainted", which is a fail-open default in the label lane.'

### 4. Repair the four verified gate-harness defects, including the undocumented `as of` drift bypass

**lane** `tooling` · **effort** MEDIUM

**Why:** Every phase number, every README badge, and the VM seal itself are gate outputs. I reproduced
three defects firsthand and found a FOURTH not in the 67-script audit's list: the DATED_LINE
regex treats the literal substring 'as of ' as proof a stamp is historical, so any LIVE numeric
stamp can be frozen forever by appending a date. That is precisely how LANGUAGE.md's 86/86
survived while the corpus is 104. This one line is why the drift gate green-lit a doc that is
wrong.

**Acceptance:** A deliberately corrupted stamp in LANGUAGE.md makes run_docs_drift_gate.sh exit non-zero; the
gate refuses when stamps_checked==0; killing the UDS listener makes t3_uds FAIL; `ls
scripts/run_*.sh | wc -l` vs the count adopting the shared fail-closed helper is published in
the seal report.

**Evidence:** VERIFIED: scripts/run_docs_drift_gate.sh:344-346 verdict tests only `[[ "$FAILS" -eq 0 ]]`;
STAMPS_CHECKED appears only in echo strings; SCAN_RC assigned :113 and never read again.
scripts/lib/docs_drift_scan.py:46 DATED_LINE includes `as of `, and :188 `if not stamp_dated and
rel in LIVE_STAMP_FILES` skips the check. scripts/run_offensive_platform_gate.sh:424 and :426
both `record "t3_uds" "PASS"`. LANGUAGE.md:546 still reads `(**86/86** as of 2026-07-27)`
against 104 fixtures on disk.

### 5. Delete or wire run_shadow_diff.sh — it runs on every VM slice and has no possible input

**lane** `tooling` · **effort** SMALL

**Why:** The ROADMAP mandates shadow-first promotion for every new check (ROADMAP.md:268-270) and run-
slice.sh:108 runs the gate every slice. The instrument behind that discipline cannot fail.
Worse, it will sit inside the end-of-arc VM seal contributing a fake green. Either restore the
shadow lane (route the bidirectional type checks through emit with shadow_gated=true as the doc
comments still claim) or remove the gate and the discipline's claim together. Do not seal with
it as-is.

**Acceptance:** Either (a) `ANUBIS_SHADOW:` lines appear on stderr for a fixture and run_shadow_diff.sh reports
a nonzero UNEXPECTED on a seeded disagreement, or (b) the gate is removed from scripts/vm/run-
slice.sh, the script is deleted, and ROADMAP.md:268-270's shadow-first claim is retracted or re-
pointed at a live instrument.

**Evidence:** VERIFIED by scripted scan: 23 `ctx.emit(` call sites in compiler/src/middle/mod.rs, ZERO pass
`true`, so shadow_diags (populated only at mod.rs:2513-2522 when shadow_gated==true) is never
populated and the `ANUBIS_SHADOW:` emitter at mod.rs:2995 is unreachable.
scripts/run_shadow_diff.sh:72 harvests exactly those lines. Stale doc comments claiming
otherwise at mod.rs:2504, :2510-2512 and middle/ty.rs:444.

### 6. The lexer silently DELETES any unrecognized character — refuse instead

**lane** `compiler` · **effort** SMALL

**Why:** The sharpest structural counterexample to the promise sentence in the entire compiler, and it is
a one-arm fix. The FRONT END assumes rather than refuses, before any security analysis runs: a
character the lexer does not match is dropped with no token and no diagnostic, and parse_source
only errors when diagnostics is non-empty — so `check` certifies a program that is not the
program on disk. Best value-per-line item on the board.

**Acceptance:** A .anb file containing an unmapped character makes `anubis check` exit non-zero with a spanned
ANUBIS_* diagnostic naming the character and its offset; a fixture locks it; the same fixture
shape with a stray token in expression position also rejects at check time, not only at run.

**Evidence:** VERIFIED: compiler/src/frontend/mod.rs:1577 — the final arm of the lex_spanned dispatch is `_ =>
{}` with no token pushed and no diagnostic. Error gate at compiler/src/frontend/mod.rs:4479-4491
keys on `output.diagnostics` being non-empty. Same class, parser half:
compiler/src/frontend/mod.rs:4017 emits `Expr::Other` with no diagnostic, treated as inert by
effects.rs:266, capability.rs:2498, trifecta.rs:276 and resolve/mod.rs:665, while only
run.rs:5097 refuses it.

### 7. End-of-arc VM SEAL on a freshly pinned binary (the repo's own definition of done)

**lane** `tooling` · **effort** MEDIUM

**Why:** HANDOFF section 10 defines completion as exactly this and it has NOT STARTED. It is ranked below
the four items above deliberately: a seal is only worth what the board underneath it is worth,
and sealing today would certify a known-live false accept plus three gates that print PASS
without measuring. Also note the battery itself is incomplete — the formal (Lean), package, and
DX gates are absent from run-slice.sh, so a seal today would not cover the 162 theorems it
advertises.

**Acceptance:** A clean tree, a fresh pin whose .meta binary hash matches HEAD's source, and a run-slice.sh
transcript from inside the tart guest showing every gate PASS — with formal, package and dx
added to the battery — archived under vm/exports/ and cited by hash in CLAIMS.md.

**Evidence:** docs/HANDOFF.md:271 'An end-of-arc VM seal — the full battery on a pinned binary inside a tart
guest via scripts/vm/run-slice.sh' + Not started. VERIFIED: scripts/vm/run-slice.sh:91-114 runs
14 gates — cargo-test, tool-test, clippy, build-rel, language, turing, security, stdlib, shadow,
seal, dogfood, effect-sh, capset-sh, type-sh, taint-sh — with NO run_formal_gate.sh, NO
run_package_gate.sh, NO run_dx_gate.sh. Working tree is dirty (mod.rs, run.rs, vm/pins/CURRENT)
so no current pin corresponds to committed source.

### 8. Scope the promise sentence to what the checker actually does, or make check refuse its unmodeled obligations

**lane** `docs` · **effort** MEDIUM

**Why:** The completion sentence is contradicted by the code's own comments in at least five places that
call the behavior fail-open by name. This is the one item where the honest fix might be the DOC,
not the code: a deferral to runtime enforcement is a defensible design, but it is not 'refused
rather than assumed'. Pick one. Shipping the current sentence over the current behavior is the
single largest overstatement in the repo, and it is the sentence a stranger will quote back.

**Acceptance:** docs/HANDOFF.md's sentence names the deferral boundary explicitly (which obligation classes are
proved vs runtime-enforced), OR `anubis check` exits non-zero on an undischarged obligation and
the corpus verdict-diff for that change is published. Not both green with the current wording.

**Evidence:** docs/HANDOFF.md:20-22 is the promise. Contradicted by compiler/src/middle/mod.rs:6789 'defers
(fail-open, the documented residual)'; mod.rs:7015 'DEFER the whole block (fully fail-open,
unchanged)'; mod.rs:6505-6507 returns `true` (discharged) whenever callee identity is not a
singleton; compiler/src/lib.rs:1338, :4630 same shape; README.md:358 states it as policy. Also
mod.rs:273-274 verbatim: 'Default-lane policy deliberately defers Unknown; it never invents a
gate.'

### 9. Float and string obligations enter the native-AUTHORITATIVE path with no Lean proof behind their lowering

**lane** `solver` · **effort** LARGE

**Why:** The headline TCB argument is 'authority bounded to machine-checked blasts'. That boundary is
crossed by two unproven source-to-source rewrites that happen at PARSE time, before the fragment
walker runs — so fragment.rs sees only proven BV tags and admits them. With z3 off PATH, a float
`ensures` can report PASS with detail 'machine-checked bit-blaster' while nothing machine-
checked the +/-0 and NaN key transform. Backing is 2000 random differential formulas, i.e. a
test, not a proof.

**Acceptance:** Either a Lean module proving the fp.rs monotonic-key transform (and the string interning domain
argument) is added to formal/Anubis/ and its theorem names are added to the drift list in
run_native_authoritative_gate.sh, OR is_proven_authoritative returns false for any formula whose
source sort was Float64/String — demonstrated by a float `ensures` falling back to z3 with the
correct detail string.

**Evidence:** VERIFIED: `grep -ci 'fp|float' solver/src/fragment.rs` = 0. solver/src/parse.rs:264-286 lowers
fp.lt/leq/gt/geq/eq and isNaN/isInfinite/isZero into BV at parse time via solver/src/fp.rs:85-97
(`key(x) = ite(sign, ~x, x ^ 0x8000...)`, soundness argued in a prose comment only). Same shape
for strings: parse.rs:13-21 interning to Term::Const with STR_W=32. formal/Anubis/ has no FP
module (verified: 15 modules, none FP). scripts/run_native_authoritative_gate.sh:124-127 drift
list contains only BV theorem names, so it cannot notice.

### 10. `spec { forall x . P(x) }` parses, generates zero obligations, and silently means nothing

**lane** `compiler` · **effort** SMALL

**Why:** Accepted-and-ignored is strictly worse than refused, and this is documented core MVP syntax in
two places. A user writes a universally-quantified specification, gets a green check, and has
had no obligation generated. The parser does not even read the predicate — it keeps the LAST
identifier or number it saw. Small, unambiguous, and it is the promise inverted in the most
literal possible way.

**Acceptance:** `anubis check` on a file containing a spec block exits non-zero with
ANUBIS_QUANTIFIER_UNSUPPORTED (or equivalent), a fixture locks the refusal, and
GRAMMAR.md/spec.md mark the construct unimplemented.

**Evidence:** compiler/src/frontend/mod.rs:3504-3521 parse_spec token-skips to the closing brace storing only
the last Ident/Number as Stmt::SpecBlock { forall: String } (mod.rs:506-508).
compiler/src/middle/mod.rs:16148 is a no-op arm in the obligation walker; mod.rs:10225 only
pushes the effect string "spec". Specified as core syntax at docs/language/GRAMMAR.md:41 and
docs/spec.md:14.

### 11. `hybrid { gpu {} cpu {} prove {} }` is documented as a lowering; it is a template with one literal substitution

**lane** `compiler` · **effort** SMALL

**Why:** A documented language construct that parses, type-checks and walks in five analysis passes, then
compiles to a hardcoded demo. User GPU and prove code is DISCARDED silently rather than refused.
Any program written against LANGUAGE.md:608 gets x=42 semantics. The fix is small because the
honest answer is refusal, not implementation.

**Acceptance:** `anubis build` on a hybrid block either refuses with ANUBIS_UNSUPPORTED_NATIVE_LOWERING
(matching the run path) or emits code derived from the actual gpu/prove bodies; LANGUAGE.md:608
no longer claims a Metal+RISC0 pipeline unless the former.

**Evidence:** VERIFIED at compiler/src/backends/native/mod.rs:112-128: the lowering scans the cpu block for
`Stmt::Let` whose name is literally "x" with a literal init, then `let cpu_val =
cpu_init_val.unwrap_or_else(|| "42".to_string())`, fed to hybrid::emit_hybrid_project which
string-substitutes into a checked-in template (hybrid/emit.rs:36). The gpu and prove bodies are
read by no backend. compiler/src/backends/run.rs:3758 already rejects hybrid on the run path.

### 12. Generics are a string heuristic: multi-char parameter names FALSELY REJECT, and any `<` makes a type compatible with everything

**lane** `compiler` · **effort** MEDIUM

**Why:** Two bugs in one 8-line function, in opposite directions. `fn pick<Item>(a: Item)` called as
`pick(1)` is a valid running program the ENFORCING path rejects — violating the repo's own
stated invariant that a working dynamic program is never rejected, and untested because every
fixture uses T/A/B. Simultaneously `let x: Option<u32> = "hello"` is accepted, so a check PASS
carries no claim at all about the contents of any container, Option or Result.

**Acceptance:** A fixture with `fn pick<Item>(a: Item) -> Item` called as `pick(1)` PASSES check; a fixture with
`let x: Option<u32> = "hello"` REJECTS; is_generic consults ctx.fn_generics rather than string
shape.

**Evidence:** VERIFIED at compiler/src/middle/ty.rs:258-264: `is_generic` = `t.contains('<') || (!t.is_empty()
&& t.len() <= 2 && all ascii uppercase)`. ty.rs:357-360 short-circuits `compatible` to true when
either side is generic. Enforcing (not shadow-gated) rejections at
compiler/src/middle/mod.rs:19878-19889 and :19284-19286. ctx.fn_generics (mod.rs:2863) holds the
real parameter names but is consulted only at :19672, :19692, :19733 — never on the
assignability path. Invariant it violates: docs/language/TYPESYSTEM_PHASE.md:44-46.

### 13. RESEARCH mode has no completion criterion anywhere and its evidence chain is broken

**lane** `offensive` · **effort** LARGE

**Why:** HANDOFF defines the language as two modes; the 11-phase ROADMAP tracks only one. The proof-
carrying thesis is INVERTED in the mode that most needs it — an operator cannot prove what they
ran. Worse, the lane's safety property is an accident: gated builtins are immune to the carrier
class because they have no runtime lowering, not because any predicate, test or comment prevents
it, so one reasonable-looking change to emit_builtin_call re-opens it with nothing to catch it.

**Acceptance:** `anubis vz exploit` and `anubis vz fuzz` each add at least one receipt to the chain such that
receipt-verify's output CHANGES, verified by diffing the chain before and after; a structural
test fails if a MODE-gated builtin gains an arm in emit_builtin_call; RESEARCH mode has a stated
done-when in ROADMAP.md.

**Evidence:** docs/COMPLETION_BLUEPRINT.md:246-254 'the tart path collects no evidence at all' — seal_action,
receipt, collect_loot, scrape have zero call sites there; vz exploit and vz fuzz produced a
SIGABRT and 14 unique crashes and left receipt-verify byte-identical while campaign-init (which
only writes Markdown) advanced the chain. docs/CLAIMS.md:360-375 'Not enforced, not probed, not
tested'. docs/HANDOFF.md:155 ~24,200 of ~26,700 dual-use lines unprobed.
tools/anubis/src/vz.rs:1127 mints the run capability with a stub id when --engage is omitted
(warned, not refused).

### 14. Silent-wrong-value residue inside SAFE mode: parse_int/int/float return 0 on malformed input, and 19 crypto builtins are UNMEASURED

**lane** `stdlib` · **effort** MEDIUM

**Why:** `parse_int("abc") == 0` is the runtime assuming rather than refusing on a path a contract can
then discharge against — it makes a contract hold for the WRONG REASON, which corrupts the proof
rather than stopping it (the repo's own framing of why it fixed 31 other builtins). The crypto
slice is both the most security-load-bearing and the only one never probed for that exact
failure mode. The 104/104 gate is a corpus score, not a coverage score: it exercises 81 of 213
builtins.

**Acceptance:** A domain/arity/wrong-type fixture matrix exists for all 19 crypto builtins and the fail-closed
gate count rises accordingly; STDLIB_CORE.md and BUILTINS.md either carry the
parse_int/int/float carve-out verbatim or those builtins refuse; BUILTINS.md's crypto table has
a host/guest column.

**Evidence:** compiler/src/backends/run.rs:2186-2188 `anubis_parse_int` uses `.unwrap_or(0)`; run.rs:1985-1986
anubis_int/anubis_float soft-coerce. docs/CLAIMS.md:141-146 'crypto / hash / KDF / random /
x25519 / pwn.anb | 19 | UNMEASURED'. LANGUAGE.md:536-539 carries the carve-out honestly;
docs/language/STDLIB_CORE.md:17-18 and BUILTINS.md:29-42 present the class as closed with no
exception. Separately: 11 of 41 crypto names hard-panic in the RISC0 guest
(pure_crypto_runtime.inc.rs:131-164) with no lane column in BUILTINS.md.

### 15. The Lean formal gate and the DX gate are in NO CI job — 162 theorems are never machine-checked on push

**lane** `formal` · **effort** SMALL

**Why:** Every proof claim in README, CLAIMS and ROADMAP is a locally-run assertion. A broken proof or an
introduced sorry would ship green. The same is true of the entire DX surface, whose advertised
15/15 comes from one manual run that predates two days of CLI and checker edits. Both fixes are
one line each in a workflow or in audit_unified.sh.

**Acceptance:** A push with a deliberately broken Lean proof turns CI red; a push with a deliberately broken
`anubis run` turns CI red; the formal gate executes `#print axioms` and asserts the axiom set,
or the '#print axioms-clean' claim is dropped from ROADMAP.md.

**Evidence:** VERIFIED: `grep -c 'run_formal_gate' scripts/audit_unified.sh` = 0 and `grep -c 'run_dx_gate'
scripts/audit_unified.sh` = 0. .github/workflows/ci.yml:53 runs only audit_unified.sh and :78
only audit_a_plus.sh. run_formal_gate.sh is invoked only from run_seal_checklist.sh:754 and
run_essence_spine_gate.sh:150, neither in any workflow, and the latter skips it under
ESSENCE_SPINE_FAST=1. Related overstatement: ROADMAP claims the set is '#print axioms-clean' but
scripts/run_formal_gate.sh:19-24 only greps the token `axiom` — `#print axioms` is never
executed.

### 16. Semantic diagnostics carry no file:line:col anywhere — CLI joins them into one string, LSP pins every contract failure to line 1 col 0

**lane** `tooling` · **effort** SMALL

**Why:** The user-facing half of 'it refused rather than assumed' is unusable when the refusal has no
location. The span data ALREADY EXISTS on the diagnostic and is read by the LSP path — the CLI
formatter simply throws it away. For the language's headline feature (a disproved
requires/ensures) the VS Code experience is a squiggle on the first character of the file, so
contract authoring has no in-editor locality at all.

**Acceptance:** `anubis check` on a fixture failing type/taint/effect analysis prints `file.anb:LINE:COL: error:
ANUBIS_*` with a source line and caret; the VS Code extension puts the squiggle on the offending
expression, not on character 0.

**Evidence:** compiler/src/middle/mod.rs:3002-3015 collapses every ctx.diagnostics entry to `format!("{code}:
{message}")` joined by '; ', discarding the span that compiler/src/lsp_analysis.rs:74-80 reads.
compiler/src/lsp_analysis.rs:92-102 obligation_to_lsp hardcodes line:0, character:0 for every
failed solver obligation. Parse errors DO get the good treatment (frontend/mod.rs:4541-4554
renders path:line:col + caret), proving the machinery exists. Still listed open at
docs/language/UNSUPPORTED.md:592.

---

## Doc corrections the lead must land

- docs/HANDOFF.md:20-22 — the promise sentence itself. 'everything it could not decide, it refused
  rather than assumed' is contradicted by the checker's own comments at mod.rs:6789, :7015,
  :6505-6507 and lib.rs:1338/:4630, and stated as policy at mod.rs:273-274. Scope it to the
  obligation classes actually discharged, or change the behavior. This is the correction
  everything else hangs off.
- README.md:9 — the unqualified green 'self-host — byte-identical fixpoint' badge publishes
  exactly the claim docs/CLAIMS.md:76 forbids publishing ('VM seal of post-registry fixpoint |
  Pending | Do not publish host fixpoint as sealed'), and README.md:242 contradicts it in the
  same file. Qualify or remove the badge until the seal lands.
- LANGUAGE.md:546 — stdlib fail-closed gate stamped 86/86; measured 104 on disk and six other docs
  say 104/104. Fix the number AND the `as of` phrasing that let it survive the drift gate.
- docs/language/ROADMAP.md:31-32, :52, :184 — the 'self-verify before quoting a green number'
  block itself is stale: security 228 (actual 311), stdlib 45 (actual 104), native 681 (actual
  882), fixpoint a01a1e8b (actual 189ac496, re-baselined 2026-07-24 per EXPECTED_FIXPOINT_VM's
  own log). Quoted twice more at :167 and :258.
- docs/language/ROADMAP.md:4 and :23 — the single pointer to the living defect list is a dead
  anchor (`CLAIMS.md#known-open-issues-2026-07-26`); the heading is `## Known open issues
  (2026-07-27)`.
- docs/language/ROADMAP.md:279 and :295 — the verbatim arc says 'Phase zero is the trust spine,
  and it is done' and 'Phase 4 ... DONE (2026-07-21)', while the living table in the SAME FILE
  at :167 and :171 says PARTIAL with VM seal pending. One file, two verdicts.
- docs/language/ROADMAP.md:118-121 and :261-264 — remove the stale Phase-9 blocker;
  run_selfhost_gate.sh no longer calls host `anubis run --allow-research`.
- docs/COMPLETION_BLUEPRINT.md — self-contradictory on its own headline gate: :80 'Phase 1 — DONE.
  Twelve carriers closed' vs :138-142 listing five as OPEN vs :169 'Done when — All 9 carriers
  CLOSED'. Also :37/:39/:76 carry three different security counts (280, 278/278, 294/294)
  against 311 measured.
- docs/COMPLETION_BLUEPRINT.md:92 'Phase 3 — DONE' and docs/HANDOFF.md:126 'Gate-harness integrity
  | DONE' — re-opened 2026-07-28 with four findings, three of which I reproduced firsthand. Also
  update the blueprint's Phase-2 done-when: commit 5bb0b38 replaced 'reduce ~7 walkers to one
  shared abstraction' with total construct coverage over a measured 26.
- docs/CLAIMS.md:118 — cites 'item 7' as evidence a carrier is still open; item 7 at :653 reads
  CLOSED. Separately the § Known open issues numbering jumps 1,2,3 -> 9,10 with items 4-8 living
  in a different section, so every 'item N' cross-reference is ambiguous by construction.
- LANGUAGE.md:6 and :613 — both point the reader to SOUL.md for the research/proof surface.
  VERIFIED: the file does not exist at the repo root. The language reference's only pointer to
  the SAFE-vs-RESEARCH story is dead. Same class: README.md:136 cites `scripts/thorough_test.sh`
  (actual path is examples/showcase/anubis_vault/scripts/thorough_test.sh).
- LANGUAGE.md:260-263 and docs/SOLVER_PIPELINE_MAP.md:38/:65 — both UNDERSTATE the solver, telling
  users to route to runtime assert things the checker now proves (`/ % << >>`, truncating casts,
  float compare, string equality, str.len, bounded arrays are all modeled).
  SOLVER_PIPELINE_MAP.md is dated 2026-07-05 and predates the Phase-3/4 lanes. The user-facing
  rejection strings at mod.rs:16946-16955 are stale the same way ('optional QF_S later' /
  'optional QF_FP later' when both lanes are live).
- solver/README.md:46, :84, :113, :123 — says var x var mul is deferred; it was admitted
  2026-07-25 and solver/src/lib.rs:12-15 already flags its own README as stale. Also :7-8's
  'zero external solver dependency' is true of the crate but not of `anubis check`, which turns
  a missing z3 into FAIL for every array, division and over-budget obligation.
- docs/CLAIMS.md:72 and :710 — '882 files, 0 mismatches' for the native-authoritative gate. The
  largest stored gate log says 719; 882 is the CURRENT CORPUS SIZE from a docs-drift scan. The
  number was refreshed to match the corpus without a run producing it. Re-run or re-cite.
- README.md:189 markets 'Effects & capabilities | ✅' with no lane qualifier, but fail-closed
  `uses(...)` enforcement requires --verified/@verified (mod.rs:2837-2839) while every Quick-
  start invocation is plain `anubis check`. Two different guarantees marketed as one.
- docs/INSTALL.md:9, :47, :50 — the literal first-run commands hardcode
  `/Users/sicarii/Desktop/metal-hybrid-prover`. Onboarding fails on any machine but the
  author's; 10 files under docs/ still contain that path.
- docs/language/BUILTINS.md:15-18 — the 'Reproduce:' block points at
  scratchpad/fleet_20260726/grok_thoth_stranger.md, which .gitignore:58 excludes from the repo.
  Point it at scripts/test_docs_drift_gate.sh, which actually ships and works.
- docs/CLI.md — omits `anubis test`, `anubis fmt`, `anubis build` usage and `--suggest-contracts`
  (0 hits each), all four shipped and gate-covered. The designated CLI reference is behind the
  binary.

---

## Already done — do NOT redo

- Tag-lane defect factory: CONVERGED, 8/8 join sites closed, verified by an adversary that had
  predicted the opposite and retired its own judgment. Do not re-open or re-audit.
- Function-identity carrier family (bare name, alias chain, if-join, match-join, list element,
  struct field, map value, enum, return, container-as-argument): CLOSED 2026-07-27 at 0eb5977
  per docs/CLAIMS.md:653/660. docs/COMPLETION_BLUEPRINT.md:138-142 still lists five of these as
  OPEN — the blueprint table is the stale half. The remaining gap is only the MUTATION and PARAM
  producers (ranked item 1).
- Phase-9 self-host re-verification is NOT blocked. ROADMAP.md:118-121 and :261-264 say the gate
  'cannot currently execute' because it calls host `anubis run --allow-research`;
  scripts/run_selfhost_gate.sh:53-59 already dropped that flag for exactly this reason and
  SH_RUN carries no --allow-research. The blocker was fixed in the script and never removed from
  the doc.
- Shadow-mode promotion of the bidirectional type checks ALREADY HAPPENED — the checks are
  enforcing (shadow_gated=false at all 23 sites). Only the doc comments at
  mod.rs:2504/:2510-2512 and ty.rs:444 still describe them as shadow-gated. Do not 'restore'
  shadow gating on these checks; the only live question is what to do with the now-inputless
  gate script (ranked item 5).
- LANGUAGE.md's 'returns 0 on empty' correction is DONE: LANGUAGE.md:559-563 already documents
  ANUBIS_EMPTY_COLLECTION / ANUBIS_NO_MATCH / ANUBIS_TYPE_ERROR panics. BUILTINS.md:41-42 still
  tells readers LANGUAGE.md is wrong. Retire the pointer, do not redo the fix.
- LSP ships and is gate-verified (tools/anubis/src/main.rs:1579-1745,
  out/dx_gate/lsp_roundtrip.log init_ok/diag_ok/hover_ok all true). MATURITY_CLAIM_MATRIX.md:61
  lists LSP as 'out of this slice' — stale in the conservative direction.
- Multi-file import resolution is REAL and transitive with fail-closed ANUBIS_IMPORT_CYCLE
  (compiler/src/resolve/mod.rs:1-10, :239) and a passing run test. README.md:240/:360 marking it
  'in progress' understates the shipped code. (The genuine residual is that namespacing/privacy
  cover functions only — that is a separate, smaller item.)
- Static monomorphization is REAL (compiler/src/lib.rs:5657-5699 asserts non-empty
  ir.mono_specializations; evidence/mod.rs:471 writes the sidecar). README.md:240's 'not yet
  statically monomorphized' contradicts both LANGUAGE.md and the code.
- The div/rem/ashr/sign_extend terminal deferral in the native solver is CORRECTLY and COMPLETELY
  gated with nested negative tests (solver/src/fragment.rs:138-143, :259-307) and a drift check.
  It is design, not debt — do not treat 'division residual' as work to close.
- Solver termination bounds are proven not to be able to flip a verdict (solver/src/lib.rs:692-731
  bounds_only_ever_weaken_to_defer). Do not re-derive or re-audit the four budgets.
- The 213-builtin inventory is exact and independently re-derivable (0 names in doc-not-in-code, 0
  in code-not-in-doc, with a live guard at scripts/lib/docs_drift_derive.py:123). Only its
  'Reproduce' pointer is broken.
- `shell`/`exec`/`system`/`memcpy`/`sql` have NO runtime lowering in any mode including --allow-
  research, locked by two tests (run.rs:8681-8702, :8714-8749). The gating is sound and the docs
  are accurate.

