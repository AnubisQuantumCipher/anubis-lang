# Completion requirements and evidence map

This maps the owner's [execution mandate](EXECUTION_MANDATE_2026-09-23.md) to the
existing roadmap, source boundaries and evidence obligations. It is a coverage index,
not another defect-status database. Current soundness status remains in
[CLAIMS](../CLAIMS.md), fed by [DEFECTS](DEFECTS.md); the execution authorities and
integration order remain linked from [MISSION](../MISSION.md).

The assessment column is a dated inspection snapshot from 2026-09-25 at
`2990eab2c6ad856fd2f1bfc25fa77c3e6a60b92a`. It is not a final acceptance receipt.
Later implementations need current source-specific evidence for every applicable row.
This inventory does not establish a complete first-party source review.

## Authority reconciliation

- The mandate authorizes routine slice and phase continuation once the actual criteria
  and required receipts hold. Blueprint phase stops do not require repeated permission.
- Production-linked correspondence and full self-hosting are mandatory mission outcomes.
  Older references to unscheduled research do not remove them from this mission.
- Documenting a refused required valid workload leaves a precision defect; it does not
  complete the requirement. A known violation inside a shipped verified claim remains open.
- Disposable-guest requirements for crash, fuzz, research and exploit execution remain
  mandatory. A memory-capped host process does not replace that witness.
- Preserve the existing language by default. Semantic changes require a reviewed edition,
  migration mechanism and compatibility evidence under the mandate, not a silent change.
- Merge, release/tag publication, destructive operations, history rewriting and changes
  to account security or isolation policy retain their actual authorization boundaries.

The mandate is copied byte-for-byte from the owner's supplied document. Its SHA-256 is
`5c7a1b1135d0045c67ec8d62cc431390f358d98925855d5165405e1479e13a62` (derived from the copied bytes by the preparation script).

## Requirement-to-evidence map

Status vocabulary here: **partial** means inspected evidence supports only a bounded part; **open** means current evidence contradicts completion; **unverified** means this inventory has not inspected sufficient current evidence. None is a PASS for final release.

