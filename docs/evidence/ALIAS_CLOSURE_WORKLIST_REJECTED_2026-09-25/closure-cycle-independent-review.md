# Independent closure-cycle prototype review

Author: Codex. Disposition: **reject this prototype for integration**. The lead has already made that decision after bounded check-only differential results confirmed the missing-capture concern raised during this review.

This review performed source inspection only. It did not build, check, test, execute an Anubis program, change compiler source, or reproduce the known recursive crash. Checker observations below were read from the lead's preserved results and logs. Semantic and architectural conclusions are ordinary static reasoning, not JACKAL or formal correctness evidence.

Reviewed inputs:

- `closure-cycle-prototype.diff` and `closure-cycle-prototype-audit.md` in this directory.
- Frozen source `/home/sicarii/.cache/anubis-wt/codex-closure-cycle/compiler/src/middle/mod.rs`, with the associated frontend, whole, security-label, analysis-limit, resource and native-lowering code.
- `closure-capture-controls/results.json` and its preserved source/log files; `closure-cycle-controls/results.json` and source controls.

## Blocking finding: deduplication makes the ordinary capture hole accept

The prototype's key preserves the represented scope, but the ordinary effect traversal does not apply a closure in the environment it captured. Consequently the repeated state is a cycle in the checker’s incorrect application model. Suppressing that cycle removes the analysis-limit refusal without visiting the runtime-reachable captured printer. A collision in Debug text is not required.

The exact original sources are:

- `/home/sicarii/.cache/anubis-review-esc/ordret3fix/alias/t5/g4_selfref_old_prints_stmt.anb`
- `/home/sicarii/.cache/anubis-review-esc/ordret3fix/alias/t5/g5_mutual_old_prints_stmt.anb`

The lead preserved copies and scalar-secret variants in `closure-capture-controls/`.

| Preserved case | Baseline `anubis-ord3b` | Alias `anubis-ord3x1` | Prototype `anubis-codex-closure-cycle1` |
| --- | --- | --- | --- |
| `g4_selfref_old_prints_stmt.anb` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc0, `check passed`, no diagnostics |
| `g5_mutual_old_prints_stmt.anb` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc0, `check passed`, no diagnostics |
| `scalar_g4_selfref_old_prints_stmt.anb` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc0, `check passed`, no diagnostics |
| `scalar_g5_mutual_old_prints_stmt.anb` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc1, `ANUBIS_ANALYSIS_LIMIT` | rc0, `check passed`, no diagnostics |

These values are transcribed from `closure-capture-controls/results.json`, which also records source/log hashes and these binary hashes:

- baseline: `90a21f28c49bd044202698ba889f72bb4d9757b721671b194bf4c7a3b786c855`
- alias: `9e8720dd805bbbd55b29b5d4904fd7b981166094277d8e5e70e7f252d7531ee1`
- prototype: `bb62868e0f1495a12f26499e6499aaa2c6ef6ffe08f1212ab59f470bd1bf6c88`

This is observed checker acceptance of invalid controls, supported by the following static runtime/checker mismatch. It is not a fresh runtime egress witness from this review.

### Capture-to-application trace

Source references in this trace are in the frozen tree.

1. Native lowering explicitly implements value capture. `compiler/src/backends/run.rs:5133` collects free value and local-callee names; `:5160` emits a clone before creating the `move` closure, and `:5166` emits a fresh clone of each captured value per invocation. A later assignment to the outer name cannot replace the captured callable.

2. In g4, `let g = |s| { print(s); 0 }; g = |s| g(s);` creates a wrapper containing the previous printer. The final `g(secret_argument)` therefore invokes that printer. In g5, `g = |s| h(s)` captures the previous printer `h`; the subsequent `h = |s| g(s)` cannot change the `h` already captured by `g`. The final call to `g` again reaches the previous printer. Neither source requires a runtime closure cycle to express the leak.

