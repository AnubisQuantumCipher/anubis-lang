# Repeatable comparator keys: proposed semantic dependency

Status: **proposal requiring independent review; not implementation approval**.

This document records a read-only design investigation on 2026-09-26. The author
did not build, run probes, edit compiler code, or use git. The parent reported a
reproduced acceptance-regression test; this document does not independently
certify that result. Source references describe the candidate working tree read
during this investigation, not an immutable release receipt.

## Problem and bounded claim

The candidate min/max transfer feeds key labels back into the next incumbent
argument (`compiler/src/middle/ifc2/builtins.rs:690`). That addresses a real
dependency in the runtime: `min_by` and `max_by` call the key function while
comparing incumbent and candidate (`compiler/src/backends/run.rs:3131`). However,
the candidate also raises the incumbent when every callback returns the same
captured secret value. A callback such as `|x| { println(x); s }` then acquires an
artificial secret-dependent argument, although its keys compare equal regardless
of the contents of `s`. `[s]` has the same issue. The required compatibility case
is recorded in `compiler/tests/ifc2_minmax.rs:108`.

The missing fact is relational: repeated applications of the **same fixed
callback instance** return comparison-equivalent keys. It is not a fact about
whether a key is secret, nor whether two abstract values compare equal.
`V::scalar` retains a label and a scalar-kind flag, but no scalar contents
(`compiler/src/middle/ifc2/value.rs:235`, `:273`). Literal evaluation similarly
forgets contents (`compiler/src/middle/ifc2/eval.rs:1757`). Therefore `left ==
right` on `V`, identical labels, a shared abstract pointer, or a memo-cache hit
must never establish runtime key equality.

The proposed claim is limited to normally returning applications whose key
production is modeled by the new summary. It does not establish termination,
remove existing IFC findings, or repair separate shape/order/trap residuals.

## Recommended approach: a callback-relative repeatability summary

Prefer a small, separate semantic summary over adding global scalar identity to
`V` in this slice. Its result is either a proof record describing a repeatable
key, or `Unknown`. Unknown preserves the candidate's conservative argument
feedback. No syntax spelling receives an acceptance exemption.

The proof obligation is: for any fixed concrete callable instance represented
by the analyzed alternative, changing its callback argument or fresh external
observations cannot change its normally returned comparison key. A fixed
captured value may be completely unknown, secret, tainted, or compound. It need
not be a literal or identical across separate executions of the whole program.

The lowering supports this scope. It clones free variables when a lambda is
created, then clones those snapshots into fresh local bindings on every call
(`compiler/src/backends/run.rs:5156` through `:5179`). Mutating a per-call capture
copy does not mutate the next invocation's snapshot. IFC2 records captures at
`compiler/src/middle/ifc2/eval.rs:2385` and reconstructs a per-call scope at
`:1000`. This is the invariant the summary must consume and test.

### Symbolic domain and transfer rules

Use a bounded symbolic value domain, separate from confidentiality labels:

- `Capture(slot)` denotes one frozen slot of the current callback instance.
- Literal terms include their AST/runtime kind and spelling or parsed value.
  No numeric simplification is necessary to establish identical construction.
- An ordered constructor or audited deterministic operation applied to stable
  terms produces a stable term. This includes `[s]` and nested lists. Constructor
  tags, list positions, enum variants, and source order belong to the term.
- Callback parameters, fresh observations, unresolved values, and unsupported
  operations are `Unknown` for repeatability. Two `Unknown` values do not prove
  equality. A parameter must remain varying even if its ordinary `V` happens to
  equal another argument's `V`.

Track lexical local environments through value reads, `let`, ordinary assignment,
blocks, returns, and modeled branches. Variable names alone are not provenance:
rebinding changes their terms and shadowing creates a distinct binding. An alias
of a stable term preserves it. A deterministic mutation of a per-call copy can
produce a new stable term if modeled; otherwise invalidate the mutated value or
return `Unknown` for the entire proof. Never retain the old term after a place
write, `push`, `pop`, `insert`, or `remove` merely because the abstract label did
not change. Existing mutation entry points include `eval.rs:1361`, `:1413`, and
`builtins::mutate`.