| ID / mandate section | Required result | Main source/artifact boundary | Evidence required for completion | Current assessment / dependency |
|---|---|---|---|---|
| M01 / §1 | Active-lead ownership, preserved unrelated work, durable continuity, real permission boundaries | AGENTS, worktrees, mission records, receipts | Exact current worktree/commit/dirty/job/pin inventory; owned lanes; durable next action; separate commits | Partial; implementation work and unrelated worktrees preserved; lead owns integration |
| M02 / §2 | Reconcile every reported revision, ancestry, source-bound baseline and Family-1 evidence | `e34d0c89`, `7220c4b1`, `13373e31`, `029d5538`, `84296cef`, `c87ad1cf`; item-21 receipt | Full object identities/ancestry/content; recovered matrix/review/log/manifests; exact source-to-pin proof; stack boundaries | Partial: remote PR and saved head receipt inspected; ancestry and original Family-1 bundle need integrator confirmation |
| M03 / §3 | Complete first-party review plus artifact/generator inventory; one status authority and roadmap | All tracked/untracked first-party source and artifacts; `docs/mission/review/`; roadmap/docs | Per-file review depth and revision; generated/vendor provenance; architectural map; requirement/source/test/proof links | Open: existing mapper reports explicitly record targeted/signature-only and unread files; this inventory is not a full source review |
| M04 / §4 | Frozen meaningful contract, named A/B finish lines, coverage map, precise outcomes/profiles | MISSION, SPEC, SPEC_1_0_FREEZE, SEMVER, diagnostics/evidence schemas | Versioned constructs→property map; frozen valid/invalid workloads; bounds, termination and external assumptions; profile acceptance end to end | Partial: vocabulary and freeze exist; full coverage and workload evidence missing |
| M05 / §5 | Close complete known false-accept mechanisms and restore required valid twins | `middle/mod.rs`, `whole.rs`, contract carrier, matrix, CLAIMS/DEFECTS | Baseline failure and diagnostic; corrected reject plus clean/satisfied/unreachable controls; runtime witnesses; every verdict flip classified | Open: saved head matrix has silent accepts and wrong-class cases; known unregistered leaks remain |
| M05-CARRIERS | D9, taint sink arguments, containers/fields/returned functions/joins, direct and indirect requires, all application forms | DEFECTS D9/T-SINKARG/M-HOF-UNKNOWN/FV-*; application/summary walkers | Mechanism-level closure including nested/cross-package forms; preserve valid `apply(f, positive)` | Partial fixes recorded; residuals require reproduction and closure |
| M05-FLOW | Mutable/shadowed names, stale facts, loops, callable identity, every expression and exit position | Label/contract/effect walkers and runtime | Composed variant suite; no capture/coercion/fresh-symbol false counterexamples; reachable/dead/zero-iteration semantics | Open; alias/summary/overref/perf and round-39 units in flight |
| M05-MATRIX | Immutable case history and no count laundering | `registry.tsv`, `history.tsv`, `recategorizations.tsv` | Unique IDs/source/intent/baseline/current/class/witness/category/rationale; SHARED remains counted; no silent whitelist or weakened oracle | Partial structured matrix exists; known witnessed probes not yet registered |
| M06 / §6 | One justified semantic foundation | Types/binding IDs/place paths/CFG or justified equivalent; summaries; builtin registry | Reviewed design; compositional transfer/join laws; sound recursion/fixpoints; dependency-complete cache keys; old/new and independent semantic tests | Unverified overall; existing name/string maps are known mapping findings |
| M06-BUILTINS | Declarative signatures/evaluation/mutation/effects/labels/proof/runtime registry | Builtin consumers across compiler/runtime | Generated total consistency checks; explicit unknown behavior; inventory covers every builtin and new additions | Partial HOF work exists; full registry requirement not established |
| M07 / §7 | Normative complete language and executable conformance | SPEC, GRAMMAR, LANGUAGE, runtime/native/proof semantics | Lexing/precedence/evaluation/bindings/types/functions/closures/recursion/collections/structs/enums/patterns/generics/traits/modules/visibility/errors/resources all specified and tested | Open: current SPEC calls itself a sketch and GRAMMAR partial per mapper report; must inspect and complete current files |
| M07-NUMERIC | Numeric semantics match execution | Integer widths/sign/shifts/division/casts/literals; floats/NaN/infinities/signed zero/coercions | Independent expected semantics, model/runtime differential fixtures, compatibility policy | Partial float fixes recorded; complete semantic coverage unverified |
| M07-COMPAT | Explicit compatibility and dynamic boundaries | Editions, migration tooling, type/label model | Deliberate changes have edition/migration diagnostic/tool/fixtures/rationale; no silent spelling change or regression relabeling | Unverified; L-SHADOW is a pending compatibility decision |
| M07-DATA | Defined bytes/text/Unicode/index/order/equality/serialization/exhaustion/determinism | Stdlib/runtime/schema documentation | Conformance with explicit environmental/concurrent nondeterminism; versioned language/schema/stdlib policy | Unverified |
| M08 / §8 | Complete production proof chain for declared core | Source→resolved types→analysis→VC→encoding→checked solver→runtime→offline verifier | Trust/link map; production-linked theorems/extraction or checked translation witnesses; all assumptions and backend TCB explicit | Open: PROOF_CORRESPONDENCE explicitly records main-chain TCB gaps |
| M08-COVER | Resolution, capture-safe substitution, updates, labels, callable summaries, joins, contracts, arithmetic, outcome preservation | Lean and actual production algorithms | Formal relation to executed implementation, not theorem-name checks or disconnected models | Open; bounded SecurityLabel witness does not establish whole chain |
| M08-SOLVER | Honest supported fragments, SAT/UNSAT validation, arithmetic/data/loops/recursion, budgets/cancellation | Solver/parser/encoder/certificate checker | Malformed/truncated/hostile certificate controls; independent model/certificate checking; invalidation under every dependency; exact fragment boundaries | Partial; A-SOLVER-1 and A-GATE/A-EVID reports need reproduction |
| M08-NATIVE | Production-linked/checkable lowering | Native code generation, emitted Rust, runtime | Lowering result or checkable route for verified core; explicit Rust/LLVM/OS/hardware trust boundary | Unverified; no borrowed claim from emitted Rust |
| M09 / §9 | Independently usable evidence product | Obligation identities, evidence bundle, offline verifier | Stable IDs/locations/call chains/properties/assumptions/provenance; typed outcomes; full source/build/dependency/profile/policy binding; canonical serialization; offline tamper rejection | Partial implementation; A-EVID reports and source-to-proof links remain open |
| M09-HONEST | Evidence tests independent of real vulnerabilities; historical stamp audit | E-POISON-1; docs drift floors/receipts; claim inventory | Controlled negative fixtures plus real forgery tests; each historical exemption justified; current claims enforced without lowered bar | E-POISON-1 marked fixed; complete floor reconciliation unverified |
| M10 / §10 | Runtime memory and system-resource model | Runtime, capabilities, allocator, handles | Specified ownership/lifetime/copy/alias/capture/cycles/destruction/limits; identity/nonduplication across calls/containers/packages/FFI | Unverified |
| M10-CONCUR | Coherent concurrency | Tasks/queues/synchronization/runtime | Structured lifetime/cancellation/timeouts/bounded queues/errors; race/cleanup evidence; stated replay semantics | Unverified |
| M10-FFI | Stable C ABI and documented Rust route | Native adapters/FFI | Layout/calling convention/ownership/callback/unwind/thread/error contracts and explicit trusted foreign summaries | Unverified |
| M10-POLICY | Compile effects enforced at runtime independently of model prompts | Native isolation/capability policy | Undeclared effects denied through adapters/dynamic calls; paths/symlinks/argv/env/handles/network/DNS/TOCTOU tests; protected secrets stay out of logs/evidence | Unverified overall; platform-specific witnesses remain mandatory |
| M11 / §11 | Secure reproducible packages and coherent stdlib | Package resolver/lock/cache/trust; stdlib/runtime | Deterministic locks/integrity/signatures/offline/conflicts; safe import authority; actual cross-package summaries; complete cache invalidation; frozen API examples/conformance | Partial; F-PKG-1/F-PKG-2 reported and not yet reproduced/fixed |
| M11-DXBUILD | Profiling/tracing/debugging and reproducible dependencies/toolchains | Install/build/debug/source-map tooling | User-facing ordinary diagnostics without reading generated Rust; clean/local/CI/release/proof relationship and defined reproducible payload | Unverified |
| M12 / §12 | Precise developer and AI protocol | DIAGNOSTICS_JSON; LSP; CLI machine interfaces | Source ranges/related sites/counterexamples/effects/assumptions/reason codes/docs; stable identity; multi-file navigation/completion/hover/rename/actions/format tests | Partial JSON schema exists; mapper reports LSP/locations/multi-file gaps |
| M12-REPAIR | Reverification of repairs without specification laundering | Repair protocol/gate and held-out corpus | Apply exact revision; recheck dependency closure; unchanged policies/contracts; behavioral tests; invalid repairs rejected; useful diagnosis/latency/editor metrics | Unverified; roadmap explicitly says repair corpus/loop absent |
| M13 / §13 | Honest first-class Linux x86-64/AArch64 and retained Apple support | Platform matrix; CI; packaging; install/upgrade/uninstall/completions/editor/headless/doctor | Actual supported clean installation witnesses, host-vs-guest labels, reproducible native CLI/toolchain, no unrelated OS modification | Open: Linux CI absent; specialized Apple witnesses not substituted |
| M13-ISOLATE | Portable isolation interface with reviewed platform mechanisms | Platform capability discovery/threat model/isolation policy | Declared process/namespace/syscall/resource/VM guarantees, proper guests for mandated lanes; no process sandbox relabeled hypervisor | Unverified; missing authorized guest keeps only that lane open |
| M13-SIA | SIA optional bounded adapter | Memory integration boundary | Core compile/verify/install/examples pass with SIA offline; protocol and test substitute if adapter retained | Unverified; no SIA modification authorized |
| M14 / §14 | Full self-hosted production compiler | Selfhost parser/checker/compiler plus Rust bootstrap reference | Component inventory; frozen surface; per-function/per-obligation semantic comparisons; shadow→opt-in→authority promotion; clean pinned bootstrap/fixpoint; meaningful independent DDC | Open: R-SELFHOST-1 reports demonstration subset; historical fixpoint is not full production surface |
| M14-FINAL | Self-host builds compiler/tools/reference apps | Production selfhost artifact | Equivalent documented semantics; remaining backend/solver/OS/hardware TCB named | Unverified and not established by existing seals |
| M15 / §15 | Proof execution, sound proof scaling and measured performance | RISC0/proof backend, slicing/cache/specialization, benchmarks | Receipt binds executable/source/build/entrypoint/inputs/outputs/assumptions; all relevant paths retained; benchmarks include distribution/variance/toolchain/hardware | Unverified; existing cost probes are not this acceptance program |
| M15-COMPARE | Reproducible Rust/Ada-SPARK/Zig comparisons | Benchmark implementations/report | Current official docs/pinned tools at execution; equivalent workload/safety/algorithms/dependencies; unsupported and unfavorable results retained | Unverified |
| M16 / §16 | Substantial useful Anubis applications | AI tool runner; production service; systems/data app; existing showcases where suitable | Clean install, threat/assumptions, positive/negative tests, operational limits/recovery and performance for each | Unverified; presence of showcases is not final application acceptance |
| M16-RUNNER | Useful capability-constrained AI tool runner | Application + policy/evidence | Untrusted text, protected secret, permitted file/network work, cancellation, action receipts, adversarial authority controls and useful positive control | Unverified |
| M16-SERVICE | Production-style service | API/persistence/concurrency/deploy/recovery | Validation/errors/limits/config/integration plus realistic confidentiality/integrity paths | Unverified |
| M16-DATA | Systems/data processing | Binary/text/checked arithmetic/collections/streaming/parallel program | Reproducible results, independent oracle, real performance workload and recovery | Unverified |
| M17 / §17 | Permanent layered verification and independent review | Unit/property/conformance/composition/differential/tamper/corpus/isolation/formal/bootstrap/platform/reproduction gates | Exact final workspace and artifact results; baseline and positive controls; independent design and final-diff review; review semantic fixes again; external expert release audit | Open: latest ordinary/whole/checker-limit follow-up reviews remain pending; full final candidate absent |
| M17-INSTRUMENT | Deterministic trustworthy harness | Build/stdin/process/temp/exit/log capture | Actual root cause of intermittent failures; unique paths/owned processes/controlled stdin; all failed logs retained; no retry-until-green substitution | Partial ETXTBSY fixes; final-candidate harness audit unverified |
| M18 / §18 | Dependency-correct execution and versioned deliverables | Stage A→H; canonical docs/evidence structure | All deliverables listed below, current status derived from structured evidence, historical corrections provenance-preserving | Partial index/matrix/reports exist; many deliverables unverified |
| M19 / §19 | Same-candidate product acceptance and full mission acceptance | Release commit/artifacts/platform matrix | Every final gate below demonstrated; no incompatible old receipt; both A and B true | Open, not close to a supportable completion claim |
| M20 / §20 | Execute useful units, retain exact state/next dependency | Active alias lane and mission records | Real implementation/test/review/integration progress; specific job handles; continue independent work through external blockers | Active implementation; this inventory is a bounded supporting artifact |

