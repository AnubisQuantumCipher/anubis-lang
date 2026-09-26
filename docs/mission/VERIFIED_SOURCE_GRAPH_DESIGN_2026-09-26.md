# Verified source graph: package and evidence design

Status: reviewed design input, not an implemented assurance claim. Commit
`72e1390b` adds a caller-supplied, path-sensitive graph identity primitive and
focused tests; it does not read the filesystem, prove that graph edges match
parsed imports, or bind checking and lowering to the graph. Package admission
therefore retains its current fail-closed limits. The current package and
evidence status remains in [CLAIMS](../CLAIMS.md), and the execution order
remains in the [completion blueprint](../COMPLETION_BLUEPRINT.md).

## Problem and required relationship

The current resolver can read a package twice: it resolves dependencies for the
entry, then combines modules through another resolution that reopens source
paths. A signed Merkle root over files supplied to an evidence bundle therefore
does not establish which files and namespaces the checker actually used. A
consumer can also import the same package file under another dotted name, or
load a module in a source directory omitted by the package walker. The current
verified-dependency gate refuses those unbound shapes. Its valid transitive
path-dependency test fails until this relationship is established.

Create one bounded `VerifiedSourceGraph` for each compilation or package
admission. A compilation graph contains every node reachable from that
compilation's entry. A published package graph covers its entire importable
surface, including exports not reachable from its own `main` or `lib` entry;
alternatively a manifest may declare an exact exported set only if the
resolver enforces that set for every consumer. It must own the exact bytes
parsed and checked. The checker, native lowering, evidence producer, and
package publisher consume that same graph; none may reopen a source path or
resolve a second graph after the claim is formed.
Filesystem paths remain local locators, never semantic identities in a signed
payload. Use validated UTF-8 or a byte-preserving portable path encoding;
never silently replace path bytes. Reject duplicate normalized paths and
platform case or Unicode collisions in a portable package. Read through a
platform-specific no-follow interface that checks every path component into a
private immutable snapshot, with explicit entry, byte, node, depth, and work limits.
An incomplete snapshot is a typed refusal.

Give the graph a domain-separated, versioned canonical encoding and digest
that binds every relative path, namespace, edge, and leaf bytes, including a
graph with only one source node. Reject ambiguous path encodings. The current
single-leaf Merkle root intentionally omits the path; it is suitable only for
its historical claim, not as the new graph identity.

The graph must retain, for each node, a portable relative identity, exact
bytes and digest, parsed items, all checker-relevant sidecars including trait
environments, source spans, and origin (project, package, or embedded standard
library). A span identifies its graph node and byte range, not an unqualified
offset. For each import it must retain the requested spelling, source node,
span, resolved target, and canonical namespace. Validate every edge before
deduplicating a target file. Namespace lowering and local import aliases must
be injective over the admitted graph: refuse one file reached under conflicting
checker names, different files lowered to the same name, and colliding aliases.
In particular, `a.b` versus `a_b` and `a.foo` versus `b.foo` need explicit
controls under the current prefix and last-segment alias rules.
Enumerate every module the consumer can import, including source directories
named `out`, `target`, `evidence`, or `evidence-*`; a build-output skip rule
cannot silently exclude an importable source. Embedded standard-library nodes
need their exact virtual bytes and a bound registry/toolchain identity.

The package graph also owns exact manifest and lock bytes, a strict parse of
both, and the complete declared dependency DAG. Malformed or unreadable nested
manifests must fail rather than disappear. A portable, versioned closure
record must bind each dependency name and version to its content digest,
verified evidence-record digest, cryptographically verified signer, and all
declared edges. Select the exact evidence artifact by a deterministic, bound
identity; never choose the first directory with an `evidence-*` name. In the
legacy lock format, `evidence_sha256` hashes the bytes of
`evidence/MANIFEST.sha256`; verify that exact byte stream under the legacy
rule. Give the new evidence-record digest an explicit field and format. Each
lock `signer_public_key` must equal the cryptographically verified signature
key.

Record and independently compare three distinct identities: the package
payload/content digest used for cache transport, the path-sensitive graph
digest for semantic source and import identity, and the selected evidence
record digest. The existing directory Merkle root omits build-output names
and the lockfile, so it cannot stand in for the complete graph digest.
Reject duplicate, extra, stale, or missing lock entries and missing transitive
edges. No dependency may add a trust root: the consuming project's trust store
and explicit project signer policy authorize the whole chain.

## Admission and evidence

The producer checks and builds from its one graph, serializes a deterministic
resolved checker input including sidecars, and seals the source nodes, import
edges, manifest, lock, dependency identities, and semantic profile, including
resolved entry/import precedence, checker options, and language edition. It
must compare the checked representation with the emitted PCA source before
signing.
The consumer reconstructs the graph from the mounted immutable snapshot,
verifies every dependency as a Safe PASS under its own signer policy, compares
the complete graph and checker input to the sealed record, then admits the
package. A checksum of a self-declared sidecar is not this reconstruction.
Proof and effect summaries need qualified identities; functions with equal
bare names in different modules cannot be collapsed.

Keep historical single-module evidence readable under its recorded rules;
readability does not grant eligibility for the new verified package claim.
The existing unsigned proof opt-in likewise cannot satisfy a claim requiring
a cryptographically verified signer. Multi-module and transitive verified
admission needs a versioned new closure claim; an old signed entry-only bundle
cannot be upgraded by rehashing it. The existing transitive fixture has an app
that imports its leaf independently while its mid function is self-contained.
Reseal that fixture with its complete lock and closure, and add a separate
mid-to-leaf call fixture that exercises the transitive edge. Unsupported
package shapes remain explicit precision defects until implemented, not
accepted as product completion.

## Acceptance controls for the implementation units

Tests must compare the exact graph used by checking with the sealed graph and
the consumer graph. Positive programs include a single-module package, a
multi-module package, a transitive path dependency that actually calls its leaf,
an independently imported `sample.extra` that `sample.lib` does not import,
an embedded standard-library import, and a package with both `lib` and `main`
entries whose consumer imports the sealed library while its main remains
separately buildable. Negative controls change source, manifest, lock,
dependency evidence, signer, an import edge, a source file after snapshot
creation, and a module under `src/out`, `src/target`, and `src/evidence-*`.
They also try `sample` and
`sample.lib` aliases with conflicting identities, `a.b` versus `a_b`,
`a.foo` versus `b.foo`, and a publisher/consumer `main` versus `lib`
selection mismatch. Compatible aliases may normalize to one identity under
a specified rule; they must not be rejected solely for having two spellings.
The checker must preserve imported trait sidecars, or refuse them explicitly
until it can. Each negative result needs a typed reason; valid controls must
still compile and run for the right reason. Independent review must inspect
the final source graph, not just the manifest schema.

This design does not prove source-to-obligation generation, emitted-Rust
equivalence, solver correctness, or external filesystem and hardware behavior.
Those arrows remain in [proof correspondence](../PROOF_CORRESPONDENCE.md).
