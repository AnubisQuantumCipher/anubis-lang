# Repeatable comparator keys: bounded extension design

Status: **bounded design reviewed with the metadata qualification below; compiler
implementation deferred to a fresh source-bound candidate unit**. This document
does not approve product integration.

This document describes a proposed extension to the withheld min/max candidate in
`/home/sicarii/.cache/anubis-wt/codex-minmax-repeatability`. Source references name
the mutable working tree inspected for this design. No compiler, checker, runtime,
build, or git command was run by this design lane. A separate read-only agent
audited the helper-call lowering; its findings were checked against the source.

## Why the candidate remains withheld

The existing `precision-expansion/manifest.json` records baseline `check passed`
with exit status `0` for `compare.anb`, `unary.anb`, and `helper.anb`.
`precision-expansion/candidate.txt` records both IFC2 findings and full-checker
`ANUBIS_SECRET_EXFILTRATION` for those same controls. The bodies return `s > 0`,
`-s`, and `fixed(s)` after logging the public callback argument; `fixed(s)` simply
returns its parameter. These are actual precision regressions, not successful
negative controls. This document does not rerun or strengthen those receipts.

The proposed fragment restores these cases by modeling their runtime operations.
It does not establish compatibility for every other deterministic Anubis program.
Known verdict flips outside the admitted fragment remain product blockers until
resolved or separately assessed; documenting conservative fallback does not erase
a known regression.

## Invariant and scope

Retain the callback-relative invariant: for every fixed concrete callable instance
represented by the selected lambda, the constructed comparison key is independent
of that callback's varying arguments and fresh external observations. Captures
are universally quantified frozen snapshots. Their IFC labels and joined abstract
values do not establish runtime equality. Distinct fixed callback alternatives
may return distinct keys, but each alternative must qualify independently.

Keep `Stable(term)`, `Varying`, whole-summary `Unsupported`, lexical stores, and
separate normal/return outcomes. This extension remains a single-path interpreter:
branches, loops, place mutations, local closure calls, methods, arbitrary callee
expressions, and builtin calls remain unsupported. It adds only the audited unary
operators, eager comparisons, and recursively modeled eligible direct free calls.

`Unsupported` invalidates the entire proof when the transfer is evaluated, even
if its result would be discarded or an eligible helper ignores the argument.
An already-taken unconditional return prevents evaluation of subsequent code;
that is control modeling, not permission to skip a reachable unsupported effect.

The summary never substitutes for ordinary IFC. Both runtime comparator callback
applications, their egress checks, callable-identity control, collection-shape
control, and continuation lifts still run through the normal interpreter. A
repeatability result only suppresses key-derived incumbent-selection raising;
it does not clear key labels, element labels, or any program-counter label.

## Unary operators and eager comparisons

Add exact tagged term constructors, for example `Unary(op, child)` and
`Compare(op, left, right)`. Operator tags must distinguish every admitted spelling;
operand positions must be preserved. These are construction descriptions, not
algebraic simplifications or evaluations. No arithmetic result is needed.

The unary whitelist is native `-`, `!`, and `~` only. `safe_run_expr` dispatches
these at `compiler/src/backends/run.rs:4648`; negation uses `anubis_neg` with
wrapping integer negation (`:1873`), bitwise complement uses `anubis_bnot` (`:1869`),
and logical negation uses `as_bool` (`:1557`). The referenced conversions are
deterministic over a fixed runtime value (`as_i64` at `:1525`, `as_f64` at `:1539`).
They have no modeled data-dependent failure branch in these implementations.

Unary transfer:

- Evaluate the operand once in the current store.
- Propagate an operand return without applying the unary operation.
- Propagate unsupported evaluation as failure of the whole summary.
- A normal stable operand produces its tagged stable term; a normal varying
  operand stays varying. In particular, `!x` does not become stable because its
  IFC abstract kind or label matches an earlier argument.

The comparison whitelist is `<`, `<=`, `>`, `>=`, `==`, and `!=`. All are eager
calls to `anubis_cmp` (`run.rs:4660`, `:4680`, `:1953`). Equality and ordering use
different runtime algorithms (`:1886`, `:1909`), so preserve the exact operator
instead of translating comparison identities. Floating unordered ordering maps
to `Equal`; never use comparator equality as a transitive equivalence relation.

Comparison transfer:

- Evaluate the left operand in the caller's current store. If it returns, do not
  evaluate the right operand.
- Evaluate the right operand in the store resulting from the left operand.
- Propagate either evaluated operand's unsupported result or return precisely.
- Construct a stable comparison term only when both normal operand values are
  stable. Otherwise produce a modeled varying result and retain the resulting
  store. `x == x`, equal IFC labels, and matching abstract values do not justify
  reducing a varying result to a constant.