## Required versioned deliverables

These must live in the repository's established documentation/evidence structure before final completion. This index identifies evidence obligations; it does not itself satisfy them.

| Mandate §18 item | Deliverable | Existing candidate | Current completion evidence |
|---|---|---|---|
| 1 | Mission index with canonical roadmap and registry | `docs/MISSION.md` | Present; this continuation updates the stale next-dependency text from 2026-09-23 |
| 2 | Architecture map and complete first-party review inventory | `docs/mission/review/01-04` | Partial review explicitly declares unread areas |
| 3 | Normative semantics, compatibility, threat/assurance profiles | `docs/language/`, `LANGUAGE.md`, platform/security docs | Incomplete/unverified against mandate |
| 4 | Requirement→source→test→proof coverage | Existing correspondence map plus this requirement index | Full versioned coverage not established |
| 5 | Stable defect/acceptance matrix, historical recategorizations | CLAIMS, DEFECTS, soundness matrix | Present but known unregistered probes and stale duplicate status remain |
| 6 | Reproducible binary/build manifest and machine-readable receipts | Pins, publish_pin machinery, CI attestations, dated evidence | Local mission pins exist; final release source/build binding not established |
| 7 | Trust/correspondence map with external assumptions | `docs/PROOF_CORRESPONDENCE.md` | Present and explicitly incomplete |
| 8 | Platform matrix with execution witnesses | Installation/CI/platform docs | Linux CI absent; full actual witness matrix unverified |
| 9 | Self-host bootstrap/equivalence evidence | SELFHOST docs and historical seals | Historical subset evidence; full surface not established |
| 10 | Developer/AI protocol and repair-validation evidence | DIAGNOSTICS_JSON and schema | Protocol partial; repair loop evidence missing per roadmap |
| 11 | Application acceptance and benchmark reports | Existing showcase docs | Final mandate acceptance not established |
| 12 | Release checklist/migration/SBOM/provenance/rollback/PR material | Existing release docs and the current draft PR | Final candidate package unverified; merge/release authorization still required |