For branches, identical stable terms from all possible normal outcomes may be
kept even under a varying condition. Different stable terms require a stable
condition and an explicit conditional term; otherwise the merge is `Unknown`.
Apply this rule to modified locals and early returns, not only the final
expression. Unsupported patterns, loops, exits, or control constructs must make
the proof unknown until their transfer rules are reviewed. In particular, do
not skip an earlier return while inspecting a block's tail.

All `Expr` and `Stmt` variants must be handled exhaustively. Initially unsupported
variants may explicitly yield `Unknown`; an accept-biased catch-all is forbidden.
Keep the supported semantic fragment documented. Normal IFC evaluation still
runs the entire body, including every egress and callback effect.

### Observations and effectful statements

A fresh source cannot become stable because its label is public, because it is
read at the same source location, or because its ordinary analysis is memoized.
The existing builtin table groups time/random results with public constants
(`compiler/src/middle/ifc2/builtins.rs:219`), and separately models input, file
reads, and process-global capability state (`:212`, `:214`, `:221`). These are
label classifications, not determinism evidence.

Mark fresh reads and results of unknown calls as unknown. This includes random,
time, input/network/file/environment observations, crypto key generation,
mutable global state, and a call through a captured closure unless that call has
its own reviewed repeatability summary. A source read once **before closure
creation** and stored in a captured snapshot is different: that snapshot is
fixed during this min/max operation, even though its origin was nondeterministic.

An effectful statement whose result is discarded need not destroy an unrelated
stable return value. For example, the actual lowered `println(x)` statement
does not mutate the captured `s` snapshot. Its arguments and control flow must
still be visited, and its egress must still be checked by ordinary IFC. Admit
such statements only with a reviewed state-transformer rule; unsupported
statements must not silently preserve the proof environment. A conservative
initial implementation can abandon the whole proof after an unmodeled effect.

Existing `EffectRow` is insufficient evidence of repeatability. It records
capability effects and an open tail (`compiler/src/middle/effects.rs:23`, `:29`),
not returned-value dependence or every external observation. Its builtin
classifier returns no capability for names outside its table (`:49`, `:78`).

### Call resolution and interprocedural scope

Resolve calls using the same rules as lowering and IFC2. Ordinary calls check
user functions, then local callable bindings, then builtins
(`compiler/src/middle/ifc2/eval.rs:2016`). Statement-form print and mutation have
special precedence (`:1724`). Recognizing a spelling such as `identity`, `len`,
or `print` without resolving its namespace is unsafe.

Initial implementation may return `Unknown` for user-function, method,
composition, recursive, and higher-order calls. Supporting these later requires
symbolic parameter substitution and a summary that separates returned-value
dependencies from state changes and fresh observations. A helper returning an
input must preserve that input's symbolic dependence. A helper wrapping a
source must retain the source dependence through aliases and nested calls.

The proof is about a fixed runtime callable, not equality between callable
alternatives. A joined `FnSet` may contain different lambdas or capture
snapshots. It is safe to establish repeatability separately for every possible
fixed alternative and require all alternatives to qualify. Their returned keys
need not equal each other's keys: a single min/max execution retains its one
selected callback. However, caller selection and secret callable identity still
affect callback execution and must retain the existing `f.lab` handling
(`compiler/src/middle/ifc2/eval.rs:1073`). Never compare `Capture(slot)` terms
belonging to different callback scopes. An unknown callable alternative prevents
the optimization.

For an initial narrow implementation, accepting only an identified lambda with
an explicit capture environment is reasonable. Supporting `Clo::Summary` later
requires a proof universally quantified over its represented snapshots; joining
the snapshots' ordinary `V`s is not such a proof.

### Compound keys and comparator semantics

Runtime ordering compares lists recursively and otherwise may compare display
strings (`compiler/src/backends/run.rs:1882`). Stable construction must preserve
all data that those operations observe. A whole captured map is stable because
the same concrete snapshot preserves its runtime order. Newly constructed map
terms must preserve source/insertion order; `MapV.known` is a `BTreeMap`
(`compiler/src/middle/ifc2/value.rs:218`) and cannot be used as a canonical runtime
comparison key. Unknown order or an unmodeled mutation invalidates the proof.