3. Main statement assignment in `middle/mod.rs:14368` constructs a replacement `closure_lambda` from the RHS. `:14430` calls `whole_captures_of` before mutation; `:14433` replaces the ordinary primary lambda and `:14434` installs whole-lane captures. `:14438` replaces `field_closures`. `whole_captures_of` at `:12245` delegates a literal lambda to `whole::closure_captures`; `middle/whole.rs:4203` snapshots free names from the old scope. This is useful whole-lane data, but it is not an ordinary effect capture environment.

4. `applied_closure_candidates` at `middle/mod.rs:4247` returns the binding's current `closure_lambda` and reserved alternative lambdas in `field_closures`. It does not return a lambda paired with an ordinary captured environment.

5. The direct application consumer at `middle/mod.rs:17461` clones the *application* scope, labels parameters from the actual arguments, and calls `analyze_closure_body_effect`. It never overlays the closure's defining environment. For g4, the inner `g(s)` therefore finds the current wrapper again. For g5, current `g` and current `h` find each other. The old printer is absent from the ordinary successor relation.

6. The prototype at `middle/mod.rs:16581` serializes these body/scope states and stops submitting states already in `seen`. The complete key includes `whole_captures`, but copying those captures into a key does not cause the ordinary effect walker to traverse them. Once the repeated current state is suppressed, the queue empties without reaching the old `print(s)`, and the former refusal disappears.

7. The whole lane cannot serve as a general repair for missing ordinary captures. `whole::call_egress` (`whole.rs:4183`) tracks whole structs and values computed from them. `declared_field_whole` (`:4027`) delegates to the whole-struct type classifier. Ordinary scalar confidentiality, taint and capabilities require their own faithful closure application. The scalar-secret controls establish the regression without depending on aggregate projection behavior.

The earlier negative control `closure-cycle-controls/reject_prior_egress.anb` places reassignment inside a loop. Loop joining (`middle/mod.rs:14869`, `:14883`) preserves alternatives from the incoming state, unlike the straight replacement above. Its semantic rejection therefore does not clear this straight-assignment capture hole. The other finite controls remain useful but are not a substitute for g4/g5 and their scalar twins.

## Required architectural correction

Before adding cycle deduplication, the ordinary effect representation must preserve a closure value together with an immutable, capture-versioned environment. Assignment must replace the outer binding while retaining the old binding version inside already-created closures. Aliasing must carry that same closure value; joins must retain each possible lambda/environment pair. Application must resolve free names from that captured environment and then shadow formals with complete abstract argument bindings.

The captured ordinary binding must retain every input the ordinary consumers need: confidentiality and taint labels, callable identities and alternatives, builtin capability tags, field/container callable state, and the corresponding captured callable versions. A whole-lane `Local` snapshot alone is not a substitute, because it is an abstraction for a different analysis. An explicit faithful conversion would need to demonstrate that it preserves all of those ordinary inputs.

This representation must reach the application consumers that can invoke these values: direct locals, aliases, field/index values, returned/forwarded closures and callbacks. The narrow requirement is consistency of closure-value creation, propagation and application; it is not permission to redesign unrelated summary or contract lanes.

Only after that successor relation is faithful can a scheduler key be justified over closure identity/version, captured environment and abstract arguments/mode. In the finite controls, the traversal must actually reach the captured old printer and emit the appropriate semantic diagnostic. Valid finite wrappers and public captures must still accept. An unresolved capture cannot be treated as an already-completed empty-effect state. A body/name re-entry guard, a larger limit, or an extra Debug field does not repair this defect.

## Debug-key completeness and collision review

I found no concrete formatting collision in the inspected current graph. The audit correctly identifies derived Debug implementations, escaped string leaves, string-backed numeric literals, immutable Rc pointees and ordered maps/sets. The key is retained as a full string, not a truncated digest. No custom Debug implementation or hidden mutable capture field was found in that graph.

