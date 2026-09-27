# Captured Safe build consumer — design review, 2026-09-26

The [reviewed design](../mission/design/CAPTURED_SAFE_BUILD_2026-09-26.md)
has SHA-256
`95908d07ccf553b2b0afe80bbb82f356649ea5cdc7b1541f7e1afb352c1f387f`.
It is a proposed opt-in Safe-default native build from captured entry and
import bytes. It does not grant verified effect completeness, package
admission, evidence sealing, or a platform release witness.

An independent read-only design review first held the proposal because the
existing native writer could overwrite an output source/executable or leave a
stale executable after failure; the private bridge collapsed distinct failure
stages into `String`; function-mode inspection missed nested mode elevators;
and the proposed result was described too strongly for default Safe checking.
The revision specifies typed stage failures, a recursive Safe-only guard,
unique staged no-clobber publication, and explicit Safe-default semantics.

The re-review held that revision for an attribute-classification gap:
`compiler/src/frontend/mod.rs` recognizes `research`, `poc`, `fuzz`, `proof`,
`defensive`, and `audit` as Research attributes, while the reused predicate
in `compiler/src/evidence/mod.rs` recognized only `research` and `exploit`.
The final design requires one shared frontend Research-attribute mapping used
by both parsing and the recursive predicate, retaining Exploit handling and
classification controls for overwritten `@safe` attributes, imported and
nested modules, and impl and trait methods.

The independent reviewer then gave **GO for implementation of this design**,
subject to review of each actual diff and lead-run tests. This is not approval
of a compiler patch, a CLI command, native output publication, package
acceptance, or any release claim. The review explicitly left the separately
parsed manifest, dependency closure, runtime toolchain, and native artifact
correspondence outside the captured compilation graph. The implementation
breakdown in the design keeps compiler checking, staged publication, and CLI
consumption in separate reviewable units. Independent actual-diff review
**held** the first compiler-side scratch patch: trait declarations and unused
or overridden defaults can disappear during desugaring before its mode guard
sees them. A revised patch must preserve source-bound pre-desugaring mode
intent for every captured module and test those trait forms. No compiler
patch from this design is integrated.

No build, Anubis source execution, runtime witness, or hosted gate was run in
this design-review lane. The next dependency is a final-diff-reviewed compiler
check/result unit, followed by its lead-run Safe controls; staged native
publication and CLI consumption need their own implementation and review.
