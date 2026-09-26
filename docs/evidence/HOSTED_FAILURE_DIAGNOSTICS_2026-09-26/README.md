# Hosted failure diagnostics

Code commit `e9ee988031df9b09fed9336d721a16536b78d5e3` adds a separate,
diagnostic-only failure artifact. The existing successful hosted validator and
upload are unchanged. The [source and check record](verification.json) binds
the reviewed patch, implementation files and retained test outputs; the
[independent review](INDEPENDENT_REVIEW.md) records its limits.

PR #48's [pull-request run](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36274422605)
failed G3; its [push run](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36274397254)
passed. Their checked-out commits differ, but GitHub's commit records identify
the same tree. The failed run redirected Cargo output to a runner-local file,
then rejected the failed summary before copying minimized artifacts. The
exact underlying test/error was not retained. Neither the passing run nor this
instrumentation diagnoses that failure.

The change captures Cargo's exit code immediately and records bounded failure
identifiers, Rust error codes, observed signal categories and the complete raw
log digest. It excludes arbitrary captured output and stops extraction once
such output begins. Payload names have an `unvalidated-` prefix so existing
release tooling rejects this directory as missing successful-attestation files.
Missing, malformed, omitted and truncated diagnostic data remain explicit.

Lead and independent runs each reported **16 Python tests passing**. The tests
exercise the existing hosted validator, the actual release evidence-copy block,
forged and incomplete summaries, captured-output spoofing, immediate exit
capture, private-output exclusion, file types and extraction limits. Shell
syntax and a local YAML structure check also passed. The workflow runs these
Python tests before its hosted gates. Linux local execution is the only new
platform witness; no macOS or GitHub execution of this patch has occurred.

The first scratch artifact layout could be consumed as success evidence after
its diagnostic marker was discarded; independent review reproduced that
defect. A later parser draft could resume extracting after a fake summary in
captured output. The final filename layout and permanent extraction stop close
those candidate defects under the tested boundaries. Neither draft was
committed or published. This receipt does not claim complete raw-log retention,
hostile same-user confinement, a passing workspace, release approval, or a fix
for the original G3 failure.
