# NEXT SLICE — Scope-aware flow walker (Phase 2 follow-on)

> Self-contained handoff for a **fresh session**. Paste the "SESSION PROMPT" block, or open this file
> and work from it directly. It carries the full phase roadmap at the bottom so nothing is lost.
> Committed at HEAD `6747431` (composite/aggregate flow `a930e7e` + Ammit wiring). Fixpoint: `dc680001` (in-VM).

---

## SESSION PROMPT

Anubis — Phase 2 next slice: make the flow walkers **SCOPE-AWARE** (walk control-flow value expressions soundly).

Repo: `/Users/sicarii/anubis-lang`, branch `a-plus-maturity/20260705-1649`, HEAD `6747431`.

### LOAD FIRST (living context — do not skip)
- Memory: `MEMORY.md` index; especially [[anubis-roadmap-source-of-truth]] (what phase / what's next),
  [[ammit-evidence-deck]] (verify my own claims), [[workflow-review-agents-must-be-read-only]] (a hard lesson).
- Committed docs: `docs/language/ROADMAP.md` (STATUS + NEXT ACTION #1 is THIS slice — the full arc is also
  reproduced at the bottom of *this* file), `docs/language/UNSUPPORTED.md` (the "Control-flow value
  expressions" + "COMPOSITE/aggregate flow" boundary entries describe exactly this gap).
- `git log a930e7e 6747431` — the composite/aggregate slice that NAMED this follow-on.

### WHAT TO BUILD (one sentence)
Make the four flow walkers track **lexical scope** so control-flow value expressions
(`match` / `if` / `if let` / block) can be walked **soundly** — closing the composite follow-on and
retiring the tail-`if`/`match` return-summary boundary at the same time.

### THE PRECISE PROBLEM (reproduce it FIRST, before changing anything)
The four flow walkers are **FLAT**: they resolve `Expr::Var(name)` by an ambient-scope lookup with no
lexical-scope tracking. In the composite slice (`a930e7e`) I added control-flow arms (match/if/if-let/block)
and the adversarial review CONFIRMED they were unsound BOTH ways, so I removed them:
- **(a) SHADOWING FALSE POSITIVE** (accept-bias violation): an inner binding — a match/if-let PATTERN var
  or a block-local `let` — that shadows a same-named OUTER tainted/secret binding was mis-resolved to the
  outer. E.g. `let key = secret_source(); let picked = match mode { Custom(key) => key, _ => 0 }; send(picked);`
  falsely rejects, because the arm's `key` (pattern-bound, clean) resolved to the outer secret `key`.
- **(b) PASSTHROUGH FALSE NEGATIVE**: a value passed THROUGH an inner binding launders. E.g.
  `send({ let v = secret; v })` and `let inner = match s { Wrap(inner) => inner, _ => 0 }; send(inner);`
  are missed, because the block-local/pattern binding is never seeded.

Reproduce BOTH (a) and (b) as failing scratch programs first (they currently compile — (a) correctly, since
I removed the arms; (b) as a genuine missed-leak) so you have a red baseline the fix turns green/precise.

### GROUNDING (all four walkers live in `compiler/src/middle/mod.rs`; grep the fn names)
The four flow walkers to make scope-aware (each already takes `&scope`/`&flow` and currently has **NO**
match/if/if-let/block arm — they fall to the catch-all; they DO have the pure-aggregate arms
ArrayLiteral/StructLiteral/EnumConstruct/MapLiteral/Try from `a930e7e`, which are sound and MUST stay):
1. `fn expr_taint_source(expr, scope, tainting_fns, param_return_taint) -> Option<String>` (integrity)
2. `fn expr_secret_source(expr, scope, secret_fns, param_return_taint) -> Option<String>` (confidentiality dual)
3. `fn expr_param_flow(expr, flow) -> BTreeSet<usize>` (param→sink summary)
4. `fn expr_param_return_flow(expr, flow, known_param_return) -> BTreeSet<usize>` (param→return summary)

Expr variants (`compiler/src/frontend/mod.rs`, ~line 500–610) with field types:
- `Match { scrutinee: Box<Expr>, arms: Vec<MatchArm>, span }` — `MatchArm { pattern: Pattern, guard: Option<Expr>, body: Expr }`
- `If { cond, then: Box<Expr>, else_: Box<Expr>, span }` — then/else_ are **Block** expressions
- `IfLet { pattern: Pattern, scrutinee: Box<Expr>, then: Box<Expr>, else_: Box<Expr>, span }`
- `Block { stmts: Vec<Stmt>, tail: Option<Box<Expr>> }`

PIECES THAT ALREADY EXIST (reuse — do not reinvent):
- `Pattern::bound_names()` → the vars a pattern binds (used today in the WhileLet handler; grep `bound_names`).
- Scope-aware BLOCK precedent: `fn body_returns_taint` / `body_returns_secret` ALREADY snapshot/restore the
  scope around if/loop/hybrid bodies and seed lets in order — study them; the control-flow arms need the
  SAME snapshot/seed discipline, just applied when the value-expression itself is a match/if/block.
- Let-seeding: `fn seed_one_let` (taint), `fn seed_one_let_secret` (secret), `fn seed_param_flow_let` (param).
  These insert a `ScopeBinding` with the right label computed from the init. Reuse/generalize them to seed a
  block-local `let` inside the walker.
- The intra Assign flow + `merge_taint_over` (span-identity) show the may-union + shadow discipline.

### DESIGN DIRECTION (get it adversarially reviewed BEFORE coding — see discipline)
Handle the three binding-introducing value expressions inside each walker by building a **LOCAL extended
scope** (clone the ambient scope, seed the new bindings so they SHADOW outer ones of the same name, recurse
into the value in the extended scope). No signature change is required — the extension is local per arm.
- **Block** `{ stmts, tail }`: clone scope; walk stmts IN ORDER seeding each block-local `let`/`let-pattern`
  (a let can reference an earlier one); evaluate `tail` in the extended scope. A block-local `let x` that
  shadows an outer tainted `x` overwrites the entry → no false positive; a `let v = secret; v` tail seeds
  `v` as secret → passthrough caught.
- **Match** `{ scrutinee, arms }`: compute the scrutinee's label ONCE; for each arm, clone scope, seed every
  `arm.pattern.bound_names()` var with the SCRUTINEE's label (destructuring a secret/tainted scrutinee
  yields secret/tainted parts — conservative), then evaluate `arm.body`. Union over arms (may-carry). A
  pattern var of the same name as an outer binding shadows it → no false positive.
- **IfLet** `{ pattern, scrutinee, then, else_ }`: like Match for `then` (seed pattern vars from the
  scrutinee's label); `else_` gets no bindings. Union then+else.
- **If** `{ then, else_ }`: then/else are Block exprs → handled by the Block case; union then+else.

Semantics per walker: label walkers (1,2) use Option/or_else (may-carry); set walkers (3,4) use union.

Key soundness question to nail in design: the scrutinee→pattern-var label flow (is "all pattern vars inherit
the scrutinee's whole label" the right conservative choice, vs. field-precise?), and whether a guard
expression can contribute (it can't be the arm's VALUE, so likely no — confirm).

**WATCH FOR** (this is the subtle part — think deeply): nested blocks; a pattern var shadowing then a later
OUTER reference after the block (must NOT leak — the extension is local to the arm/block); mutual recursion
of the walkers via the aggregate arms; declassify inside a seeded binding; and the interproc summaries (a
param destructured out of a match arm must still flow to the return for `expr_param_return_flow`).

### FILES
- `compiler/src/middle/mod.rs` — the four walkers + seeding helpers.
- `compiler/src/lib.rs` — unit tests.
- `tests/fixtures/language_core/*.anb` — new fixtures.
- `docs/language/UNSUPPORTED.md` + `ROADMAP.md` — flip the "Control-flow value expressions" boundary to REAL,
  update STATUS + NEXT. Then delete/retire this handoff file.

### FIXTURES + TESTS (must include all of these)
- **REJECT** (passthrough now caught): secret/tainted through a block-local let then sink; through a
  match-arm that returns a pattern-destructured field of a secret scrutinee; through an if-expression branch.
- **ACCEPT** (shadowing FP closed — THE regression this fixes): outer tainted/secret `x`, inner match/if-let
  pattern or block-let named `x` that is CLEAN and returned → must accept (no false positive). This is the
  exact reproduction from problem (a).
- **ACCEPT** (precision): a clean match/if/block value to a sink accepts; a declassify inside a branch releases.
- **Interproc**: `fn pick(x){ return match x { _ => x }; }` summarizes `{0}` so `send(pick(secret))` rejects.

### DISCIPLINE (non-negotiable — how every slice here runs)
- Heavy builds ONLY in the tart VM via `bash scripts/vm/run-slice.sh` (the host twice hit a watchdog reset
  from all-core builds). It runs the full battery + asserts the self-host binary fixpoint holds at `dc680001`,
  and now also emits the VM's real libtest JSON to `.ammit/cargo-test.json` and collects it back.
- This change edits ENFORCING shared walkers, so shadow-first isn't clean; the corpus enforcing gates
  (language/turing/security/stdlib) + shadow diff UNEXPECTED=0 are the regression test. Reproduce the FP and
  the FN first (red baseline), then fix.
- Adversarially review the DESIGN before coding AND the landed code after, via the Workflow tool with
  `agentType: 'Explore'` (**READ-ONLY** — a default-type workflow agent once WROTE to a source file and
  corrupted it; see [[workflow-review-agents-must-be-read-only]]). Verify every blocking finding
  independently. After any workflow, `git diff --stat` to confirm no agent mutated the tree.
- After the VM is green, run `ammit weigh` (`~/.local/bin/ammit`) and confirm the anubis-lang deck stays
  0-contradicted; the machine-produced test result is your evidence, not your own count.
- Commit atomically on green with the exact battery numbers + "fixpoint UNCHANGED at `dc680001`" and the
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer. Then a docs commit. Update memory.
- No parser/AST/selfhost-projection change (keep the fixpoint). `ty.rs` parity oracle is frozen — do not touch.
- Every guarantee proven or precisely scoped; residuals named, not hidden. **The human presses send** — no
  agent-created public repos, no agent-pressed sends. Nothing is pushed; all commits stay local on the branch.

### DEFINITION OF DONE
The four flow walkers walk match/if/if-let/block soundly: the shadowing false positive is closed (accept),
the binding-passthrough false negatives are caught (reject), precision holds on clean control-flow values,
and the interproc summaries carry a param destructured through a branch. Full VM battery green, fixpoint
`dc680001` unchanged, Ammit 0-contradicted, single atomic `feat` commit + docs commit. **Recommend STARTING
in plan mode with a design workflow**, because the scrutinee→pattern flow and the shadow/passthrough duality
have real subtleties (the composite slice's first attempt shipped a false positive that only the adversarial
review caught).

---

## FULL ROADMAP (all phases)

### STATUS — as of 2026-07-15 (HEAD `a930e7e` code / `6747431` docs)

| Phase | State | One-line |
|---|---|---|
| **0 — Trust spine** | ✅ DONE | byte-identical source + binary fixpoint (`c640badd` host / `dc680001` in-VM), ablation gate, DDC C-native parser lane, hermetic repro. Residual → author-diversity (Phase 7). |
| **1 — Real type system** | 🟡 ~90% | bidirectional inference, floats, captured generics, real traits + coherence — enforcing. RESIDUALS: typed `?`; generic-bound capture + trait **bound** checking. |
| **2 — Capability & effect (the differentiator)** | 🟢 CORE DONE | effect inference (Koka rows) + linear capability tokens (Austral) + effect-capability composition; **lethal trifecta = compile error** + shell egress + confidentiality label; taint reassignment fail-open CLOSED; **leg-1 confidentiality FLOW** (`ANUBIS_SECRET_EXFILTRATION`); **interprocedural secret + leg-2** summaries; **composite/aggregate flow**. RESIDUALS: **scope-aware flow walker (THIS SLICE)**; confidentiality param→egress-sink dual; `secret<T>` qualifier; trifecta in Safe mode; presence-level declassify hatch; closure/method calls; `sql` egress; field granularity + higher-order taint summaries. |
| **3 — Broaden verified surface** | ⬜ TODO | QF_FP (float) + QF_S (string) solver lanes + bounded-array maturity, each fail-closed; differential harness beyond i64. Can run partly parallel to Phase 2. |
| **4 — Port checker into Anubis** | ⬜ TODO | port inference + effect + taint engines to Anubis so the ablation gate covers type-checking too. Do once, after 1–3 settle. Fixpoint may reseal. |
| **5 — Mechanized semantics + soundness** | ⬜ TODO | small-step semantics in Lean/Rocq; prove (a) Safe-mode non-interference, (b) inferred-⊆-declared effect soundness, (c) SMT-encoding soundness. |
| **6 — Proof-carrying packages** | ⬜ TODO | signed evidence bundles (source Merkle root + effect/taint summaries + fixpoint); consumer verifies + inherits; fail-closed on tamper. |
| **7 — Ma'at endgame (minimize TCB)** | ⬜ TODO | shrink Rust runtime seed to an audited kernel; second independently-authored parser + independent second backend (closes author-diversity residual). |
| **8 — Developer experience** | ⬜ TODO | LSP (hover shows contracts+effects+taint), formatter, test runner, doc-gen, REPL, tree-sitter, registry, spec, tutorial. |
| **9 — Independent external reproduction** | ⬜ TODO | publish the self-contained proof bundle; multiple strangers reproduce the sealed hashes + confirm negative controls. |
| **10 — Production hardening + 1.0 freeze** | ⬜ TODO | ship real systems in ≥2 domains with evidence bundles; semver + backwards-compat policy + frozen 1.0 spec. |

### The arc, verbatim (operator-authored 2026-07-14)

Before the phases, the honest frame, because it determines the whole shape. Full maturity for this language does not mean catching up to Rust or Dafny on feature count — you'd lose, and it's the wrong race. It means completing the one thing no other language holds all of at once: push-button proof, counterexamples an agent can actually read and repair, capability-and-effect enforcement that makes whole classes of attack a compile error, evidence bundles that let one party verify another's work without trusting them, and a self-reproducing compiler witnessed by diverse toolchains. My research this session confirmed the gap is real — Dafny has the automation but no capabilities, no evidence bundles, no self-host witness; Austral has the capabilities but no SMT and no proofs; Koka has the effects but no verification; Lean has the depth but twenty-seven percent agent success and no systems story; CakeML has the verified minimal core but none of the rest. A class of its own means assembling that entire stack to production quality, and proving or precisely scoping every guarantee along the way. The maturity is the assembly, and the honesty about each boundary is what makes the assembly believable. Here is the arc, dependency-ordered, from the foundation you've already banked to completion.

Phase zero is the trust spine, and it is done — the byte-identical source and binary fixpoint, the ablation gate that proves the language's own features are load-bearing, the diverse double-compiling lane now closed at the source-derivation level by your C-native parser, and the hermetic reproducibility lane. This is the foundation everything downstream inherits, and it is the rarest thing you own. Its residual is now author-diversity, which Phase seven addresses. You built this first correctly, because a package manager shipping evidence bundles means far more when the compiler verifying them is itself witnessed by two toolchains.

Phase one is making the type system real, and you are inside it now. The bidirectional inference core landed, floats closed the unsound aliasing bug, and what remains is captured generics, real traits with coherence and bound checking, and typed question-mark propagation — each landing shadow-clean through the harness with the fixpoint holding on c640badd. This is first because everything after it needs a real type system beneath it: you cannot build a capability system, or port a checker, or prove soundness, over a stringly-typed substrate. Definition of done is the four workstreams enforced at zero shadow regressions, the frozen parity oracle untouched, the fixpoint unmoved. The honest boundary is that the checker is still Rust after this — porting it into Anubis is Phase four.

Phase two is the differentiator, the one that earns the agentic-engineering claim outright: the capability-and-effect system that makes the lethal trifecta a compile error. You fuse three proven ideas onto the real type system from Phase one. From Koka you take row-polymorphic effect types, so a function's side effects are visible in its signature and an expression typed without an effect provably never performs it — maturing your existing uses clause into real inference. From Austral you take capabilities as unforgeable linear tokens, so the only way to touch the filesystem, the network, a signing key, or a fund transfer is to hold a token that cannot be duplicated or conjured, which is exactly how you constrain a dependency or a sub-agent from doing what it was never granted. And from your own taint layer you make I/O reads into taint sources and I/O writes into sinks, so a program that reads untrusted content and reaches an exfiltration path without an explicit declassification simply will not compile in Safe mode. This phase also closes the one documented taint fail-open — the reassignment-insensitivity — with proper control-flow-merge dataflow, and adds higher-order summaries and field granularity, retiring three of the residuals the dissertation lists at once. This is the phase that matters most for your stated audience, because the entire security industry converged in 2026 on the conclusion that prompt injection is architectural and unpatchable, with OWASP mapping it to six of ten agentic risks — and Anubis can be the first language where the structural separation everyone prescribes is enforced by the type checker rather than by hope. Definition of done is a fixture suite where the trifecta is a type error with accept-bias guards proving it doesn't over-reject dynamic code, and the effect discipline's inferred-subset-of-declared property stated soundly enough to be proven in Phase five.

Phase three broadens the verified surface soundly, and it can run partly parallel to Phase two since it touches the solver rather than the effect system. You add the float and string solver lanes that are currently planned behind flags — QF_FP and QF_S — and mature the bounded-array lane, each under the same fail-closed discipline where anything outside the model is refused with a precise diagnostic rather than falsely discharged, and each guarded by extending the differential-testing harness beyond its current i64-only generation. This is a coverage gain inside an already-sound framework; the obstacle, as the dissertation names it, is widening what can be modeled without ever claiming a proof you don't have. Definition of done is the harness exercising floats and strings with zero false-accepts and every unmodeled construct still deferring.

Phase four completes the self-hosting story by porting the checker into Anubis. Right now only parse, basic check, and codegen are dogfooded; the type checker, the effect checker, and the taint engine are Rust. Porting them — the real inference from Phase one, the capability-and-effect discipline from Phase two — makes the checker itself Anubis, so the ablation gate extends to prove the type-checking constructs load-bearing too. You do this after the checker's shape has settled through Phases one through three, so you port it once rather than three times. The fixpoint may reseal to a new hash, which is fine as long as the gate still enforces it. The honest boundary is the one the dissertation already draws: this deepens self-hosting toward the irreducible runtime seed but does not eliminate it, because something must still execute the first interpreter.

Phase five takes the language into genuinely rare territory: mechanized formal semantics and soundness proofs. You give Anubis a small-step operational semantics in Lean or Rocq and prove three theorems — that the Safe-mode taint discipline enforces non-interference, meaning no tainted value reaches a sink without declassification, as a theorem rather than a set of enforced diagnostics; that the effect discipline's inferred-subset-of-declared property is sound with respect to that semantics; and that the SMT encoding is sound, so a discharged contract implies the runtime property. This is what puts the distinctive guarantees on a proof footing instead of an enforcement footing, and it is the company of CompCert and seL4 and CakeML, scoped narrowly to your specific disciplines. The obstacle the dissertation correctly identifies is that the erased dynamic runtime means these theorems are stated over dynamic semantics, which changes their shape from a conventional type-soundness result. You prove soundness before Phase six propagates these guarantees through a dependency closure, because you want the properties proven before you multiply them across an ecosystem.

Phase six is proof-carrying packages, the phase that multiplies the trust spine across the supply chain. The package manager produces and consumes evidence: every package ships a signed bundle carrying a source Merkle root, its effect and taint summaries, and its fixpoint, and the consumer verifies each dependency's bundle and inherits its summaries, so a dependency's declaration that it returns tainted data, or needs network access, or requires a nonzero divisor, is enforced at the consumer's own call sites, fail-closed on anything unverified or untrusted. This is the cryptographic answer to the supply-chain problem — the one that let a single viral agent accumulate five hundred vulnerabilities — and it pays off more precisely because the compiler verifying those bundles is witnessed by diverse toolchains from Phase zero. Definition of done is a multi-package project where a dependency's summaries are hash-pinned and enforced at call sites and the build fails closed on tamper.

Phase seven is the Ma'at endgame: minimize and independently verify the trusted base, and close the author-diversity residual. You shrink the hand-written Rust runtime seed toward a small, audited kernel with its own mechanized correctness argument — the CakeML north star, where the trust reduces to a tiny examined core rather than a thousand-line interpreter. Here also live the two deepest trust moves the dissertation flags as future work: a second, independently-authored parser from an independent reading of the spec, which is the actual closure of the author-diversity residual your C-parser work exposed, and a genuinely independent second backend that shares neither execution machinery nor code generator, so the diverse lanes agree at a deeper level than execution. The obstacle is the irreducible bootstrap floor — the seed cannot be zero — so the honest goal is minimize-and-verify, not eliminate.

Phase eight is full developer-experience and toolchain maturity, the adoption layer. The LSP grows from MVP to complete, and it can do the one thing no other language's tooling can — hover a function and show its contracts, its effects, and its taint status. The formatter, the test runner, a doc generator that surfaces requires and ensures as a contracts section, a REPL, a tree-sitter grammar, syntax highlighting, a package registry, a language spec, and a real tutorial. Each ships with an integration test exercising the real path, not a stub. This is the phase that decides whether anyone but you ever writes Anubis, and it is deliberately late because a rich toolchain over an unsound core would be polishing the wrong thing.

Phase nine is the one the dissertation names as most valuable and the one you cannot do alone: independent external reproduction and adversarial scrutiny. You publish the self-contained proof bundle — the fixpoint source, both manifests, the gate scripts, the pinned toolchain and container digest, the reproduction instructions — such that a stranger can reproduce the agreed hashes and confirm the negative controls on their own hardware. The entire Thompson frame is about not trusting a single party, and every hash in your dissertation was verified only by you; a trust artifact whose certificates have never been re-checked by anyone else has a self-referential tension it cannot resolve from the inside. This is less a research problem than an exposure task, and the C-parser closure makes what a stranger would reproduce — source-to-execution diversity, not just execution diversity — far more compelling than it was a week ago. Definition of done is multiple independent parties reproducing the sealed hashes and confirming the controls, documented.

Phase ten is production hardening and the 1.0 spec freeze — maturity proven by use rather than asserted. The language demonstrates itself by building real systems in its class behind proper host applications with authenticated data and human approval: a high-assurance financial engine matured from your ledger kernel, an agentic control plane where the trifecta-as-compile-error from Phase two is deployed in a real orchestrator, evidence-native security tooling where a bug-bounty finding ships with a receipt a skeptical triager can re-derive, and consensus and ledger cores where the proofs are load-bearing. Alongside it comes the stability contract: semantic versioning, a backwards-compatibility policy, and a frozen 1.0 specification for the stable surface, so the language becomes something others can build on for a decade. Definition of done is real systems shipped in at least two target domains with their evidence bundles, and a versioned spec freeze.

Two honest truths to close on. First, completion for a language is asymptotic — the incumbents you'd stand beside have decades and communities, and no roadmap reaches "one hundred percent done" as a discrete event; what these phases give you is the point at which the distinctive stack is complete, every guarantee is either proven or precisely scoped, and the residuals are named rather than hidden, which for this language is the only definition of done that would be honest. Second, if I had to name the single highest-leverage move in the whole arc, it is Phase two — the capability-and-effect system that makes the lethal trifecta a compile error — because it is the one capability that no incumbent can stack onto what they already have, and it is the one that turns "a verification-first language" into "the language you must use to build an agent that cannot be turned against you." Everything else deepens or proves or propagates; that phase is the thing that makes it a class of its own. The trust spine you already banked is what makes anyone believe the claim when you make it.
