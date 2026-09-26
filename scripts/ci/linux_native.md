# Native Linux ordinary CI

`linux-native.yml` adds supplemental, separately named `linux-native-x86_64` and
`linux-native-aarch64` jobs. Each receipt covers only its observed architecture.
The workflow succeeds only if both matrix jobs succeed; there is no combined
receipt or branch-protection change. Missing, failed, cancelled, or pending
architecture evidence is not a claim that both architectures passed.

The lane builds the default-feature CLI and only the integration targets in
`linux_ordinary_manifest.json`. That manifest records a static source review of
fixed, finite ordinary tests. Their source hashes and exact test names must still
match before compilation. Changed test sources require renewed classification;
the manifest is not a defect registry. Known crash, recursion-exhaustion, stress,
fuzz, research and full-workspace tests are excluded from this execution scope.
Their existing required gates remain pending until separately executed in the
authorized disposable guest. No existing G1–G31 gate, Tart/VZ policy, Metal gate,
Omarchy installation witness, or release requirement is weakened or satisfied by
this supplemental lane.

`linux_scope.sh` launches the driver in a transient system service, explicitly
passing the runner's HOME and tool locations. It applies memory, swap, CPU,
process, core-dump and wall-time limits to the entire descendant cgroup. Before
any build or test, the driver verifies its actual cgroup-v2 membership and
memory/swap/CPU/process limits. It refuses missing controls and does not fall
back to an unbounded command. The launcher requires service collection after
`systemd-run --wait`; a query failure or surviving unit fails the job. The
launcher retains its immediate exit status and teardown query status on failure.
Its exit/signal trap stops only the owned unit; required cleanup still makes the
job fail, even when cleanup succeeds. A forced kill that bypasses the trap leaves
no finalized PASS receipt; the service also has its independent runtime limit.
These are resource controls, not a VM, security sandbox, or Tart-equivalent seal.

All builds finish before the driver freezes the CLI and test executables and
records their hashes and native ELF architecture. It checks them before and after
each exact libtest command.
Cargo's JSON build-finished result and executable roster must match the invoked
commands and the executed test paths. The libtest event stream must show precisely
the requested test passing; an empty suite, duplicate, wrong test or incomplete
summary fails. These source-reviewed tests assert their native outputs and evidence
semantics internally. Their temporary generated native programs are not retained
as separately reusable instruments; the receipt identifies the CLI and test
executables, source inputs, and complete test process logs.

The receipt binds clean tracked source files, commit, tree, classification,
compiler/tool versions, observed Linux architecture, image, libc, tool environment,
on-disk TMPDIR, resource settings, executable hashes, and command exit/signal/timeout
and log hashes. It requires unchanged source, instruments, and memory event counters.
Validation requires a caller-supplied SHA and architecture; live validation also
compares the recorded source manifest with the current checkout. `out/linux-native`
is ignored output, not an input to that tracked source manifest. `GITHUB_SHA` is
the actual checkout expectation: for a pull request this is its merge commit,
not the source branch head. `job.json` records the event, ref and pull-request
source head separately. Receipts and logs are workflow artifacts even on failure.
The payload first writes `PAYLOAD_COMPLETE_PENDING_TEARDOWN`. Only `finalize`,
after launcher teardown and a final current-source comparison, writes the PASS
claim and successful final-validation status. Ordinary validation requires both;
a final validation failure downgrades the receipt to FAIL.

The claim is `LINUX_NATIVE_ORDINARY_PASS`, never `HOSTED_PASS`, full seal, proof of
absence of defects, or Linux-wide support. The versioned receipt retains explicit
external requirements. A receipt is technical CI evidence from this workflow,
not an independent witness or the project's release-pin publication contract.
The current implementation still needs real native CI observations; script/control
tests alone cannot establish platform admission, successful compiler builds, or
native test outcomes.

## Tool provenance and runner prerequisites

The workflow reuses the existing checkout, Rust-toolchain and upload action commit
pins and Rust nightly. GitHub's [runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
lists `ubuntu-24.04` as x64 and `ubuntu-24.04-arm` as Arm64. The driver independently
requires the expected `uname` architecture and native Rust host triple; no cross
target or emulation command is used. Availability of the labels, systemd transient
services, cgroup-v2 controllers, sufficient disk, and successful pinned dependency
bootstrap remain infrastructure prerequisites, not assumed passing results.

The Z3 archive SHA-256 pins below were read on 2026-09-25 from the `digest` fields
of the official [Z3 release API](https://api.github.com/repos/Z3Prover/z3/releases/tags/z3-4.15.4),
for the [4.15.4 release](https://github.com/Z3Prover/z3/releases/tag/z3-4.15.4).
They are upstream metadata observations, not locally measured archive hashes.
The workflow downloads and verifies the actual archive against the corresponding
pin before adding its executable to PATH, and records the used archive checksum
and executable hash.

| Asset | API asset ID | SHA-256 |
|---|---|---|
| `z3-4.15.4-x64-glibc-2.39.zip` | `310207451` | `a41b690e89c343931471506cdc6d957b6044a200fd2d240cc017432afdff7d3e` |
| `z3-4.15.4-arm64-glibc-2.34.zip` | `310207441` | `9e832578e28d9ed51a79b97948728a874854a3c38ee49e7aae05e7d6e0e93508` |

## Harmless harness controls

`python3 -B -m unittest discover -s scripts/ci -p test_linux_native.py` checks
synthetic receipts and mocked launcher failures. It does not build the compiler,
invoke Anubis, create real services, or execute crash payloads. It tests refusal
of changed sources, stale receipt expectations, missing/failed commands, changed
instruments/logs, invalid cgroup membership/caps, missing tests, unexpected Cargo
artifacts, and missing teardown. The synthetic successful fixture has deliberately
fictional source and executable identities and is not execution evidence.

Broader Linux coverage, an aggregate downloadable receipt verifier, and any Linux
guest-equivalent backend remain proposals outside this initial implementation.
Current Tart policy remains authoritative until an explicit reviewed revision.