Do not enable all `Expr::Binary` variants. `&&` and `||` require conditional
evaluation with conditional stores and exits. Integer `/` and `%` explicitly
panic on zero divisors (`run.rs:1832`, `:1842`). Even if their *normally returned*
keys repeat, a captured-secret-dependent trap can change how many callback
effects occur. Existing IFC binary handling does not supply the missing trap
control proof. Arithmetic, bitwise binary operations, shifts, indexing, casts,
assertions, and other operators remain outside this extension until separately
reviewed. A division or remainder inside an otherwise eligible helper must also
make the whole summary unsupported.

## Immutable helper metadata and resolution API

Current IFC `FnDef` stores name, parameters, body, return annotation, and the
runtime's complete local-name set (`compiler/src/middle/ifc2/eval.rs:55`). Its
registration currently discards effective mode, attributes, and generics
(`:382`). Those omitted fields cannot be recovered from labels or function names.

The registry is immutable during a summary query, but its parameter annotations
are **analysis metadata, not necessarily the original runtime signature**.
`ifc2_findings` directly analyzes the supplied AST (`middle/mod.rs:5106`), whereas
full `typecheck_ex` first mutates its analysis copy through
`infer_unannotated_struct_params` (`:5126`, `:5130`). Lowering uses its own AST, as
the source comment explicitly states. Thus an originally unannotated struct
helper can retain empty annotations in the IFC2-only entry point but acquire
inferred parameter annotations before the full-check IFC pass. An admission test
requiring empty annotations can consequently admit the former and exclude the
latter even though the runtime signature was unannotated in both cases.

Do not describe the proposed registry view as identical to runtime signatures,
and do not use inferred annotations as evidence of runtime guards. The narrow
implementation can conservatively exclude these analysis-annotated helpers, but
a resulting rejection of a valid precision control remains a precision defect;
it is not an expected security negative. Restoring admission requires separately
preserving original signature provenance for the eligibility/boundary decision
while retaining inferred qualifiers for ordinary IFC checking. It must not erase
the analysis annotations or weaken the full checker to make the summary pass.

Extend registration to retain a conservative repeatability eligibility record
from the same resolved `Item::Fn`. The first admitted class should require:

- an ordinary free function in Safe mode, rather than an impl method;
- no generics or specialization dispatch;
- unannotated parameters and no declared return annotation;
- no attributes or other unreviewed boundary transformation;
- a body whose every evaluated transfer is accepted by the semantic summary.

The annotation exclusions are deliberate admission policy, not claims that an
annotation itself causes effects. They avoid executable numeric boundary
transformations and traps: parameter guards/coercions occur at `run.rs:928`, and
return guards/coercions wrap every exit at `:1017`. Broader signature support
requires modeling those transformations and potential failures before admission.
Contracts, effect rows, and a name such as `fixed` or `identity` never prove the
body repeatable. Ordinary contract/effect checking remains authoritative.

Provide a read-only view from `Interp`, conceptually:

```rust
struct FreeFnView<'s> {
    id: FreeFnId,                   // identity in this immutable resolved program
    params: &'s [(String, String)], // analysis annotations; not raw-signature proof
    body: &'s [Stmt],
    locals: &'s BTreeSet<String>,   // exact runtime collect_local_names result
    admission: HelperAdmission,    // EligibleBoxed or explicitly Unsupported
}

// None means absent. Some(Unsupported) still wins namespace resolution.
type LookupFreeFn<'s> = dyn Fn(&str) -> Option<FreeFnView<'s>> + 's;

fn key_is_repeatable(
    params: &[String],
    body: &Expr,
    captures: &[String],
    owner_locals: &BTreeSet<String>,
    free_functions: &LookupFreeFn<'_>,
    budget: &mut SummaryBudget,
) -> bool;
```

This is an API sketch, not compiled Rust. The parent may use a borrowed resolver
trait or another equivalent immutable view. Do not expose ordinary IFC call
memoization, provisional summaries, joined arguments, or mutable `Interp` state.
Build metadata from the existing resolved free-function registry, preserving
module/name policy. Methods stay in their separate registry. Do not reconstruct
a second lossy namespace from accepted helper bodies alone. If original signature
provenance is added, keep it explicit and distinct from these analyzed parameter
types; the API sketch above does not already provide that provenance.

Replace the single expression-return boolean with per-frame resolution context.
For `Expr::Call(name, args)`, first look up a free function, including known but
ineligible functions; only absence permits considering the current frame's
complete owner-local name set, then the builtin namespace. This matches
`run.rs:4690`. A local callable remains unsupported. In the builtin namespace,
only the already-reviewed expression `return` control transfer is admitted.
Builtin `identity`, output expressions, fresh sources, and other names receive
no new exception.

