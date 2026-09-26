# Captured Safe compilation comparison — local receipt, 2026-09-26

Code-only commit `339a960a1da0acd7aa65e23f7469a4ae77f77007` (tree
`6b9da5a95aea76362ad3c784d1d106d667f72f7d`) changes only
`compiler/src/resolve/captured.rs` (SHA-256
`2bc88a033a72f146f5495e32f965914f867a6b354a90497e0720662c2c97ab9c`).
The reviewed scratch patch had SHA-256
`87067c1ebc93a5b15315bfa28f93d47720c035c681e9e06778f806fcd1d933f4`
against base file SHA-256
`bd505e0aa690d5d126577d400366ebd78473dc70dd124f2d0d08c375e5ccb59d`;
the lead added a non-executing Research/Exploit classification control before
the commit. This source identity is local and has no clean-room or hosted witness.

The private `PreparedCapturedProject` keeps its combined AST and compilation
source graph behind one constructor. Both derive from the same captured bytes
and parser-resolved import occurrences. Its comparison method classifies the
whole program as Safe, typechecks that AST, runs the default solver-refusal
gate, and lowers the same AST to Rust **source**. The changed-import test
captures `util::value`, replaces its on-disk body under the same function
name, and verifies that both the saved graph and emitted Rust still contain
the earlier body. Further controls cover imported trait sidecars, collisions,
limits, and refusal before any Research/Exploit checking or lowering. That
last control only parses and classifies; it does not execute those programs.

The [machine-readable gate receipt](gate-receipt.json) (SHA-256
`31f9c9cbb3341efe589fb106872f112c93ae3db8075fde566b2201d93fcbb8d1`)
records the exact code/tree identity, commands, exit codes, and raw/stored log
hashes. On that source, the captured-resolver library tests passed with `17`
passed and `0` failed; compiler-library Clippy with `-D warnings` and workspace
format check both exited `0`. Cargo commands ran in a memory-capped user
scope. The [test](captured-tests-source-bound.log),
[Clippy](clippy-source-bound.log), and [format](fmt-source-bound.log) logs are
retained here. One machine-local cargo target prefix in the test log was
replaced by `<local-cargo-target>` for public storage; the receipt preserves
both the raw-log and stored-log hashes. The local raw manifest SHA-256 is
`d71a736d93a50e5a6d99950ddb551cd4aa0e54139ebff95cf6ff569cc3c4c72a`.

The [independent final-diff review](INDEPENDENT_REVIEW.md) (SHA-256
`b55661fa3f045466d3402f50aca8a369acae9fabcfad2dbfb73dd675cfa2410f`)
approved this *private comparison* unit for a code-only commit. Its gate-log
comments describe the earlier pre-commit logs; the later source-bound gate
receipt above supplies explicit exit codes and complete stored output. The
reviewer did not independently run those gates, prove source/runtime
correspondence, or approve a production caller.
The CLI `check`, `build`, and `run` paths do not use this paired capture. It
does not issue a native artifact, package admission, or independently verified
evidence. `GraphCoverage::Compilation` covers parsed imports, not the full
declared or publishable package surface. The `--verified` capability profile,
resolved-evidence trait-sidecar refusal, package proof refusal, Linux procfs
capture assumption, and mandatory guest gates remain open or unchanged.

Next, design and review one supported Safe CLI consumer that checks and lowers
the paired source without reopening it. Keep evidence and package admission
closed until the versioned checker-input and dependency-closure representation
can justify them. No full workspace, matrix, hosted, guest, Apple, or release
gate result is claimed by this receipt.
