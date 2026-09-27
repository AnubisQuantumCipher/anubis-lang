# CI trust boundary

GitHub Actions proves a bounded hosted statement. It does not prove the Apple Virtualization or
require-Metal statement.

## Required hosted check

`hosted-gate-witness` runs `scripts/audit_unified.sh --profile hosted` on a stock GitHub macOS
runner. The runner derives the exact named 29-gate roster from the script. Every host-verifiable
gate, including the pinned Lean formal gate, must pass. `G9_poc_kit` is exactly `EXTERNAL`, and G14
is limited to its non-executing host-isolation witness. The only successful hosted verdict is
`HOSTED_PASS`.

The workflow publishes only `gate_report.json`, `gate_log.txt`, `profile_environment.txt`,
`attestation_identity.txt`, and a checksum manifest. The identity record binds the artifact to the
workflow run, exact commit/tree, and observed Rust, Z3, elan, and Lean tool versions. Raw gate
trees, generated source, test-key material, binaries, and offensive engagement output are not
uploaded by hosted CI.

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

## Supplemental native Linux ordinary checks

The `linux-native-x86_64` and `linux-native-aarch64` jobs in `linux-native.yml`
build and execute only the source-reviewed finite integration-test roster. Their
claim is `LINUX_NATIVE_ORDINARY_PASS`, separately for each observed architecture;
it does not satisfy `HOSTED_PASS`, the full workspace, Tart/VZ, crash/fuzz,
Omarchy installation, bootstrap, or release requirements. The driver requires
resource admission, source and executable identity, exact test results, and
completed teardown before finalizing a receipt. See the
[lane contract](../scripts/ci/linux_native.md) and
[preparation receipt](evidence/MISSION_RESUME_2026-09-26/README.md).

## Future runner minimum (specialized self-hosted lanes)

A future runner requires separate operator approval and, at minimum, a dedicated host or account,
ephemeral registration, `contents: read`, checkout without persisted credentials, reviewed action
SHAs, one-job concurrency, dedicated Tart credentials, the host-resource guard, disposable guest
teardown, workspace destruction, and verified deregistration. The `metal` label is valid only after
a real require-Metal positive/negative validation.