## Final audit checklist: mandate §19

Each item must be demonstrated on the same identified candidate and compatible platform artifacts. Current status of all items is **not proven complete**.

- Frozen language/API implemented, normatively specified and conformance-covered.
- Required assurance claims scoped with every obligation accounted for; no unresolved known soundness violation inside a verified shipped claim.
- Original and expanded defects accounted for; required valid programs accepted for the right reason.
- Unknown/unsupported/timeout/tool failure/runtime enforcement/counterexample/proof distinguishable end to end.
- Runtime/contracts/labels/effects/packages/native/proof backends agree within declared boundaries.
- Required applications work from clean supported installations and meet behavior/security/operations/measured performance acceptance.
- Host/guest and platform/isolation witnesses are accurate and complete for every advertised target.
- Build/check/run/test/package/supported prove/offline verify documented as an end-to-end workflow.
- Final workspace/fixtures/formal/lint/format/drift/bootstrap/platform gates pass under their actual conditions.
- Independent final trust-sensitive review and independent environment release reproduction exist.
- Install/upgrade/migration/stable schemas/debugging/reference docs/tutorials/release/rollback complete.
- Release stack respects branch protection and hosted witnesses; local work not called merged/released.
- Full production-linked correspondence and full self-hosting acceptance met; remaining external TCB explicit.

