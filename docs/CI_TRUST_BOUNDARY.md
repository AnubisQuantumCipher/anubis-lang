# CI trust boundary

GitHub Actions proves a bounded hosted statement. It does not prove the Apple Virtualization or
require-Metal statement.

## Required hosted check

`hosted-gate-witness` runs `scripts/audit_unified.sh --profile hosted` on a stock GitHub macOS
runner. The runner derives the exact named 31-gate roster from the script. Every host-verifiable
gate, including the pinned Lean formal gate, must pass. `G9_poc_kit` is exactly `EXTERNAL`, and G14
is limited to its non-executing host-isolation witness. The only successful hosted verdict is
`HOSTED_PASS`.

The successful `hosted-gate-report` artifact contains only `gate_report.json`, `gate_log.txt`,
`profile_environment.txt`, `attestation_identity.txt`, and a checksum manifest. The identity
record binds the artifact to the workflow run, exact commit/tree, and observed Rust, Z3, elan,
and Lean tool versions. Raw gate trees, generated source, test-key material, binaries, and
offensive engagement output are not uploaded by hosted CI.

On a failed job after checkout and successful extractor tests, a separate
`hosted-failure-identifiers` artifact may contain `failure_ids.json`. It is diagnostic only and
does not turn a red job into `HOSTED_PASS`. The workflow runs the extractor's required Python
tests. The extractor validates the gate report
before naming a failure. For G5 it publishes only bounded, sorted IDs of Git-tracked language
fixtures when the report has a complete, consistent row for every tracked fixture. Otherwise it
records a typed unavailable reason. G3 remains `no_reviewed_public_test_id_allowlist` because
raw Cargo logs are not a reviewed source of publishable test names. No raw failure log, source
text, machine-local path, or diagnostic message is copied into the artifact. A failure before
checkout, or a failure of the extractor tests, prevents this artifact. Its absence does not
establish a passing gate or identify a cause. The hosted failure-upload behavior requires an
actual red workflow witness; local extractor tests do not supply it.

## Sealed lanes are out of CI

No persistent self-hosted runner is part of the Phase-1.5 design. The daily signed-in Mac must not
execute public-repository branch code as a GitHub runner. The former active Metal workflow and the
queued sealed-VZ job were removed from `.github/workflows/`; a queued, skipped, or absent runner is
never counted as PASS.

The sealed Tart/VZ battery and require-Metal parity lane are operator-run evidence until a dedicated,
hardened, ephemeral runner is separately approved. Research, crash-capable, fuzz, exploit, agent,
C2, and offensive work must use a disposable guest cloned from `anubis-xcode`; there is no host
fallback.

An operator-run sealed receipt must bind:

- the full commit SHA and a fresh `scripts/publish_pin.sh --release` identity, verified again with
  `scripts/publish_pin.sh --verify-release`, plus its binary SHA-256, metadata, and source manifest;
- the pinned Rust, Lean, Z3, Xcode/SDK, deployment-target, entitlement, and framework identities;
- the exact guest name, vCPU/RAM/job limits, host-resource admission, ordered gate roster, and
  immediate outer exit code;
- fixpoint, validator results, evidence-manifest hashes, teardown/delete result, and final Tart
  absence;
- an independent read-only review tied to those exact artifacts.

The release VZ entry point is `bash scripts/vm/run-slice.sh --release` after
`scripts/publish_pin.sh --verify-release` is green, `RELEASE_PIN="$(scripts/publish_pin.sh --current)"`
resolves that verified immutable pin, and `"$RELEASE_PIN" vz status` is green. Capture each command's
exit code on the immediately following line. A run without `--release` is bounded technical evidence only. The Metal entry
point is `bash scripts/check_metal_parity.sh --require-metal --out <new-evidence-root>`. These lines
describe the contract; they are not evidence that either command ran for a given commit.

## Future runner minimum

A future runner requires separate operator approval and, at minimum, a dedicated host or account,
ephemeral registration, `contents: read`, checkout without persisted credentials, reviewed action
SHAs, one-job concurrency, dedicated Tart credentials, the host-resource guard, disposable guest
teardown, workspace destruction, and verified deregistration. The `metal` label is valid only after
a real require-Metal positive/negative validation.
