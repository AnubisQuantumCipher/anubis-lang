# No-default-feature CLI lint and Safe run — local receipt, 2026-09-26

Code commit `5e3f76dfb7ef92faf0bb99c7a42e341a29f5df4b` (tree
`42f746203812d602df48ce456ce59e03ba6dda87`) gives the no-`prove`
CLI build the same zero-warning Clippy gate as the default build. It
feature-gates proof-only metadata, codec helpers, an import and an internal
action variant. The no-`prove` error paths remain available with the same
messages, and ordinary `run --input-json` still parses values. No lint
level, feature default, proof policy or isolation rule was changed.

The reviewed scratch patch SHA-256 is
`beed99d9405dd542c00c2ebb508e0dd2234b1a7ba4c0af91b1784c92d9b22923`.
Independent static review gave GO on the exact integrated diff: its only
difference from the scratch candidate is rustfmt whitespace. The reviewer
also confirmed the D9 compiler analysis file was untouched. Review did not
build or execute code.

Lead-owned validation on the exact source bytes found:

| Check | Result | Retained local log SHA-256 |
|---|---|---|
| `cargo fmt --all --check` and `git diff --check` | pass | terminal result; no separate log |
| `cargo clippy -p anubis --no-default-features --all-targets -- -D warnings` | pass | `1c0de314430155a1741c98a1147b37625f6fdaee5ebf0a1d3e39437848111e17` |
| `cargo clippy -p anubis --all-targets --all-features -- -D warnings` | pass | `37f3207fc29fd28056271fb37eb9ed820a6215af8f47e19895e1e1df45bd1372` |
| No-default `proof_input::tests` and `run_input_json` integration target | five unit tests and one Safe native-run test passed | `2752faee1fe5af0f231ba6c809fc7e671f2231b7af70c3538f18ff44b09ba3c0` |

On the committed clean source, `cargo build -p anubis
--no-default-features` passed under the local bounded wrapper. The debug
binary SHA-256 is
`fb75d68e3b14818aed7da894e511353400ae3efc242eadc3965eeb84be10edea`;
the build log SHA-256 is
`59639588a6f1826f129cbcd121edc07279dab94786f5fa5bde6b3439b2e845cc`.
This is a local Linux AArch64 QEMU-guest build, not a hosted platform or
reproducible-release witness.

That binary ran the small Safe source also used by the
[run-input integration test](../../tools/anubis/tests/run_input_json.rs),
using `proof_input_u32("n")` with
`run --input-json '{"n":5}'`; it exited successfully and printed `5`.
The no-`prove` `prove`, `verify-receipt` and internal child commands exited
nonzero with their explicit feature-unavailable errors, before proving or
reading receipt inputs. The versioned [machine-readable smoke result](NO_DEFAULT_PROVE_CLI_2026-09-26-smoke.json) has SHA-256
`0a58f6a1de01fd76b54884e453f64e64330997ed29fd6beb0e43ab0e26bbd413`;
the Safe program source SHA-256 is
`5d38d27d920353cb117cb9b8db941eb3e5ec3f7737728801f80511738c8f0c7c`.
The earlier no-default Clippy failure remains recorded in the
[PCA v4 receipt](HONEST_FAIL_SOURCE_CHECK_2026-09-26.md); this separate code
unit closes that lint failure on the named configuration. It does not claim
a full workspace, proof execution, Research guest, hosted, self-host,
clean-room or release result.
