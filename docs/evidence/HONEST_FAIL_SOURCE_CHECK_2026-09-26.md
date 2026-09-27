# Source-derived check evidence — local receipt, 2026-09-26

Code commit `fc72526b5477f6475d33adf3d86ad02a467b1046` (tree
`7c671193777e5ee6f7f646b6fc9fc013dde01463`) introduces PCA v4 check
scope. A Safe `check` refusal can now carry an honestly recorded `FAIL`
verdict that `anubis verify` re-derives from the sealed source. This is
evidence that the named checker refused that source under the recorded
configuration; it does not turn the program into a PASS, establish native
runtime behavior, or make the same compiler's re-derivation independent of
compiler defects. Parse, invalid-typecheck and undecided outcomes cannot be
rehash-laundered into a verified check. An empty or unencoded obligation
inventory records `solver_execution: not_run`; the aggregate backend is
`unobserved` until per-obligation authority is captured.

The current `source_check_v1` scope re-derives the source verdict, command
refusal, ordered check rows, analysis and presentation sidecars, and default
check configuration. A single-module Safe package requires a separate
`package_publish_v1` claim plus mounted package identity before verified
dependency admission. Multi-leaf source snapshots, unresolved mode
elevators and nondefault check configurations are withheld from current
check authority. Rehashed unexpected attachments and unknown or duplicate
environment fields are refused. Historic PCA v2 source claims remain
inspectable; a version-specific v3 migration probe was tested on the current
layout, but an archived v3 producer bundle was not available for this check.

Independent static review of the exact integrated diff gave GO for lead-run
source-bound tests. The frozen v15 scratch patch has SHA-256
`b984a724602e0e2137e65be988ef5e101b225ee2caade7c2d09763417efe2a90`.
The reviewer inspected the two lead fixes after that patch: retaining raw
canonical environment bytes alongside its
parsed fields, and narrowing a test-only import to its matching feature
configuration. This is not an external expert audit.

On the clean code commit, the focused CLI integration target passed five Safe
tests and left its Research-mode test ignored for the required disposable
guest. Four selected compiler-library tests passed: honest source refusal,
canonical unrejected FAIL denial, mounted package identity, and cold checking
of the migrated real PCA v2 receipt fixture. The retained focused-test log
has SHA-256
`e156ccc2e700ab03773092d2bb9a81193576c7d17c75279e8a6c096c11e148f6`.
Additional development-time evidence and package unit tests passed before
the final commit; they are not counted as a source-bound release gate.
`cargo fmt --all --check`, `git diff --check`, and default-feature
`cargo clippy --workspace --all-targets -- -D warnings` passed; the Clippy
log SHA-256 is
`2a65698287454adb7849b105f04c33f6af20590f1514586ac3b9118e49b62cb7`.
The canonical documentation-drift gate passed with zero drift; its local log
SHA-256 is
`aa8698d2a31ed3fce04d696e135c32baf1da6def475d56492fa057e83db4e43d`.

The committed source built with the repository-pinned
`rustc 1.97.0-nightly (82bee9650 2026-05-09)` on Linux AArch64 in a QEMU
guest, with Z3 4.16.0 available. `cargo build -p anubis --release` exited
successfully under the bounded local build wrapper. The release binary
SHA-256 is
`98e8090a07716e4876c65422c368e373d92717456edc235f296d8b687f17aac0`;
the build log SHA-256 is
`11523de798dfcf21fb4afc870d8256e444ef15912bceada861616ae65758c843`.
This is a local binary identity, not an independently reproduced or hosted
artifact.

Using that binary, the Safe source from
`safe_information_flow_refusal_has_checked_negative_evidence_and_clean_twin`
exited nonzero for tainted egress and emitted `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`.
Its bundle recorded PCA v4 `FAIL`, `security_policy_finding`, and
`solver_execution: not_run`; both `anubis verify` and `anubis evidence-verify
--json` accepted the integrity and source-check claim while retaining the
recorded `FAIL`. The declassified clean twin exited successfully and recorded
PCA v4 `PASS`, also with no solver query. Both bundle checks exited
successfully. The local machine-readable smoke result has SHA-256
`cd435d06d105238ee8f4f35587c336ed67d2c81baf40352d834c0fd0a3724c35`;
the source hashes are
`386315c0d4089cfea4aff2ea14fbc8208e2c4587171a9fc2412aa01e7d06194f`
and
`ded238d531c9b5acd0f4c8f2094668d3ddbb0e537cc25e9f0defb225a100f982`.
The integration test contains the exact reproducible sources and commands.

An extra `cargo clippy -p anubis --no-default-features --all-targets -- -D
warnings` attempt **failed** on feature-gated CLI warnings; its retained log
SHA-256 is
`a5eaf316e7c23a3943b4900ffb880971b2672798b82100d8bb1d91af1964070c`.
That optional configuration has not been banked as a clean gate. No full
workspace, matrix, Research guest, hosted platform, clean-room, or release
result is claimed for this commit. The full trust-chain correspondence and
per-obligation solver provenance remain open.
