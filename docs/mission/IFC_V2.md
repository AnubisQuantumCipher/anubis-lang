# IFC v2: the information-flow interpreter

IFC v2 is the semantic foundation for information flow that the mandate asks for (section 6): an
abstract interpreter that evaluates a Safe-mode program the way the runtime (`backends/run.rs`)
executes it, over abstract values that carry security labels. It replaces syntactic special cases
with the runtime's own semantics: pure value semantics (every read clones), closures that snapshot
their captures when they are created, the runtime's call-resolution order, method dispatch on the
receiver's runtime type, and every builtin's behaviour.

Code: `compiler/src/middle/ifc2/` (`value.rs` abstract values, `eval.rs` the interpreter,
`builtins.rs` the builtin table, `mod.rs` the entry point).

## What it reports

In Safe mode, every flow the other lanes report, with the same codes:

| code | flow |
|---|---|
| `ANUBIS_SECRET_EXFILTRATION` | secret data reaches an egress (print family, `send`, `network_send`, `connect`, `http_get`, `http_post`, `shell`, `exec`, `system`, `target_run`, a `panic` message, an `exit` status, the proof journal) |
| `ANUBIS_IMPLICIT_FLOW` | an egress runs under a condition a secret decides (decision D1) |
| `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` | untrusted data reaches an integrity sink |
| `ANUBIS_IFC2_LIMIT` | the interpreter stopped before it finished (a budget, a fixpoint that did not settle); the program is refused because nothing is known about what it discloses |

Findings are prefixed `[ifc2]`. `anubis check` runs IFC v2 beside the other lanes in every
Safe-mode check (decision D8): a program is refused if any lane finds a flow. The hidden developer
command `anubis ifc2-report <file>` prints IFC v2's findings alone, for measurement; it is not a
check.

## Abstract values

A value (`V`) stands for every runtime value a program point may hold. It is a tree that mirrors
the runtime value, with a label at each node: `SEC` (confidentiality) and `TNT` (integrity) bits.

- A node's own label is its own information: a scalar's content, a list's length, a map's key
  set, a struct's or enum's type and tag, which function a function value is. The children carry
  their own labels, so `len(xs)` of a list of secrets is public and `xs[0]` is secret.
- A node may be several kinds at once (a join of a struct and a closure, `Some` and `None`).
- Lists keep each position while the sequence is known (a literal nothing has resized since), and
  otherwise the join of their elements. Maps keep each literal key (as the runtime displays it:
  `007` is key `"7"`), the value at any other key, and the label of computed keys; a computed key
  may equal a literal one, so its value joins every known slot. Keys and struct fields that may be
  absent are tracked, so a write under a secret condition changes a container's shape only when
  it may add a key or field.
- `top` is a value of unknown shape, produced by folding a value nested deeper than the depth
  limit, wider than the node limit, or passed to a summary context. It keeps its shape label, its
  content label, the content of each field name anywhere inside it and of the scalars held
  directly under it, and every function it holds.
- Function values name user functions, builtins and closures (a lambda and its captured values).
  Closures nested more than three deep in each other's captures are summarized per lambda and
  reach label (what their captures can reach), and nested compositions become a chain (any
  composition of their functions); both bound values built in loops.

Choosing between two values under a condition (`V::select`) raises only the nodes whose shape
may differ: two structs of one type with the same fields keep a public type, two lists of one
known length keep a public length.

## The interpreter

