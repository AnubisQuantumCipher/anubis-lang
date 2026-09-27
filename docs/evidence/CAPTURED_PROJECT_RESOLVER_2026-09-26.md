# Captured-byte project resolver: private staged result

Local code commit `03212a4c71037caa3645d8148d5c0fba46445c99`
adds a private project-only resolver that parses a previously captured
`CapturedTree`. For reached project modules it reads the saved bytes without
reopening source paths; embedded standard-library modules come from the
compiler's static registry. It preserves each module's AST and trait sidecar,
each top-level import spelling and source-qualified byte span, and module
postorder. It uses the existing project candidate precedence and explicitly
refuses cycles, missing modules, invalid UTF-8, resource limits, and
namespace, alias, case, and lowered-function collisions.

The first standalone draft was held in independent review: the legacy
`collect_imports` sees only top-level items, while the parser accepts
imports inside `module { ... }`. The final patch scans parsed nested item
containers before graph construction and returns a typed `NestedImport`
error with source key, spelling, and span. It does not silently omit those
bytes or claim to implement nested-module import semantics. The test checks
both an entry-file nested import and one nested inside an imported file.

Independent read-only review gave a scoped GO on the final patch SHA-256
`faea18b29a6132ac141e5b3af75f0339da9777ee8f295adaf3052bfa6007f2fc`.
Lead-owned Linux AArch64 validation passed the focused captured-resolver
Safe tests (10), existing source-graph and reader Safe tests (20), workspace
Clippy with `-D warnings`, format, diff, and documentation drift. The local
source- and log-bound validation manifest SHA-256 is
`3b279dbb08e0f14fa64401fe7f9dca8bba8a6fe8e4ad73ef344d0815418df69b`.
No release binary or final workspace gate was built for this private unit.

**No production admission authority follows.** The public resolver,
checker, native lowerer, package proof gate, and evidence verifier do not
consume this graph. It has no dependency mounts, no source-graph v2 identity,
and no proof that every package source is in an admitted closure. The valid
transitive package fixture still refuses. Next, bind the captured graph to
one checker/lowering/evidence input with explicit dependency behavior and
versioned source identity; keep unrepresented nested imports refused until
their semantics and proof correspondence are designed. The capture reader's
trusted-procfs assumption and unsupported Apple acquisition remain external
platform gaps. No hosted, guest, independent rebuild, or release result is
claimed here.
