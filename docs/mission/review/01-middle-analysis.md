# 01 — Middle / analysis layer map (`compiler/src/middle/`)

Branch `mission/anubis-1.0`. Read-only map. Line numbers are against the working tree at the time
of reading (2026-09-23). `docs/CLAIMS.md` cites older line numbers for the same code; the numbers
below are the current ones.

**Reading coverage (honest):**

| Read in full | Read in part (targeted ranges) | Only searched / header |
|---|---|---|
| `contract_carrier.rs` 1–175, 1125–1473; `carrier.rs`; `mod.rs` 40–130, 175–1010, 2941–3530, 5593–5836, 7215–7424, 7447–7510, 7634–8410, 8594–8640, 8840–9100, 9136–9290, 10220–10300, 10633–10852, 11395–11500, 13018–13060, 13120–13400, 14192–14240, 14615–14741, 16355–16400, 17165–17255, 20080–20160, 21313–21410, 23700–23900, 24080–24160, 24522–24680, 24733–24760 | `effects.rs` 20–160, 257–300, 425–472; `ty.rs` 1–60, 214–245, 474; `security_label.rs` 60–110; `trifecta.rs` 44–108; `capability.rs` 7–60; frontend `parse_params` | `research_profile.rs`, `proptest.rs`, `diverge.rs`, `loopctl.rs` (headers + item list); `capability.rs` body; `mod.rs` wrap-safety (6612–7215), solver/z3 (15253–17095), SMT encoders (17096–19004), loop invariants (21511–22019), typing (22415–23584), summaries (26383–28500) — outline only |

Findings marked **(candidate)** come from reading code and were **not reproduced** by running
anything (no cargo, no `anubis check` was run for this review). Findings marked **(confirmed in
code)** are direct readings of control flow. Findings marked **(docs)** are restated from
`docs/CLAIMS.md` / `docs/evidence/ITEM21_FAMILY1_CONTRACT_CARRIER_2026-09-23.md`.

---

## 1. Inventory

| File | Lines | Role |
|---|---:|---|
| `mod.rs` | 29,994 | Everything else: HIR/IR structs, `SemanticContext`, program-surface registration, interprocedural summaries, per-function analysis (`analyze_function` / `analyze_stmts` / `analyze_expr_effect`), taint/secret value-flow, contract discharge, SMT encoding, Z3/native solver drive, counterexample formatting, typing checks, unit tests (from 28912). |
| `capability.rs` | 4,436 | Linear capability tokens (`cap_acquire`, move-on-rebind, reuse/missing, non-exportable sealing, causal spend in verified mode). Own walker (`walk_stmts`/`walk_expr`, `merge_branches`), own `CapMap` keyed by variable name. |
| `ty.rs` | 1,938 | String-type predicates (`normalize`, `is_integer`, `is_secret`, `container_element_type`, …) that `mod.rs` delegates to, plus a structured `Ty` enum and HM-lite `InferEnv`/`synth`. Header says the structured form is "not yet consumed by the checker" (`ty.rs:8`). |
| `contract_carrier.rs` | 1,473 | Item-21 Family-1 collector: per-callee `CarrierItem`s (Apply / CallNamed / Escape / Return + guards + for-vars) for function-valued formals; `function_like_formals` gate. |
| `research_profile.rs` | 975 | Research/security-profile HIR types and `ProvenEffectSet` (shared effect IR for confinement). |
| `security_label.rs` | 786 | `SecurityLabel {Clean, Labeled{source}, Unknown{reason}}` lattice + legacy bool adapters + correspondence observer. |
| `effects.rs` | 678 | Transitive effect rows (6 capability ids + `open` bit), call-graph fixpoint; builtin effect table; HOF closure-arg table. Has a real lexical `Scope` stack. |
| `trifecta.rs` | 592 | Lethal-trifecta leg scan (private read + untrusted input + egress) and `leg2_fns` fixpoint. Own alias map (`collect_fn_aliases`). |
| `proptest.rs` | 289 | Deterministic program generator for solver↔runtime differential tests. |
| `diverge.rs` | 173 | `block_diverges`: does a block definitely not fall through (early-return guard facts). |
| `loopctl.rs` | 173 | `break`/`continue` outside a loop. |
| `carrier.rs` | 163 | `carrier_class(Expr)`: total, wildcard-free classification of how a construct can carry a callable. **No consumer outside its own tests** (grep over `compiler/`). |
| **Total** | **41,670** | |

---

## 2. Architecture: resolved AST → solver obligations

### 2.1 Driver (`typecheck_ex`, `mod.rs:3357–3527`)