Statement-form output and `return` retain their runtime special precedence even
when a user function or local shares the name (`run.rs:3650`, `:3677`). A direct
statement-form `push` must remain unsupported before user-function resolution:
runtime `emit_safe_run_stmt` intercepts it at `:3635`. Do not accidentally admit a
user function called `push` as the meaning of a `push(...)` statement. A final
expression statement converted to an implicit tail has expression-call semantics,
as dictated by `split_tail_expr`, rather than the original statement dispatch.

## Actual evaluation and arity conventions

There are distinct runtime call conventions. Do not copy IFC's `Vec::resize`
normalization (`eval.rs:2058`) into every helper call and call it lowering parity.

| Call form | Runtime behavior | Initial extension |
|---|---|---|
| Nongeneric direct named free call | Emits every actual in source order into a fixed Rust signature; no padding/truncation (`run.rs:4711`, `:915`) | Require exact declared arity; otherwise unsupported |
| First-class free-function wrapper | Caller evaluates the entire actual vector; wrapper binds declared positions, pads missing values with `Int(0)`, and drops extra values (`:4558`, `:4723`, `:4999`) | Unsupported; retain this explicit future contract |
| Lambda value call | Caller builds actual vector; parameter binding pads missing values (`:5167`) | Nested calls remain unsupported |
| Generic/unboxed direct call | Specialization selection and conversions; actual lowering uses `zip`, so some extra actuals are not emitted (`:4695`) | Unsupported |
| Method call | Separate dispatch, receiver, and arity/evaluation rules | Unsupported |

For an admitted direct helper call, evaluate every actual once, left to right,
before constructing a helper frame. Thread caller stores between actuals. Any
return during actual evaluation belongs to the caller and prevents later actuals
and the helper body from running. Unsupported actual evaluation invalidates the
whole proof even when the corresponding helper formal is unused.

If wrapper support is added later, *first* evaluate all actual expressions according
to that call site's convention, *then* truncate/pad the resulting symbolic values.
A missing argument receives the exact default-zero term; it does not acquire a
capture atom. Never drop an extra argument's effects merely because its value is
discarded by the wrapper. No mismatch is a basis for laundering a rejected direct
call into an assumed wrapper call.

## Helper frames and result substitution

After normal actual evaluation, save the caller's updated lexical store and
resolution context. Enter an isolated helper frame binding each formal to its
actual symbolic value. Stable actual terms remain references to the current
callback's immutable construction terms; varying actuals stay varying. Do not
seed helper formals as new stable captures.

The helper has no implicit access to caller locals or capture names. Lookup and
assignment must stop at the helper frame boundary. A helper parameter or local
named `s` is unrelated to an unpassed caller capture `s`; an unbound read is
unsupported. Local reassignments affect only the helper frame. No new callback
capture atoms are created for a helper call.

Interpret the helper's statement body using the runtime function-tail adapter:
final bare expressions provide the result except statement-only output/return;
trailing ordinary statements and empty bodies yield the default-zero term
(`run.rs:1047`, `:958`). Unsupported trailing branches must not silently become
zero. Existing lexical-block behavior may be shared with an explicit adapter,
but the body is a statement list, not an arbitrary lambda expression.

A helper's normal tail or explicit return is its result. On leaving the helper,
restore the saved caller store and resolution context, then turn the helper's
`Return(v)` into the caller's `Normal(v)`. A `return` encountered while evaluating
the actual arguments never crosses this boundary and must still return from the
caller. This distinction also applies to returns in printed arguments and inside
unary/comparison operands.

This is bounded semantic substitution, not a boolean purity cache. For example,
the same `fixed(v) { v }` body yields stable output for `fixed(s)` and varying
output for `fixed(x)`. A helper that ignores a varying actual can return stable
output if argument evaluation and its entire executed body are supported. A
helper that logs a secret still reaches normal IFC egress checking even if the
helper's returned key is stable.

## Recursion, limits, and caches

Maintain active helper identities separately from the existing IFC recursion
machinery. Calling a helper already active on this semantic-summary stack is
unsupported, including mutual recursion and calls with different symbolic
arguments. Never assume an in-progress helper repeatable. Use no helper cache in
the initial extension: analyze eligible bodies with substituted terms directly.

Share one decreasing work budget across argument evaluation, helper bodies,
frame creation/binding, name resolution, and term interning. Add a call-depth
bound alongside existing AST/construction depth and term bounds; incrementing a
helper call must not reset any bound or create a fresh work allowance. The outer
`has_repeatable_key` query should share its budget across callable alternatives,
while each alternative retains distinct capture scope and term identity. Budget
exhaustion discards the optimization, never returns a partial success.

