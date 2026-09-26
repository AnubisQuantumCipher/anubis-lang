# Bool proof-commit result kind — 2026-09-26

Implementation: `b835b94b44fa8705dd973eb1133f862706fd04a8`. This is a bounded
compiler-library correction, not round-2, proof-journal, platform or mission completion.

`proof_commit_bool` returns an integer after truth conversion in both native and
guest lowering. IFC2 previously returned its argument's abstract shape. When that
argument was a struct, a later method call could skip an extra argument that the
runtime scalar fallback evaluates. The recovered `r2p_13` source hides secret
printing in that extra argument.

The bool transfer now produces `V::scalar(arg(a, 1).truth())`. It preserves the
existing deep journal-disclosure check and leaves the sibling transfers unchanged.
The current truth dependency, rather than the input's struct or collection kind,
reaches consumers. The ordinary method control still accepts an unused argument
that its runtime does not evaluate.

## Evidence

- `baseline.json`: the old `anubis-ifc2land-3` pin accepts the original in full
  Safe checking and IFC2-only analysis, and rejects the literal-scalar direct
  control with `ANUBIS_SECRET_EXFILTRATION`. Its SHA-256 is recorded from its bytes.
- `baseline-native.json`: the same pin's ordinary native runs print `3\nend\n`
  for supplied synthetic secret `42`, and `7\nend\n` for `142`; both exit with
  status `0`. These are finite native programs in the existing Linux QEMU guest
  under the existing memory cap. They do not run a zkVM proof or establish a
  disposable-guest isolation witness. Native stubs do not commit a proof journal.
- `tests.txt`: source-matched release library integration tests report **7 passed,
  0 failed**. They assert IFC2 and full Safe results for original/direct/helper,
  journal truth, public/declassified, ordinary-method and public-kind controls.
- `clippy.txt`: targeted release clippy with `-D warnings` passed. Changed Rust
  files passed pinned-toolchain rustfmt. `linux-harness.txt`: **16 tests, OK**.
- `source-manifest.json`: source commit, tracked file hashes, toolchain and the
  retained immutable integration-test executable. This executable is a static
  library test instrument, not a published `anubis` CLI or release pin.
- `REVIEW.md`: independent final review and the subsequent test/CI review.

The original fixture is inlined in the Rust test and was checked byte-for-byte
against the recovered source. The native Linux allowlist binds its test-source
hash and exact names, so changing that source requires classification renewal.
The matrix retains the intended outcomes of the registered cases and direct control.
No current full-matrix history is fabricated from these focused tests.

## Remaining boundaries

A full CLI build was started from this code commit; its pending handle belongs
in the continuation checkpoint until it produces a result. No current full
workspace, full matrix, final native-hosted or mandatory guest-journal receipt
exists for this slice. Keep those gates open.

Journal-PC behavior, the conservative deep journal policy, and the differing
native/guest `proof_commit_u32` return behavior remain separate work. This change
does not establish support for first-class proof-builtin runtime values or
correctness of the unchanged `proof_commit_u64` entry.