1. Collect impl method names → `declared_method_names` (3369–3374).
2. `register_program_surface` (3529…): enums, `all_fns`, `fn_params` (name → param type **strings**, 3885), `fn_contracts`, `fn_carrier_items`, `fn_sole_return`, `fn_returns_param`, method twins, struct field types, applied-param alias scans.
3. Fixpoints: `compute_applies_param_fixpoint`, `compute_returns_param_fixpoint`, `rescan_applied_param_forwarders` (3378–3395).
4. Trait env checks (3399–3409).
5. Interprocedural label summaries: `compute_tainting_fns`, joint `compute_sink_summaries_joint` for `is_sink` and `is_egress_sink` (free fn + method), `compute_param_return_taint`, `compute_secret_fns`, `trifecta::compute_leg2_fns` (3413–3445).
6. `effects::compute_fn_effect_rows` (3449), `capability::program_summary` (3452).
7. `collect_items` → `analyze_function` per fn/method (3453, 5433, 5593).
8. Any `ctx.diagnostics` → `Err(joined messages)` (3471–3487); else `TypedIR` with `solver_obligations` (3514–3525). Obligations are discharged later by `SymbolicEngine::check_obligations` (15273).

### 2.2 Per function (`analyze_function`, 5593–~6612)

Resets function-local solver sets (5625–5630), builtin-shadow marks (5639–5673), seeds param `ScopeBinding`s (5787–5834: `fn_identities = Unknown`, `builtin_gate_tags = Unknown`, `secret = is_secret_type(ty)`, `tainted = is_tainted_type(ty)`), models int params / seeds `requires` as assumptions (5836–6006), collects name-keyed write sets (`reassigned_roots`, `struct_write_disqualified`, `shadowed_lets`, 6008–6063), then `analyze_stmts` (6067). After the body: implicit-flow return check (6080), `capability::check_linearity` (6095), declared-vs-inferred effects (6114–6150), transitive rows (6153–6212), open-row-in-verified (6214–6238), trifecta (6240…), `ensures` at every return (6368…), wrap-safety (6551…).

### 2.3 Main entry points

| Entry point | Location | State / role |
|---|---|---|
| `SemanticContext` | struct 2975–3349 | ~70 fields. Almost every table is `BTreeMap<String, …>` / `BTreeSet<String>` keyed by **function name, bare method name, or variable name**: `fn_params`, `fn_contracts`, `fn_carrier_items`, `fn_sole_return`, `fn_returns_param`, `param_sinks`, `param_egress`, `param_return_taint`, `tainting_fns`, `secret_fns`, `method_*` twins, `fn_effect_rows`, `solver_int_vars/float/string`, `symbolic_widths`, `reassigned_roots`, `shadowed_lets`, `annotated_vars`, `known_bindings`. |
| `ScopeBinding` | 183–233 | Per-name scope entry (`BTreeMap<String, ScopeBinding>`). Carries `info: BindingInfo` (`ty: Option<String>`, `tainted`, `taint_source`, `span`), `closure_arity`, `closure_lambda` (an `Expr` copy), `field_closures` (path → `Expr`), `fn_alias: Option<String>`, `fn_identities: FnIdentitySet`, `field_fn_identities` (path → set), `builtin_gate_tags`, `field_builtin_gate_tags`, `taint_label`/`secret_label` (lattice) + legacy `secret: bool`. Constructed by struct literal at **21 sites** in `mod.rs`. |
| `FnIdentitySet` | 318–369 | `Known(BTreeSet<String>) | Unknown`; `Unknown` annihilates `union`; `into_singleton` is the contract policy (only a singleton discharges). |
| `BuiltinGateTags` | 371–429 | Same shape for builtin values (`Capability`, `TaintSource`, `Leg2Source`, `SecretSource`, `IntegritySink`, `EgressSink`). |
| `fn_alias_of` / `_d` | 453–640 | Single preferred name; joins pick the "dangerous" branch (`join_fn_alias` 697). Used by label lanes. |
| `fn_identities_of` / `_d` | 722–954 | Set-valued identity spine. Used by contracts, gates, sealedness. Depth bound `FN_ALIAS_MAX_DEPTH = 8` (470) → `Unknown`. |
| `analyze_stmts` | 9585–12281 | Statement driver. Per statement it runs **separately**: `analyze_expr_effect` (effects/caps/sinks), `check_expr_semantics` (types), `discharge_calls_in_expr` (contracts), solver-fact seeding, label updates. Branches: clone scope → analyze → `restore_block_scope` (7215) → `merge_taint_over` (7231) + `merge_fn_alias_over` (7291). |
| `analyze_value_block` | 9565–9583 | Match/if-let arm bodies via `analyze_stmts`. |
| `analyze_expr_effect` | 13018–14732 | Expression-level effect/capability/sink/egress/interproc checks. Lambda bodies deliberately opaque at definition (14726–14730); charged at application through `closure_lambda`. |
| `walk_block_effects` | 14993… | Separate effect-only walker for value-position blocks (doc 14981–14992). |
| `expr_source` | 24522–25116 | Shared taint/secret source finder, parameterized by `SourceLane`. Returns `Option<String>` (first source found). |
| `walk_block_labels` / `walk_block_taint` / `walk_block_secret` | 23879 / 24084 / 24104 | Shared value-block label walker via `BlockLabelDomain` (23707). |
| `discharge_call_requires` | 7634–7869 | Direct callee `requires` → `requires@callee:smt` obligations in int/FP/QF_S/strlen lanes. Returns `bool` "all checkable". |
| `discharge_carried_call_requires` | 7875–8190 | Carrier-site discharge from `fn_carrier_items`, budget 256 frames (7873). |
| `discharge_resolved_call_requires(_d)` | 8371–8397 | Singleton identity → direct + carried. |
| `discharge_method_requires` | 7447–7500 | Bare-method-name contract; temporarily inserts `<method>m` into `fn_contracts`/`fn_params` (7477–7496). |
| `discharge_calls_in_expr` | 8594–9093 | Expression walk for contracted calls with scoped path conditions (`push_branch_path_condition` 9108). |
| `push_ensures_obligations` | 19962… | Postconditions; unmodelable → refused with code (20081–20158). |
| `SymbolicEngine::check_obligations` | 15273… | Solver drive; `requires-unresolved@` short-circuits to UNDECIDED (docs 16362–16380). |