Identical stable runtime-value construction is a sufficient equality proof;
general algebraic equality is unnecessary. Avoid using existing structural
equality of abstract values, map key-set equality, or `V::select` as a substitute.
Limit the initial constructor/operation whitelist to reviewed deterministic
lowerings. Floating-point and mixed-kind operations require tests of the actual
ordering behavior before adding transformations beyond identical construction.

## Integration boundary and caches

The consumer should ask whether the fixed callback has a proved repeatable key.
When it does, preserve normal callback analyses and collection-shape PC, but do
not add that key's confidentiality label to incumbent selection. Do not clear
the key's label globally: printing the secret key must still be refused.
Preserve existing argument labels and caller/callback identity labels. This
optimization must not hide a secret-length callback schedule or a secret-chosen
callback body. Keeping the joined candidate abstraction is conservative for tie
selection; no runtime algorithm change is required.

Keep summary caches separate from ordinary IFC value-result memoization.
Existing call keys contain callee, abstract argument values, and PC
(`compiler/src/middle/ifc2/eval.rs:85`, `:682`); equal keys there intentionally
merge different concrete scalar values. Existing summary contexts further join
arguments (`:734`), and recursive solutions use provisional results (`:784`,
`:853`). None is a proof of repeated runtime value identity.

A universal syntactic-semantic lambda summary may be cached by immutable lambda
identity plus its resolved lexical/function environment and analysis version.
Any specialization by callable captures or helper summaries must put those
dependencies in the key and invalidate on change. Callback-local capture atoms
must never escape into an unrelated cached callback instance. An in-progress or
provisional summary is `Unknown`, not repeatable. Bound term depth, term nodes,
visited statements, call depth, and analysis work; exceeding a limit discards the
optimization and preserves ordinary conservative checking. Do not construct an
unbounded hash-consed history of loop iterations or source reads.

## Why global provenance in V is a larger change

A durable optional value-identity field could eventually support broader
precision, but its soundness obligations exceed this patch. It would need:

- identities tied to concrete snapshot scope, never a source site or equal label;
- replacement on every value-changing operation and mutation;
- a must-equality join retaining an identity only when both inputs justify it;
- preservation across clone/release/raise only where those change labels rather
  than runtime data;
- explicit behavior in folding, widening, closure summaries, recursion, stores,
  memoization, and all early equality shortcuts (`value.rs:706`);
- fresh-observation behavior that cannot reuse an identity through a cache hit.

Changing derived equality/hashing on `V` also changes call-context selection and
fixpoint convergence. Adding a field without incorporating it into order/join
and invalidation can instead preserve a false equality fact. A callback-relative
summary avoids making that broad claim in the current fix.

## Required validation before integration

The future implementation owner should provide source-bound receipts for both
the ordinary checker and IFC2-only findings, with direct controls and runtime
transcript evidence where authorized. Required cases include:

- Captured scalar, nested list, alias, and deterministic compound keys preserve
  public argument logging for both min/max operations.
- A value observed before capture is repeatable; a fresh observation inside the
  callback is not granted repeatability merely because it is public or cached.
- Secret-dependent unequal keys, nested secret components, callback-argument
  dependence, and branches choosing different captures remain rejected when
  they change observable callback arguments.
- Rebinding, shadowing, place assignment, collection mutations, early returns,
  and unsupported control flow cannot retain a stale stable-key proof.
- Direct and aliased helpers preserve dependencies; user/local names shadowing
  builtin names do not acquire builtin determinism privileges.
- Joined callbacks, distinct capture snapshots, summarized closures, and
  recursive/provisional contexts do not share unjustified capture identities.
- Constant logging, singleton/pair controls, declassified keys, secret collection
  length, and secret callable selection retain their established behavior.
- Ordered compound keys exercise map source order and duplicate-key behavior;
  unknown structure takes the conservative path.
- Deep terms, large expressions, and recursive helpers hit bounded conservative
  fallback without hanging or claiming a proof from a partial traversal.

## Ownership and review requirement

The lead should assign an implementation owner for the new summary and its
consumer integration, followed by an independent review of lowering parity,
source/observation classification, scope handling, and caches. The lead remains
the build/pin/integration owner. This design author owns no compiler edits in
this slice. The currently overrefusing candidate must remain unintegrated until
the required acceptance regression and security controls have source-bound
validation. This proposal supplies a next dependency, not an approval to merge.
