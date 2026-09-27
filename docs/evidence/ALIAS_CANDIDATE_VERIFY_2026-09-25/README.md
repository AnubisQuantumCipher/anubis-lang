# Alias candidate verification failure — 2026-09-25

The uncommitted alias candidate did not pass workspace verification. The complete
[workspace log](workspace-release.log) records a stack overflow in
`recursive_closure_source_reports_limit_and_recovers`, in the compiler's
`closure_analysis_limit` integration target. Cargo finished the other targets under
`--no-fail-fast` and returned `101`; this is not a passing workspace receipt.

The candidate is reconstructed by applying [candidate.patch](candidate.patch) to
`2990eab2c6ad856fd2f1bfc25fa77c3e6a60b92a`. The [source manifest](source-manifest.json)
records the source hashes and inherited CLI pin identity. The tests compiled with
repository-pinned Rust nightly; their executable is distinct from the CLI pin.
[receipt.json](receipt.json) binds the retained artifacts and execution conditions.

The verification driver preserved complete subprocess output and immediate return
codes. After inspecting the failure, the lead stopped its owned systemd scope during
solver-debug. Matrix, corpus and gate verification were not completed by that job.
The retained [driver](verify_unit.py) is an execution record, not authorization to
repeat crash-capable checks on the host.

This was a local capped run on the current QEMU guest, not the mandatory disposable
Tart/VZ witness. Further deliberate crash/stress verification requires the mandated
guest or an explicitly authorized, reviewed equivalent. Do not claim a passing
isolation gate from this failure. The candidate-versus-baseline differential has
not been measured under matching conditions, so whether alias introduced this
failure remains unresolved.

This failure is separate from the known `rv30_valid_n02` precision regression:
that case reaches the closure-analysis limit rather than proving acceptance of a
required valid program. Neither failure is resolved by publishing this receipt.
