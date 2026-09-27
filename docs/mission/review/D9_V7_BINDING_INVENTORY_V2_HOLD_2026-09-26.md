# Independent static review: binding inventory v2

**Verdict: HOLD for production-linked integration.** Frozen patch SHA-256:
`6711e9c1ba3a9cf47a9d658fbfaf17f0f777dd53452306e815d0a260a730c93e`.
The integration base `compiler/src/middle/mod.rs` was SHA-256
`6abfc5e3359adb535a42a64a84de78b746d5df93ffea81ca4a9aa9ed0f4a981c`.
This review compared the frozen patch and `v2-candidate` against the source-matched frontend,
runtime, gate script, and registered fixture files. It made no shared-checkout edits and ran no
git, Rust build, Clippy, tests, Anubis parser/check/run, or Safe corpus grading. The reported
scratch G19 analyzer results were supplied by the author; they were not reproduced here.

## Remaining blocker

The production hook creates a function-local inventory, then reads it only inside
`debug_assert!(binding_inventory.internally_consistent())`
(`v2-candidate/compiler/src/middle/mod.rs:7892-7897`). Rust's `debug_assert!` is disabled by
default in optimized builds ([Rust documentation](https://doc.rust-lang.org/core/macro.debug_assert.html));
the source-matched Cargo configuration has no release override. Consequently the release path
has no surviving inventory read or retained result. Optimization may eliminate the construction,
and even if it remains, the identity map is discarded before `analyze_stmts`. Direct builder unit
tests cannot prove the real analysis path produces an observation. This is a source-level hook
for debug/test builds, not yet a durable production-linked sidecar.

The minimal next revision should retain or emit a **read-only, per-function observation** through
the actual `analyze_function` path in release builds, with a pipeline test that verifies the
observed identities. It can remain outside Safe verdict, evidence, and codegen decisions. Merely
adding an artificial read or `black_box` would keep work alive without making the result usable
for the reviewed place/effect transfer.

## Revisions that resolved v1 findings

`binding_sites.rs:419-445` now marks an empty Or pattern opaque and still traverses the enclosed
match guard/body or if-let then expression; the constructed-AST control at `:850-892` checks
this. `bind_pattern` also marks an empty Or opaque at `:350-355`. All current code-bearing
`Expr`, `Stmt`, and `Pattern` fields are explicitly destructured in the new walker, eliminating
the v1 `..` field-omission risk. The new tests exercise registered D9 shadow and deferred-lambda
fixtures (`:785-822`), a value-position block, nested list pattern, and if-let (`:824-848`),
and an Or alternative with an unresolved use (`:894-915`). Referenced fixture files exist in
the source-matched checkout; the registered D9 fixture hashes matched the prior design's frozen
identities when read for this review. Their parser behavior remains unexecuted.

The source-matched `Span` is `Copy`, so reuse of the test `span` value is type-plausible. The
added test syntax matches the parser's list-pattern, if-let, and value-block branches on static
inspection. There is no evident Rust enum/API mismatch. The new module remains observational;
the patch does not directly alter a Safe verdict branch.

## Boundaries for the lead's executable checks

The scratch G19 checks reported `binding_sites::walk_expr:expr OK` and
`binding_sites::walk_stmt:stmt OK`, while the pattern scope is vacuous under that analyzer because
its tracked fields hold no executable expressions. This does not register the new walker in the
repository's ongoing G19 gate (`scripts/run_walker_completeness_gate.sh:33-74`). Explicit field
binding currently supplies a Rust compile-time floor, but a lead-owned build and gate run are
still required.

The private tuple payloads of `DeclarationId` and `PathStep` appear to be read only by derived
comparison/order implementations (`binding_sites.rs:10-24`); they may trigger denied `dead_code`
warnings. This is a Clippy risk, not an observed failure. The lead must run the pinned Clippy
gate. `internally_consistent()` permits `opaque_sites` by design (`:96-111`), so any future
consumer must separately refuse or propagate incompleteness and unresolved/multi-candidate uses.
The new allocation and traversal cost on debug/test paths has not been measured.
