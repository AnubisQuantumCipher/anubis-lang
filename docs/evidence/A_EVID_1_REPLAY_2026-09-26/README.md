# A-EVID-1: ordered solver replay claims

This is a focused Safe-mode evidence-producer receipt. The exact binary-build
source commit is `e0ba84c6eb7f1e0aa554c96147af32c9ee3c924c` (tree
`78e2283cfbc400f95fea201e9d85b79f43be7499`). Its clean, source-bound
local binary has SHA-256
`3981e97a2ef8d515b42ac32b6e654845e74bb3df6367c9237a2bac4e92268e64`.
The code was cherry-picked into the integration branch as
`47be70a86c1642a0b2f349f5100f1f4e63a53aba`; the changed compiler file has
the same Git blob in both commits. Their complete source trees differ, and the
binary does not attest to a build of the integration tree.
The [manifest](manifest.json) records the pinned toolchain, build and validation
logs, source hashes, complete emitted bundles, and publication hashes. This
technical pin is neither an independent build reproduction nor a release seal.

The [probe](run_probe.py) checked the same harmless [mixed](mixed.anb) and
[no-assertion](empty.anb) sources against the prior pinned binary and this
candidate. It only ran `check --evidence`; no Anubis program was executed.
The [observed results](probe-results.json) and the complete
[baseline](artifacts/baseline) and [candidate](artifacts/candidate) bundles are
published here. Local absolute paths in command output were replaced in the
published logs; the manifest keeps the original ignored log hashes. The
bundles' own `MANIFEST.sha256` files remain byte-for-byte as generated.

On the mixed source, both binaries emitted a proved assertion followed by a
replayed counterexample. The old `solver_replay.json` had only an aggregate
replay claim, so it did not identify which check was replayed. The candidate
emits an ordered row for each
check, bound by index, with `not_applicable` for the proved row and
`counterexample_replayed` for the failed row. On the no-assertion source, the
old sidecar claimed `counterexample_replayed` despite no model. The candidate
records the synthetic `solver:no-obligations` row as
`not_applicable_no_obligations`, with aggregate `not_applicable` and
`replay_valid: false`. A synthetic PASS is not a proof of a program contract.

The producer's focused unit tests, archived PCA-fixture control, format check,
workspace Clippy, release build, and same-source CLI probes passed on the
identified source. An independent agent reviewed the producer diff statically
and found no blocker; this is not external expert audit. The offline verifier
for the new sidecar is a separate follow-up work unit and is **not** claimed by
this receipt. This result does not establish source-to-SMT correspondence or
close the unrelated unresolved solver-status, soundness-matrix, platform,
hosted, clean-room, and release gates listed in the manifest.
