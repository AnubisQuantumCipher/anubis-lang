# Linux hosted failure evidence and bounded follow-up success

The first native Linux push run [36220630178](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36220630178), at `f48a5c9aa63125b9a069c4837bd42693a95a812c`, failed on both architectures. These are retained failures, not a Linux completion receipt.

- AArch64 timed out during the cold CLI build. The timeout was recorded, but a later descendant check masked its primary error. No closing memory counters survived, so the receipt cannot establish whether memory pressure or OOM occurred.
- x86_64 completed its recorded build and ordinary test commands, then failed the strict unchanged-memory-event check. The receipt records `max=337`, with OOM counters unchanged at zero. Service collection succeeded on both architectures.

[ANALYSIS.md](ANALYSIS.md) preserves the independent investigation against the pre-change source. [download-validation.json](download-validation.json) records rehashing of every command log named by the downloaded receipts. The raw [push artifacts](push-36220630178/), [run metadata](push.json) and [failed-job log](push-failed.log) are retained without rewriting. This integrity check does not change either FAIL verdict.

## Follow-up source and local verification

Code commit `b30001bf381251a7526194cf92fe805ca063da61` explicitly selects native `clang` and `clang++` inside the sanitized payload, preserving invocation names and recording resolved executable identities. It retains primary payload errors, adds command-boundary and failure resource snapshots, and shares a finite timeout-exit budget between leader waiting and descendant draining. Receipt schema is `anubis.linux-native-ordinary.v2`.

The existing Cargo/test command roster, default features, concurrency, resource limits, timeouts, strict memory-event gate and owned-service collection requirements remain unchanged. Selecting Clang is an experiment whose build-time and memory effects require a new hosted result.

- [Harness controls](harness-tests.txt): `Ran 27 tests`, `OK`; [exit code](harness-tests.rc) `0`. These use synthetic receipts, mocked process operations and harmless launcher stubs. No native CI build, service, crash or fuzz workload was run locally for this follow-up.
- [Local validation](local-validation.json): Python AST parsing, shell syntax and changed-source whitespace passed. Static comparison found the declared build/test functions and systemd property lines unchanged.
- [Docs drift](docs-drift.txt): `PASS (38 stamps checked, 0 drift)` after the claims and inventory updates.
- [Independent review](INDEPENDENT_REVIEW.md): no blocking finding; reviewed file hashes match the local validation receipt. The reviewer did not execute tests or services.

The enclosing documentation commit does not alter the tested code. The following hosted
result was obtained afterward; full mission acceptance, release readiness and guest
seals remain open.

## Hosted follow-up at `a00387d9`

[Run 36224977907](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36224977907)
completed successfully for both `linux-native-x86_64` and `linux-native-aarch64`.
The pull-request merge commit used by the runner has the same Git tree as source
head `a00387d9097d60bafe076abed0133f0558e71b72`. Both receipts report
`LINUX_NATIVE_ORDINARY_PASS` under the unchanged cgroup limits and ordinary
command allowlist. Downloaded receipt logs matched every recorded SHA-256;
all named commands returned zero, none timed out, source and binary snapshots
were unchanged, memory-event counters were unchanged, and each owned service
was removed without a teardown error. [The compact checked summary](success-summary.json)
records the source, toolchain and receipt identities without copying raw logs or
runner-local paths into this commit. The duplicate [push run](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36224975780)
also completed successfully on both architectures.

This is the first successful hosted witness for this *bounded ordinary Linux
lane*. It is not a full workspace test, full soundness matrix, Omarchy
installation, disposable-guest replay, VM/Metal seal, or release witness.
The initial failed run and its primary timeout and memory-event observations
remain part of the record.