### 2.4 How lanes share or duplicate traversal

| Lane | Walker(s) | Identity of "what is called" | Notes |
|---|---|---|---|
| Effects (direct) | `analyze_expr_effect` | `sink_callee` = `scope[callee].fn_alias` else raw name (13130–13133) | Name-keyed builtin arms (`"shell"|"exec"|…`, 13141). |
| Effects (transitive) | `effects.rs` own walker + fixpoint | Lexical `Scope` stack: local shadows global (effects.rs:120–148, 272–276) | **Free functions only** (`collect_fn_params_bodies`, effects.rs:431). |
| Capabilities (linear) | `capability.rs` own walker | `CapMap` by var name | Intraprocedural + named interproc shapes. |
| Integrity/taint | `analyze_expr_effect` sink arm (13280–13330) + `expr_source(…, Taint)` + `walk_block_taint` + summary walkers (`body_param_sinks` 27165, `body_param_returns` 28191, `fn_returns_taint` 26514) | `fn_alias` (single) + `fn_identities` (set) in `expr_source` Call arm (24567–24593) | |
| Confidentiality/secret | Same functions with `SourceLane::Secret` / `BlockLabelDomain::Secret`, `compute_secret_fns` | same | Sharing via `SourceLane`/`BlockLabelDomain` enums is real; the summary walkers are still separate per lane. |
| Contracts | `discharge_*` + `contract_carrier.rs` own collector (`Collector`, `FnLikeVisitor`) | `fn_identities_of` (global-first) or `carrier_identities` (scope-first, 8207–8263) | Separate expression walk from the effect walk over the same statements. |
| Trifecta | `trifecta.rs` own walker | own `collect_fn_aliases` (trifecta.rs:63–104): flow-insensitive, last-write-wins, `let x = Var` only | |
| Types | `check_expr_semantics` (23053), `infer_expr_type_scoped` (22488), `ty::InferEnv` | strings | |