## ROADMAP_AI_ERA acceptance reconciliation

These are proposal criteria in that file, not assertions of present achievement. All need a mapped decision/evidence outcome; neither a legacy “done” heading nor this inventory closes one.

| Criterion | Required witness | Current assessed state |
|---|---|---|
| 1 | Forged certificate invalidates bundle after hashes recomputed | Historical first-step evidence exists; revalidate final artifact |
| 2 | Independent pinned third-party recheck of every obligation without Anubis | Unverified; A-GATE-4 reports not wired |
| 3 | Honest numerical certificate coverage in verdict | Implemented first step documented; final obligation completeness unverified |
| 4 | Every obligation including vacuity has file-backed witness | Open certificate gaps reported; not proven complete |
| 5 | Load-independent corpus verdicts under declared saturation runs | Resource handling fixes exist; required complete campaign not established |
| 6 | Kernel denies undeclared effect | Required runtime-policy witness not established |
| 7 | Source-derived sealed kernel policy, edits rejected | Unverified |
| 8 | Mechanized restrictive policy for open effect row | Unverified |
| 9 | Machine actionable refusal schema and repair-corpus convergence | Schema present; corpus/loop explicitly absent in roadmap |
| 10 | Contract adequacy/vacuity policy and evidence field | Proposed policy needs compatibility/semantic reconciliation; not achieved |
| 11 | Conformance with DEFER | Not established |
| 12 | Accept-set monotonicity on declared corpus | Must reconcile precision restorations and intentional editions; no fake zero-regression claim |
| 13 | Linux bootstrap fixed point in required guests, CI and release artifact | Linux CI absent; mandatory witness open |
| 14 | Production Lean↔Rust binding stronger than name grep | Bounded SecurityLabel component exists; main chain unproven |
| 15 | Proposed offensive application | Mandate §13 prohibits expansion into new unauthorized offensive operations; use the authorized §16 application acceptance, do not infer offensive authority |
| 16 | Durable independently implementable archived proof verification | Format/verifier work exists; full future-verifiability evidence not established |