Do not infer purity from ordinary IFC cache hits, provisional return values,
declared effects, or stable source locations. If caching is proposed later, its
key must include immutable function identity, resolution/boundary metadata, and
actual symbolic dependencies scoped to the current callback; only completed
nonrecursive summaries can be reused.

## Required validation before integration

Keep the current candidate withheld while implementing and reviewing this slice.
Unit tests should assert summary outcomes directly; integration tests should
separately assert both IFC2 and full-checker results. `Unsupported` does not mean
the complete program must be rejected: a public varying key can still be safe.
Do not turn every fallback test into an artificial egress rejection expectation.

Required accepted precision controls for both min/max include the measured
`s > 0`, `-s`, and `fixed(s)` witnesses, plus `!s`, `~s`, the other admitted
comparisons, nested ordered keys, aliases, and helpers forwarding through another
eligible helper. Include helper explicit returns and implicit tails, a helper
that ignores a varying actual and returns a stable actual, and ordinary output
inside a helper while the returned key stays fixed.

Required dependency and control tests include:

- Unary varying operands and comparisons involving callback arguments remain
  varying. Existing secret-dependent unequal-key witnesses still reject.
- Directly constructed `Expr::Block` AST unit controls exercise a left comparison
  operand mutating a binding that the right operand reads; the right must see the
  updated value. Returns in either operand suppress the remaining comparison and
  the appropriate enclosing continuation. These are semantic-interpreter unit
  controls, not claims that every corresponding block shape is expressible as a
  standalone source value block.
- Direct block-AST controls also exercise a helper actual mutating caller state;
  later actuals and the caller continuation see that update. A helper-local
  mutation cannot overwrite the caller's same-name binding. A return inside an
  actual exits the caller; a return inside the helper exits only that helper.
  Separately parsed source twins using `if true { ... }` must remain unsupported:
  that syntax introduces an `Expr::If`, whose transfer is outside this fragment.
  Do not silently grant branches support to make an AST-level control pass as a
  source integration test.
- Add an entry-point parity control with an originally unannotated helper whose
  callers supply a struct containing a secret field, and whose body returns that
  parameter as the fixed key. Record raw IFC2-only findings, full-check findings,
  and the original versus analysis-inferred signature metadata. The valid program
  should preserve public callback-argument logging under both entry points.
  If analysis annotation causes full-check exclusion while raw IFC2 admits the
  helper, retain the full-check verdict flip as an unresolved precision failure;
  do not change the control's expected acceptance or claim parity from IFC2 alone.
- A captured earlier observation stays fixed. Fresh time/random/input/file calls
  inside helpers, nested arguments, or discarded statements force unsupported
  summary, including when the returned value ignores the observation.
- Free-function precedence over same-name locals is preserved for expression
  calls. Local-only callable names remain unsupported. User functions named
  `println`, `push`, or `return` exercise both statement and expression positions;
  wrapper/value lookup is never confused with direct-call lookup.
- Wrong-arity direct calls are unsupported. Local aliases of function values,
  missing/extra wrapper arguments, generic specialization, methods, and typed
  signature guards stay excluded until their adapters are separately implemented.
  Tiny direct tests of any future wrapper adapter must prove extra actual effects
  are evaluated before truncation and defaults are inserted only after evaluation.
- Direct and mutual recursive helpers fail conservatively before entering a
  provisional summary. Tiny injected budgets test nested-call work, call-depth,
  term, and construction-depth exhaustion without host stress/crash workloads.
- `/`, `%`, `&&`, `||`, indexing, assertions, and unsupported mutations fail the
  summary even when a later tail is stable. Data-dependent trap witnesses must
  not obtain repeatability through a helper. Trap/crash runtime execution remains
  subject to the repository's disposable-guest requirement; no host substitute.
- Direct secret-key egress, secret callable identity, secret collection shape,
  empty/singleton/pair controls, declassified keys, and joined callback snapshots
  retain their existing expected behavior.

Independent review must cover exhaustive AST matches, runtime operand order,
statement-versus-expression dispatch, frame isolation, helper boundaries, and
budget/cycle behavior. After source-bound tests pass, compare an expanded valid
corpus against the pinned baseline before asserting compatibility. Native CI and
full mission completion remain separate obligations.

This design does not claim general purity inference, termination, protection from
resource exhaustion, a new trap-flow model, or complete runtime noninterference.
It provides the next bounded dependency for the measured regressions and names
the admission frontier that still needs evidence.