At least **four parallel representations of "which callable is this value"** exist on one binding
(`fn_alias`, `fn_identities`, `closure_lambda`/`field_closures`, `builtin_gate_tags`), plus two
more outside `ScopeBinding` (`trifecta::collect_fn_aliases`, `contract_carrier::Denot`/alias map).
The `ScopeBinding` doc (213–222) states the split explicitly ("Existing label-lane consumers
continue to use `fn_alias` unchanged"; "eta expansion … loses the original set-valued identity").

### 2.5 Where types are strings, where names drive resolution

| What | Evidence |
|---|---|
| Binding type is `Option<String>` | `BindingInfo.ty` (50); param types copied as raw strings (5796–5798) |
| Unannotated param type is the empty string | frontend `parse_params`: `let mut ty = String::new()` (frontend/mod.rs:3127–3131) |
| Unannotated fn return type is `""`, filtered as unknown | `place_struct_type` Call arm `.filter(|t| !t.is_empty())` (9253–9257) |
| Structured `Ty` exists but is unused by the checker | `ty.rs:8`, `ty.rs:29`; `grep -c 'ty::Ty\|Ty::parse' mod.rs` = 0 |
| `is_secret` is a substring test | `ty.rs:474–475` `contains("secret<")` (so `list<secret<T>>` labels the whole binding) |
| Field qualifier lookup keyed by the **type string** of the base | `declared_field_type` / `place_struct_type` (9189–9287) |
| SMT variables are source names | `smt_var(name) = "anb_" + name` (22714–22716); soundness under shadowing relies on `invalidate_binding_facts` (17182), `drop_written_after_scope` (21313), `havoc_loop_written` (21378) removing facts by name |
| Write sets are name sets, body-wide | `collect_assigned_roots` → `reassigned_roots` (6011–6012), `shadowed_lets` (6062–6063); also used by `contract_carrier` (contract_carrier.rs:116–122) |
| Branch-join identity is the binding **span** | `merge_taint_over` / `merge_fn_alias_over` compare `info.span` (7240–7247, 7310–7314) |
| Global name before local scope | `fn_identities_of_d` Var arm (740–749); `fn_alias_of_d` Var arm (484–491); `closure_arity_of` (440–443); `carrier_mentions_function` (8270–8274) |
| Bare method name, receiver type ignored | `method_contracts`, `method_sole_return`, `method_param_sinks`, … (3102–3267); `method_sole_return.entry(name).or_insert` — first impl wins (4011–4014) |
| Free-fn map looked up by a method's bare name | in-code self-report at 3242–3246; `fn_effect_rows.get(name)` without an `is_method` gate (6166–6168) |
| `annotated_vars` / `known_bindings` never cleared between functions | inserted at 9694–9697, 5831; no `.clear()` in `mod.rs` (only `reassigned_roots`/`struct_write_disqualified` are cleared, 6011/6013) |

---

## 3. Soundness / precision risk inventory

Direction legend: **UNSOUND** = can accept a program that violates at runtime; **PRECISION** =
over-rejects or loses a proof; **REFUSES** = explicit UNDECIDED/diagnostic.

### (a) Unannotated formals / interprocedural qualifier info (D9)

| # | Evidence | Direction |
|---|---|---|
| a1 | Unannotated param → `ty = ""` (frontend/mod.rs:3127); param binding `ty: Some("")`, `secret = is_secret_type`, `tainted = is_tainted_type` → both false (5794–5822). | Formal enters the body **Clean**, not `Unknown`. |
| a2 | `expr_source` FieldAccess arm: base source, else `declared_field_type(base, field)` (24733–24745) → `place_struct_type` Var arm reads `scope[v].ty` (9232) → `""` → no nominal head → `None`. So `fn leak(s) { print(s.k) }` with `struct S { k: secret<…> }` sees no declared qualifier. | **UNSOUND** (docs: CLAIMS row 9, "D9 … NOT closed", CLAIMS.md:1575; evidence doc line 59) |
| a3 | No interprocedural type propagation into formals: `fn_params` stores only declared param type strings (3885–3888); nothing records caller argument struct types per formal. Label summaries (`param_egress`, `param_sinks`, `param_return_taint`) are keyed by formal index and fire only when the **caller's argument expression** already carries a label (13400–13600) — a declared-field qualifier is only materialized at a field read, so it is not on the caller's argument. | Mechanism behind a2 |
| a4 | Param `fn_identities` and `builtin_gate_tags` seeded `Unknown` (5815–5818); combined with the item-11 rule (i) a formal is never treated as carrying a contracted function inside its own body — the design relies on carrier substitution at the caller. | By design; see (i) |
| a5 | Params are not in `annotated_vars` (only `let` inserts, 9697), so the `ANUBIS_SECRET_TO_PUBLIC` assign rule (10925–10939) does not apply to an annotated public-typed **param** reassigned a secret. **(candidate)** | Coverage gap of that rule |

### (b) Place-assigned taint sink-argument carrier

| # | Evidence | Direction |
|---|---|---|
| b1 | Integrity sink check `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` exists in the `Expr::Call` arm only (13280–13313), keyed by `is_sink(sink_callee)` or the **local binding's** `builtin_gate_tags` (13137–13139, 13282–13284). | |
| b2 | `Expr::CallExpr` arm (field/index callee: `b.f(p, x)`, `xs[0](p, x)`) charges `builtin_gate_tags_of(callee)` via `charge_applied_builtin_gate_tags` (14212–14214), which is a **no-op for `IntegritySink`/`EgressSink`** (1926–1928). It then checks `param_egress`/`param_sinks` only for a stored **user** fn found in `field_closures` as `Expr::Var` (14220–14240). No arm runs the direct builtin sink value-flow check on the arguments of a place-stored sink builtin. | **UNSOUND (candidate)**: `b.f = write_file; b.f(p, input())` gets the `fs.write` capability charge but no taint-to-sink check. Under a declared `uses(fs.write)` the capability charge passes. Matches the open residual "direct `obj.f()` sink-direction bare-builtin row" (CLAIMS.md:1556–1560) and "taint sink-argument carrier remain[s]" (evidence doc line 60). |
| b3 | HOF named-callable path skips any name present in scope (`if !scope.contains_key(fname)`, ~13680), and resolves a local only through `closure_lambda` (13665–13669). A local bound to a sink **builtin** (`let w = shell; each(xs, w)`) has no `closure_lambda`. **(candidate)** | Possible UNSOUND; needs repro |

### (c) Function values in local containers, fields, returns, joins

| # | Evidence | Direction |
|---|---|---|
| c1 | `could_carry` looks only at the top-level `fn_identities` (+ syntactic function names): `Known(non-empty)` or `carrier_mentions_function` (7908–7910). `let xs = [f]` gives `fn_identities_of(ArrayLiteral) = Known(empty)` (937–939); the callable lives only in `field_fn_identities`. So `app(xs)` early-returns `true` (7911–7916). | **UNSOUND** (docs: residual, comment 7898–7900; evidence doc "Residuals") |
| c2 | Carrier `Return` item is only escalated when the callee is **not** in `fn_returns_param`/`fn_sole_return` (8156–8171); the caller-side join resolution for returned values is `fn_identities_of` → joins go through `into_singleton` (359–368), so a returned join `{f, g}` discharges nothing (8390–8392). | **UNSOUND** (docs: "item-10 join lane") |
| c3 | Branch joins merge `fn_alias`, `fn_identities`, `field_fn_identities`, gate tags (7291–7397) but **not** `closure_lambda`, `field_closures`, `closure_arity`. After `restore_block_scope` the pre-branch values survive. The Assign arm sets `closure_lambda = None` for any non-lambda, non-Var RHS such as an `if`-expression of lambdas (11398–11402). Lambda bodies are opaque at definition (14726–14730), so their effects are charged only via `closure_lambda` at application (13244–13257). | **UNSOUND (candidate)**: `let g = |x| x; if c { g = |x| shell(x) }; g(y)` may apply the stale lambda. Transitive row goes `open`, and open is accept-biased in Safe (6158–6161). |
| c4 | `fn_identities_at_path_expr` falls back to the union of all known paths when the exact path is missing, but returns `Unknown` when the root has no entries (961–975). Combined with (i), Unknown means no contract discharge. | Precision → accept (item-11 rule) |
| c5 | `method_sole_return` is first-impl-wins (4011–4014) and read by bare name in `fn_identities_of_d` / `fn_alias_of_d` (910, 588). A second impl's returned callable under the same method name is never considered. **(candidate)** | Possible UNSOUND |
| c6 | Returned-closure resolution uses the callee's sole return, and a recursive builder's container literals (`fn_recursive_container_ids`, 796–811; 842–858). Both are bounded, add-only heuristics; a return shape other than sole-return / forwarder / recursive-literal yields `Unknown` (939–941). | Precision → accept under (i) |

### (d) Direct-call `requires` paths that emit no obligation

| # | Evidence | Direction |
|---|---|---|
| d1 | In `discharge_call_requires`, an unmodelable clause sets `all_requires_checkable = false` and calls `carrier_unresolved_clause` (7832–7848). That function **returns immediately when `ctx.carrier_origin` is `None`** (8340–8343), which is the case for every direct call outside a carrier frame. No obligation, no diagnostic. | **UNSOUND** (confirmed in code). The callee assumes its `requires` in its own body (`seed_requires_fact` 18462) and contracts are "NOT runtime-enforced" (20082). Doc comment admits the fail-open (7406–7411). Compare: an unmodelable `ensures` is refused (20081–20158). |
| d2 | The same happens on the strlen "not covered" branch (7825–7831). | **UNSOUND** (same) |
| d3 | Arity mismatch → `return true`, no obligation (7643–7645; carried 7886–7888). | Silent (handled "elsewhere" per 7418 comment; not verified) |
| d4 | `discharge_resolved_call_requires_d`: depth > 8 → `true` (8388–8390); non-singleton identity → `true` (8391–8393). | **UNSOUND** for joins/unknown callees |
| d5 | The returned `bool` is used only to gate `ensures` specialization at a `let` (10237–10254); in `discharge_calls_in_expr` it is discarded (8602–8608, 8618). | By design; not a refusal channel |
| d6 | `discharge_method_requires` resolves by bare method name; ambiguity is refused (`ANUBIS_AMBIGUOUS_METHOD_CONTRACT`, 7456–7467), then routes through d1 via the temporary `<method>` key (7477–7482), so unmodelable method preconditions are silently dropped too. | **UNSOUND** (same as d1) |

### (e) Mutable/shadowed vars, stale branch facts, loop-carried state

| # | Evidence | Direction |
|---|---|---|
| e1 | Solver identity is `anb_<name>` (22714). Correctness under shadowing depends on every rebinding site calling `invalidate_binding_facts` (17182–17197); `LetPattern` does (10292–10298), for/while-let binders do (11781–11787, 12205–12207). | Discipline, not structure |
| e2 | `drop_written_after_scope` and `havoc_loop_written` clear only **int** modelability (`clear_binding_modelability`, 21332–21334, 21392–21394). Float/string vars stay "modelable" but their facts are removed, which leaves them unconstrained. `invalidate_binding_facts` clears all three lanes. | Asymmetric. Appears sound (unconstrained), **(candidate)** |
| e3 | `discharge_calls_in_expr` Block arm: if any block-local `let` name already has solver state, the **whole block is skipped** (8926–8927). No obligation, no refusal. | **UNSOUND** (silent defer, admitted in comment 8920–8925) |
| e4 | `walk_block_effects`: "a loop-carried label escaping to the block tail is a named fail-open residual" (14989–14991). | **UNSOUND** (admitted) |
| e5 | `contract_carrier` stability is name-based and body-wide (`collect_assigned_roots` + method mutators, contract_carrier.rs:116–122). Sound over-approximation; precision loss (docs: loop-carried mutation refuses UNDECIDED). | PRECISION / REFUSES |
| e6 | `annotated_vars` and `known_bindings` persist across functions and shadowing (see 2.5). An inferred `x` in function B that shares a name with an annotated `x` in function A is held to B's current `ty` and is not flow-updated (11479–11497), and `ANUBIS_SECRET_TO_PUBLIC` fires on it (10925–10939). `known_bindings` suppresses the unknown-variable check for names bound in earlier functions (9623). **(candidate)** | PRECISION (over-reject) + diagnostic gap |
| e7 | `merge_taint_over` merges root labels only; per-field label state does not exist (whole-binding granularity; `apply_container_mutation_taint` 9360). | Over-approx, PRECISION |

### (f) Application-form coverage

| Form | Parsed as | Contracts (`discharge_calls_in_expr`) | Effects/sinks (`analyze_expr_effect`) |
|---|---|---|---|
| `f(x)` global name | `Call{callee:String}` | `fn_identities_of(Var)` global-first (8600–8606) | `sink_callee` alias-resolved (13130) |
| `g(x)`, local alias | `Call` | via `scope[g].fn_identities` **unless `g` is also a global name** (see h) | `fn_alias` / gate tags (13130–13139) |
| `b.f(x)` / `recv.m(x)` | `CallExpr{FieldAccess}` | **method only** — `discharge_resolved_call_requires` explicitly skipped for FieldAccess (8617–8619); field-stored contracted fn not discharged | capability tags + user-fn `param_*` via `field_closures` (14212–14240); no builtin sink check (b2) |
| `xs[0](x)` | `CallExpr{Index}` | resolved set → singleton only | tags + `field_closures` |
| `mk()(x)` returned closure | `CallExpr{Call}` | `fn_identities_of` Call arm (756–898) → singleton only | `resolve_closure_value` (12602) path |
| `(|x| g(x))(1)` | `CallExpr{Lambda}` | Lambda identity = `Known(empty)` (924) → nothing; lambda body is `_ => {}` in discharge (9090–9092) | Lambda body opaque unless bound |
| builtin HOFs | `Call` to `map`/`each`/… | carrier escape "is passed to a builtin" only inside carrier frames (8150–8152) | `effects::higher_order_closure_args` table (effects.rs:94–111). A missing name "under-fires … SILENTLY" (effects.rs:88–90). `apply`/`call`/`compose` are listed. |

`fn_identities_of_d` resolves only `identity`/`secret_source`/`pop`/`last`/`get`/`remove`/`fold`/`reduce`
(759–783). Any other builtin result goes to `Unknown` (939).

### (g) Missing expression positions

| Position | Evidence | Effect |
|---|---|---|
| Statement-level `match` arms | `obl_mark` taken (10673), arms analyzed, then `ctx.solver_obligations.truncate(obl_mark)` (10719) | **All obligations from arm bodies are dropped**, including direct `requires@` and carrier `requires-unresolved@` obligations. **UNSOUND** for contracted calls in arms (the stated rationale, 10667–10672, is arm-body `assert`s) |
| Statement-level `if let` then/else | `obl_mark` (10771) … `truncate` (10801) | Same as the row above |
| `if let` expression, THEN branch | `discharge_calls_in_expr` IfLet arm discharges only scrutinee + else (9084–9089) | Silent defer (comment 9078–9083) |
| Lambda bodies | `_ => {}` (9090–9092) | Silent |
| Value blocks that shadow a modeled name | e3 | Silent |
| Conditions / `for` bounds | `discharge_calls_in_expr(cond)` 11538/11633; `start`/`end`/collection 12108–12112 | Covered |
| `&&`/`||` RHS | scoped path condition (8633–8650) | Covered |

### (h) Global-name-before-scope resolution

| Site | Code |
|---|---|
| `fn_identities_of_d` Var | `if ctx.fn_params.contains_key(name) { one(name) } else if let Some(b) = scope.get(name) …` (740–744) |
| `fn_alias_of_d` Var | `fn_params || is_sink || is_egress_sink` before `scope` (484–491) |
| `closure_arity_of` Var | `ctx.fn_params.get(n)…or_else(scope)` (440–443) |
| `carrier_mentions_function` | any var in `fn_params`, scope ignored (8270–8274) |

`carrier_identities` corrects this for carrier frames only: scope-first, and `Unknown` for a
compound expression that mentions a shadowed global (8207–8263, "review N5"). Every direct
contract discharge (`discharge_resolved_call_requires` → `fn_identities_of`, 8391), identity
resolution for label lanes, and arity checks still resolve global-first. That comment and
effects.rs (272–276) say the runtime resolves the local first. I did not independently read the
runtime resolution order. Consequence **(candidate)**: a local/param `f` holding contracted `g`,
where a global `f` also exists, discharges the global `f`'s contract (or none) instead of `g`'s.

### (i) Contract-carrier "Unknown identity is accepted" and `function_like_formals` over-marking

| # | Evidence | Direction |
|---|---|---|
| i1 | `could_carry` requires `Known(non-empty)` or a syntactic function name (7890–7916); an `Unknown` argument skips the whole callee. | **UNSOUND by policy** (item 11; documented residual) |
| i2 | Apply site resolving to `Unknown` is a no-op (8127–8129). | Same policy |
| i3 | `carrier_escape`: `Unknown` → not relevant → `true` (8288–8291). | Same policy |
| i4 | `Unknown` arises from `Try`/`Other` (953), depth > 8 (736–738), unresolved builtin results (939), unannotated/untyped formals (5815), `fn_returns_param` arity mismatch (788), and any unresolved container path (c4). All of these become "accepted" in the contract lane. | Scope of i1 |
| i5 | `function_like_formals` over-marks: **every bare formal passed as an argument to any call** (contract_carrier.rs:1292–1303), every match/if-let scrutinee (1347–1372), every returned/stored formal. Its own comment calls this safe because `could_carry` filters (1292–1299). | PRECISION (possible spurious carrier sites/escapes; relies on i1 to stay quiet) |
| i6 | Under-marking candidates in the same function **(candidate)**: `mark` fires only on `Expr::Var` (1141–1147), so a formal wrapped in a non-Var argument (`h(if c {g} else {k})`, `return if c {g} else {k}`, `Tainted`/`Declassify`-wrapped) is not marked. `collect_alias_bindings.expr_roots` handles `Var` and `If` only, not `Match`/`Block`/`IfLet` (1159–1169). Its `walk` does not visit `LetPattern` or expression-embedded lets (1170–1207). `contract_carrier` doc says unknown callees are over-approximated. These rows are the exceptions. | Possible UNSOUND; needs repro |
| i7 | `carrier_unresolved` dedups by obligation name (8356–8360). Distinct unmodeled clauses can collapse into one (docs, "Known diagnostic limitation"). | Diagnostic only |

### (j) Facts dropped silently vs refused

| Refused (explicit) | Dropped silently |
|---|---|
| Carrier budget exhausted → `requires-unresolved@` (7943–7954) | Direct/method unmodelable `requires` (d1, d2, d6) |
| Carrier unresolved clause/expr **inside a carrier frame** (8326–8356) | Non-singleton / depth-limited identity at a direct call (d4) |
| `ANUBIS_AMBIGUOUS_METHOD_CONTRACT` (7456) | Statement-level match/if-let arm obligations truncated (g) |
| Unmodelable `ensures` → specific code (20081–20158) | IfLet-then / lambda / shadowing value block in discharge (g, e3) |
| `ensures` over reassigned/shadowed param → `ANUBIS_CONTRACT_UNPROVABLE` (6393–6400) | `expr_source` returns `Option<String>`: `Other`/leaf → `None` = Clean (25105–25110). An unresolvable expression cannot be `Unknown` at the expression level, even though `SecurityLabel::Unknown` exists (security_label.rs:75–96) |
| Open effect row in `--verified` → `ANUBIS_EFFECT_OPEN_IN_VERIFIED` (6214–6238) | Open effect row in Safe: accept-biased (6158–6161); trifecta legs under-approximate (6263–6266) |
| Solver `unknown` → UNDECIDED (16355–16360) | `charge_applied_builtin_gate_tags` no-op on `Unknown` and on sink tags (1916–1928) |
| `requires-unresolved@` → UNDECIDED without solver (16362–16380) | HOF table miss "under-fires … SILENTLY" (effects.rs:88–90) |
| | Unknown-label at sink is only shadow-logged (13316–13331, 13374–13386). `set_taint_label` maps `Unknown` to tainted (272–278) at the **root binding** level, so roots fail closed |

---

## 4. Candidate decomposition

### 4.1 Coherent modules already present as contiguous regions of `mod.rs`

| Proposed module | Current range(s) in `mod.rs` | Content |
|---|---|---|
| `identity.rs` (callable resolution) | 318–470, 473–1110, 1942–2940, 12602–13017, 14741–14992 | `FnIdentitySet`, `fn_alias_of*`, `fn_identities_of*`, container closure/identity collectors, `resolve_closure_value`, `resolve_closure_arg`, eta expansion |
| `gate_tags.rs` | 371–430, 1116–1941 | `BuiltinGateTag(s)`, seeding, path projection, `charge_applied_builtin_gate_tags` |
| `surface.rs` (registration + summaries) | 3529–4741, 12368–12601, 19365–19962, 26383–28542 | `register_program_surface`, alias scans, forwarder/applies/returns-param fixpoints, taint/secret/sink/egress/param-return summaries |
| `contracts.rs` (call-site discharge) | 7424–9135, 19962–20260 | `discharge_*`, `carrier_*`, `match_arm_pattern_fact`, `rename_binding`, `push_branch_path_condition`, `push_ensures_obligations` |
| `smt_encode.rs` | 17096–19004, 20260–20450, 22714–22745 | modelability predicates, int/float/string/strlen encoders, `smt_var`, `mangle_field` |
| `solver.rs` | 15253–17095 | `SymbolicEngine`, Z3/native drive, certificate coverage, counterexample format |
| `wrap_safety.rs` | 6612–7214 | AoRTE-lite overflow obligations |
| `loops.rs` | 17165–17330, 21313–22114 | fact invalidation/membership, loop havoc, invariants |
| `labels.rs` (value-flow) | 9360–9564, 20449–20810, 23584–26383 | container mutation taint, implicit-flow checks, `BlockLabelDomain`, `expr_source`, `SourceLane`, seeders, `ReturnSummaryLane`, `body_returns` |
| `typing.rs` | 22415–23584 | return-type checks, `infer_expr_type_scoped`, generics/bounds, `check_expr_semantics`, exhaustiveness |
| driver (stays `mod.rs`) | 3351–3528, 5343–6612, 9565–12281, 13018–14741 | `typecheck_ex`, `analyze_function`, `analyze_stmts`, `analyze_expr_effect` |

Only the source lanes are already partly parameterized, through `SourceLane` and
`BlockLabelDomain`. The interprocedural summary walkers (`body_param_sinks`, `body_param_returns`,
`body_returns`, `fn_returns_taint`/`fn_returns_secret`) are still separate traversals. So are
`effects.rs`, `capability.rs`, `trifecta.rs` and `contract_carrier.rs`.

### 4.2 Explicit representations that would replace string/name reasoning

| Representation | What it replaces (existing evidence) | Existing precedent to build on |
|---|---|---|
| **BindingId** from one resolver pass (each `Var` occurrence → `Local(id)` / `Global(FnId)` / `Builtin(id)`) | name-keyed `scope` maps; span-as-identity at joins (7240–7247); `anb_<name>` SMT symbols (22714) + invalidation discipline (17182, 21313, 21378); global-first lookups (h); cross-function `annotated_vars`/`known_bindings` | `effects.rs` `Scope` stack (120–148); `carrier_identities` scope-first rule (8207–8225) |
| **FnId** with separate free/method namespaces (`Free(name)`, `Method{impl_ty, name}`) | bare-method-name maps (3102–3267); first-impl-wins `method_sole_return` (4011); free-fn row looked up for methods (3242–3246, 6166) | `method_contracts` `Option` ambiguity marker (3102, 7456) |
| **Place path** `{root: BindingId, proj: [Field(s) | Index(Const k | Dyn)]}` | dotted string paths and `*` wildcard in `field_fn_identities`/`field_closures`/gate-tag keys (2366 `flatten_access_path`, 1547 `prefixed_gate_path`); `mangle_field` string symbols (22733); name-level `assign_target_root` (17102) | `field_place_path` (9149) already returns `(root, Vec<String>)` |
| **One abstract callable value** `{user: Set<FnId> ∪ ⊤, builtins: Set<BuiltinId>, lambdas: Set<LambdaId>}` stored per place | the four parallel callable fields on `ScopeBinding` (183–233) and the two external alias maps (trifecta.rs:63; contract_carrier.rs:1129) | `FnIdentitySet` / `BuiltinGateTags` already have the right lattice shape (`Unknown` as ⊤) |
| **Callable summary per FnId** (params+types, requires/ensures, carrier items, param→sink/egress/return-label, returns-param, sole-return, effect row, cap facts) | ~25 name-keyed `BTreeMap`s on `SemanticContext` (3063–3348) filled by separate walkers | `fn_carrier_items` (3079) already bundles formals + items |
| **Structured types on bindings** (`Ty`, with an explicit `Unknown`/`Dynamic`) | `info.ty: Option<String>`, `""` for unannotated (frontend/mod.rs:3127), substring `is_secret` (ty.rs:474) | `ty::Ty` enum + `Ty::parse` (ty.rs:29…) exist, unused |
| **Expression-level label result** `SecurityLabel` instead of `Option<String>` | `expr_source` returns `Option<String>`, so Unknown collapses to None (24522–24530, 25105–25110) | `SecurityLabel` (security_label.rs:75) already used at root bindings |
| **Typed obligation with provenance**: `enum Obligation { Requires{callee: FnId, site, clause}, Ensures{…}, Assert, WrapSafety, Unresolved{reason} }`, with discharge returning `Discharged | Emitted(ids) | Refused(reason)` instead of `bool` | name-prefix protocol `requires@…`, `requires-unresolved@…` (16362–16380); SMT-string assumptions/vars (`SolverObligation`, 93–115); `bool` returns that are discarded (d5); `truncate(obl_mark)` rollback (10719, 10801) | `UNRESOLVED_REQUIRES_PREFIX` path already shows that "refuse, don't drop" works through all consumers (evidence doc, "The fix") |
| **Consumers dispatch on `carrier_class`** | `carrier.rs` is total but unused | `carrier.rs:61` |

The two rows that matter most for the mission items above:

- a `Refused` variant that callers cannot drop would convert d1–d6, e3, and the (g) positions from
  silent to explicit;
- scope-first `BindingId` resolution would retire (h) and make e1/e6 structural rather than
  disciplinary.