That limited representation conclusion does not establish semantic completeness. The blocking finding holds even if serialization is injective: the ordinary transfer function uses the wrong environment. The audit's assertion that each job contains every capture needed for semantic traversal must therefore be withdrawn or qualified to distinguish stored whole-lane captures from captures actually consumed by ordinary effects.

There are also identity and convergence caveats. `pattern_binder_span` (`mod.rs:30694`) embeds a Pattern address, and `seed_effect_pattern` (`:33979`) copies it into binding metadata. Jobs clone expression bodies, so equivalent binders can acquire different addresses. `whole::Env::closure` (`whole.rs:1134`) creates allocation-based closure IDs that are retained in Debug. These details can prevent deduplication of semantically equivalent states and make keys dependent on allocation history. I did not establish a false acceptance from pointer reuse: the span comparisons reviewed belong to merge/shadow helpers outside the effect-only traversal. These are precision/convergence concerns, not an additional confirmed leak.

Debug is not an enforceable stable semantic-key contract. Any future field omission, custom formatter or opaque/mutable field would invalidate the representation argument. A purpose-built structural key with explicit closure/capture versions would make its obligations reviewable.

## Context, outputs, guards and resource boundaries

The per-root synchronous drain is a useful boundary. The reviewed effect call graph reads semantic tables without updating them; local scopes are owned clones. Diagnostics, taint traces and effects are appended, and application-witness flags are monotone outputs consumed after body analysis. I found no distinct bug in the queue's ownership of the shared effects vector or its clearing of `ctx.closure_effect_work` at normal completion.

The audit's statement that all inputs are fixed should nevertheless be narrower: return-value expansion TLS (`mod.rs:27121` onward) includes mutable active-query state, spent budget, trips and memo entries. Analysis-limit TLS also changes. Existing fallbacks are intended to be conservative, but traversal order can alter budget use and refusal behavior. No new false acceptance from these ambient counters was established here.

Duplicate suppression and LIFO scheduling change diagnostic/effect multiplicity and order. Capability enforcement consumes a set (`mod.rs:8129`), while raw effects are copied into MIR/HIR (`:8610`). The latter is an observable output difference requiring deliberate review even after the capture blocker is fixed. This review did not prove artifact equivalence.

The inspected formatter checks `analysis_limit::cut()` before each append and notes return-value trips; a partial failed key is not inserted. A guard reached during drain stays sticky, and frame tokens are dropped on exit. I found no normal path that clears the sticky refusal or publishes a partial queue as a completed summary. This does not rescue the confirmed acceptance: those repeated checker states were skipped before a guard needed to fire.

The scheduler still has significant resource and valid-program refusal risks. Full body/scope strings are retained in `seen`; pending jobs also own cloned bodies/scopes. Allocation-dependent metadata can create fresh keys for equivalent work, and recursive Clone/Debug/drop operations remain. The logical depth recorded at first discovery is traversal-order-dependent. Broad valid workloads can consume memory or trip existing limits even when a more canonical state representation would converge. No performance or stack-safety claim is established by this static review.

Source analysis remains independently recursive. The lead's existing stack-overflow observation in `recursive_closure_source_reports_limit_and_recovers`, recorded in the prototype audit, is not fixed by scheduling ordinary effects. This review did not execute that case.

## Disposition and retained validation value

Keep the prototype and both control directories as rejected-design evidence. The observed restoration of `rv30_valid_n02`, `valid_straight_wrap` and `valid_public_capture`, and the semantic rejections of the other focused controls, show that the prototype changes useful behavior. They do not justify integration in the presence of the captured-printer acceptance regression.

The next implementation should address capture-versioned ordinary bindings first, with g4/g5 and the scalar variants required to reject semantically. Any later scheduling proposal needs its own review of successor completeness, stable identity, conservative unresolved states, output changes, resource refusal and recovery. No replacement guard workaround is proposed here.
