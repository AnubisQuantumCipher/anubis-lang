# Independent final review: hosted failure diagnostics

Decision: **GO for the bounded CI observability change** represented by the frozen v3 patch. This decision is not a hosted-gate pass, compiler release approval, or permission to publish. No remaining must-fix issue was found in this final diff within the boundaries below.

## Exact reviewed candidate

- Patch: `../lead-candidate-v3/applied.patch`
- Patch SHA-256: `b7685e67ba3a9d647fb6b6cd68a84d0eb913f2de9e8b05775ef3ee897a9b6d0c`
- Reported base commit: `e34d0c89c2181e8bd02ff52b0ab5d50cf97bac4b`
- Workflow SHA-256: `937f2381e0737380d528eafd81e97d9be1bb4097a0d43b0b8c33ce5a8d65f9e6`
- Gate script SHA-256: `e4228edef2b12e0ac6f2ac9b0024bdf4fbb4e6a497aca702ffab916f04f6d802`
- Packaging helper SHA-256: `c7a32d30be596288b8624b95612543fe11a7a87d8e33ba2126a89560f7f7ccac`
- Python tests SHA-256: `a8415e069441ea86373cdab422e6f24a0ae694b2503194355ac50c76415e7b9c`

The full helper, tests, workflow changes and gate-script changes were read. The frozen patch hash was independently checked, and the final source bytes were checked against its adjacent manifest before copying to `final-v3-copy/`. No git command, compiler build, compiler execution, shared source edit or external write was performed by this reviewer.

## Findings resolved in the final diff

The original candidate retained success-artifact filenames. A controlled reproduction demonstrated that a false `HOSTED_PASS` report with a failed gate could be copied through the unchanged release consumer while its diagnostic marker was discarded. The final helper writes `unvalidated-` names, and an executable negative control invokes the exact evidence-copy block extracted from the existing release builder. That consumer now rejects the diagnostic directory as missing `gate_report.json`. This closes the introduced layout-confusion path; it is not a claim that every pre-existing release consumer independently validates the full gate roster.

The earlier combined parser could resume extraction after arbitrary captured stdout printed a plausible summary and test header. The frozen earlier patch reproduced export of a synthetic private marker and error code. In the final helper, entering captured output permanently stops extraction while hashing continues. The final controlled probe preserves the legitimate failure identifier, omits the synthetic marker and error code, and records the stopped state. Earlier HOLD reports and reproductions remain unchanged in this directory.

The final script captures the Cargo exit status as the first command in both branches. No gate grading or roster changed. The new artifact exports the full raw-log digest, bounded ASCII test identifiers, bounded Rust error codes, explicit literal signal categories and typed missing/malformed states. Raw G3 bytes and arbitrary panic/compiler text are not copied. Signal identity is not inferred from an exit code. Missing or truncated extraction does not establish absence of failures.

The existing success-attestation validator and successful artifact upload are byte-for-byte preserved. The workflow adds a mandatory Python self-test before host gates, and a separate failure-only preparation/upload path. Removing that self-test insertion leaves the original workflow as an unchanged prefix followed by the diagnostic steps. Packaging success cannot convert a failed prior step into a successful job. If failure occurs before G3, the helper records missing G3 evidence.

The fixture root now resolves the physical temporary directory before creating explicit symlink controls. This addresses the `/var` temporary-path alias pattern without relaxing the helper's parent-symlink rejection. This is source review plus Linux execution, not a macOS execution witness.

## Independent verification

`python3 scripts/test_hosted_diagnostics.py` was run from the verified private copy with Python bytecode writes disabled. It exited `0`; its output reports `Ran 16 tests` and `OK`. These controls cover failed/malformed/forged reports, unchanged hosted validator decisions, the exact release consumer rejection, raw/private output exclusion, manifest binding, omitted symlink/FIFO inputs, explicit missing evidence, bounded extraction with a full digest, captured-output forgery, and immediate exit capture using an extracted shell block with a controlled subprocess.

- Receipt: `final-v3-tests.json`
- Complete stdout/stderr: `final-v3-tests.stdout`, `final-v3-tests.stderr`
- Stderr SHA-256: `54a747705850e51ae7e7d66ab34c9545fddc1997036cd41cec2b528ba34b6888`
- Independent captured-output probe: `captured-escape-final/RESULT.json`, bound to the same final helper SHA-256.

No Rust workspace gate, full hosted workflow, actual GitHub artifact upload, macOS job or release build was executed by this reviewer. The lead's separate shell-syntax and diff checks are reported in the candidate directory, not substituted for these independent tests.

## Trust and diagnostic limits

The helper assumes a quiescent CI output directory. Its symlink/nonregular checks, no-follow opens and inode/stability checks do not claim confinement against an active hostile namespace writer or hardlink substitution. The existing minimized-input filename allowlist is not a content sanitizer for those existing surfaces. GitHub identity fields are allowlisted; diagnostic observations remain unvalidated.

The bounded parser recognizes syntactic log fragments. It does not authenticate the process that printed them, and test output emitted outside normal libtest capture could imitate those fragments. The marker states that limitation. It intentionally sacrifices later extraction once arbitrary captured output begins. Raw bytes remain excluded from publication and their complete digest is retained.

The original hosted G3 failure's exact failing test or infrastructure cause remains unknown. This change prepares useful evidence for a future failing run; it does not establish that earlier failure's cause, fix it, or demonstrate a passing hosted candidate.