The state along a path is the scopes of the running function, the label of the conditions it is
under, and the lifts: a `return`, `exit` or `panic` taken under a label lifts the rest of the
function (and, for `exit` and `panic`, every caller's continuation; decision D6); a `break` lifts
the rest of the loop body and every later iteration; a `continue` lifts the rest of the body only;
a loop header decides whether the next header runs. The program counter is their join, and an
egress under a secret program counter is an implicit flow.

- Branches evaluate each side from a copy and merge. Truthiness follows `as_bool`: a scalar's
  content, a collection's emptiness; a struct, enum or closure is always true. A constant
  condition (`if true`, `while false`, `false && x`) runs only the branch the runtime runs, and an
  operation on an operand whose evaluation cannot complete has no value.
- A `match` arm runs when its pattern and guard match and no earlier arm did; an earlier arm whose
  pattern is disjoint from this one never decides it. A destructuring `let` does not test its
  pattern (the runtime does not).
- Method calls evaluate their arguments inside the chosen type arm (truncated or padded to the
  method's arity, as the lowering does); which arm runs is secret only when the receiver may have
  more than one type name.
- Loops iterate to a fixpoint with widening (coarse widening after 12 rounds); solved loops are
  cached per call context.
- Calls are analyzed per (callee, arguments, program counter), 12 precise contexts per callee;
  past that, or once a callee recurses, calls share summary contexts keyed by (callee, program
  counter, joined argument label, the functions the arguments hold), iterated until their
  arguments and result settle.
- Stores outside the program's values are modelled where a builtin writes and another reads back:
  files (`write_file`, `append_file`, `delete_file` → `read_file`, `open`) and the
  Keychain/Secure-Enclave binding (`cap_acquire_nonexportable` → `keychain_se_last_bind`).
- Declared types add labels (`secret<T>`, `tainted<T>`), directed by the type: in `Option<…>`,
  `Result<…, …>`, `list<…>` and `map<…, …>` only the wrapped payloads or elements, when the value
  has that shape; a value of another shape (declared types are not checked at runtime) takes every
  qualifier the type holds. When two definitions share a struct or enum name (a module's and the
  program's), a field takes every type either declares.

Every runtime builtin has an entry in the builtin table; a unit test extracts the runtime's
builtin names from `run.rs` and fails for any without one.

## Budgets

| limit | value | on reaching it |
|---|---|---|
| statements analyzed | 1,500,000 | `ANUBIS_IFC2_LIMIT` |
| value operations (joins, comparisons, widenings, raises) | 40,000,000 | `ANUBIS_IFC2_LIMIT` |
| rounds of one loop, recursion, composition or reduce | 60 | `ANUBIS_IFC2_LIMIT` |
| global rounds (closure summaries, stores) | 12 | `ANUBIS_IFC2_LIMIT` |
| the checker's stack and memory guard (`analysis_limit`) | per request | `ANUBIS_ANALYSIS_LIMIT` (and `ANUBIS_IFC2_LIMIT`) |
| data depth / node count / closure depth / summary depth | 32 / 256 / 3 / 4 | folding (a sound approximation) |

## What it does not claim

- Termination and timing channels (decision D1): whether a program diverges or traps, and how
  long it runs. A runtime trap's message names the operand's type (decision D7).
- `declassify(value, policy, reason)` with both strings non-empty releases the value; whether the
  release is justified is the author's statement.
- Contracts, capabilities and effects: other lanes check them.
- Research and exploit modes: flow checks are Safe-mode only.
- Its answer is sound only where each piece mirrors the runtime; the soundness matrix and
  independent adversarial review are what establish that, and neither proves the absence of
  further divergences.

## Evidence

- Review round 1 (independent adversarial review of the first version, five lenses and a
  verifier): 107 confirmed findings (67 leaks, 31 over-refusals, 9 robustness). All fixed; the 107
  programs are registered as matrix cases (family `IFC2-REVIEW-1`). Two of them (L17, L21) were
  runtime trap messages, fixed in the runtime (D7).
- Matrix, IFC v2 alone against the lanes (2206 cases before the round-1 cases were registered):
  it rejects all 58 registered leaks the lanes accept, accepts 105 valid programs the lanes
  refuse, refuses no valid program the lanes accept, and reaches no limit. The 117 registered
  rejections it does not make are contract, capability and effect cases.
- Corpus (968 programs, `examples/` and `tests/fixtures/`), IFC v2 beside the lanes: 0 verdict
  changes, certificates unchanged (955 certified, 1421 discharged); 67 s for the whole corpus.
- The landed tree (e516b1f3), IFC v2 beside the lanes: the whole matrix (2319 cases) has no
  silent accept; the workspace tests, the corpus and every local gate pass (DEFECTS.md, the
  e516b1f3 section).
- Review round 2 (of the fixed version, before its final landing changes) confirmed 75 more
  findings: 41 leaks the lanes also accept, 28 over-refusals (22 of valid programs the lanes
  accept, which the union therefore newly refuses) and 6 robustness (DEFECTS.md, IFC2-REVIEW-2).
  They are the next unit.
