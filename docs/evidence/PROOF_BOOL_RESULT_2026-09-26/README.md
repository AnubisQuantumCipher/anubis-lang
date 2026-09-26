# Bool proof-commit result kind — 2026-09-26

Implementation: `b835b94b44fa8705dd973eb1133f862706fd04a8`. This is a bounded
compiler correction with focused library and CLI evidence, not round-2, proof-journal,
platform or mission completion.

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
- `cli-build.txt` and `cli-build.rc`: `cargo build --locked --release -j 2 -p anubis`
  finished successfully with exit status `0`. It began at `b835b94b` and finished
  after the documentation commit `e3b8e1a1`; `source-stability.json` records no
  changed compiler/solver/CLI/vendor/toolchain inputs. The existing source manifest
  remains bound to `b835b94b`. This is not an independent byte-for-byte rebuild.
- `cli-pin.json`: retained CLI
  `/home/sicarii/.cache/anubis-item21/pins/anubis-proof-bool-c91445cdb04faab8`,
  SHA-256 `c91445cdb04faab827925289a1c426ac358d4491ae6732f63d1bfb752a4713e7`.
  This is a source-attributed technical pin, not a release artifact.
- `cli-checks/results.json`, `cli-checks.txt` and `cli-checks.rc`: **14 PASS**
  observations across full Safe `check` and IFC2-only `ifc2-report`; harness exit
  status `0`. Original method-fallback, literal-scalar direct, helper-return and
  secret-truth controls produce `ANUBIS_SECRET_EXFILTRATION`; public, released and
  ordinary-struct controls remain accepted. Each observation retains its command,
  source hash, exit status and complete stdout/stderr. The CLI hash matches before
  and after these checks. `check_cli.py` is the exact retained focused harness.
- `cli-checks/history.tsv`: the full Safe observations appended once to matrix
  history under `proof-bool-focused-b835b94b`; the direct control remains a
  `compare` row. IFC2-only reports are preserved separately in the JSON receipt.
  `cli-followup-validation.json` records static copy/hash/history validation.

The original fixture is inlined in the Rust test and was checked byte-for-byte
against the recovered source. The native Linux allowlist binds its test-source
hash and exact names, so changing that source requires classification renewal.
The matrix retains the intended outcomes of the registered cases and direct control.
The appended history covers only these registered finite bool-result controls;
it does not represent a full-matrix run.

## Remaining boundaries

The full CLI build and focused CLI checks are complete. No current full workspace,
full matrix, final native-hosted or mandatory guest-journal receipt exists for this
slice. Keep those gates open. The technical pin and focused observations do not
establish a release, a new runtime witness or proof-journal execution.

Journal-PC behavior, the conservative deep journal policy, and the differing
native/guest `proof_commit_u32` return behavior remain separate work. This change
does not establish support for first-class proof-builtin runtime values or
correctness of the unchanged `proof_commit_u64` entry.
