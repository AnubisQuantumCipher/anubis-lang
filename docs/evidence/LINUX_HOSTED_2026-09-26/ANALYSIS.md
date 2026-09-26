# First hosted Linux failure analysis

> Archival note: this analysis was written before the follow-up patch. Every “current” source reference and source line number below refers to the pre-follow-up files at `f48a5c9aa63125b9a069c4837bd42693a95a812c`, whose harness bytes also match `ba4b7ec84b24e667f0c43c492c8388b16f717c98`. The original analysis body is retained. Follow-up implementation and local verification are described in [README.md](README.md).

Read-only review of push run [36220630178](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36220630178), source f48a5c9aa63125b9a069c4837bd42693a95a812c. No build, runtime test, workflow rerun, source edit, or threshold change performed. The current linux_native.py, linux_scope.sh, and tools/anubis/Cargo.toml bytes match the source_before hashes in both downloaded receipts. Raw receipts remain untouched.

## What actually failed

### AArch64: cold CLI build timeout, followed by a secondary descendant check

- receipt.json:37 records build-cli elapsed_seconds=3600.019051544; :43 records return_code=-9; :45 records timed_out=true. The configured per-command timeout is 3600. The ordinary Rust test executables were never built or run in this job. Artifact roster remains empty.
- build-cli.stdout.log:643 contains a completed anubis-compiler library artifact. The build had progressed beyond that crate. The RISC Zero Keccak native dependency was still unfinished: :473 is only the custom-build executable for risc0-circuit-keccak-sys@4.0.2, and there is no later build-script-executed or library-artifact event for that package. :536 similarly contains the downstream risc0-circuit-keccak@4.0.5 custom-build executable, without completion. No build-finished success exists.
- This contrasts with the x86_64 build log, whose :651 completes the Keccak native build, :652 emits its library, :653 and :654 complete the downstream Keccak crate, and :657 records build-finished success. That ordering supports an unfinished native Keccak build as the remaining dependency path, not an Anubis ordinary-test failure. The logs lack process snapshots or per-build-script timings, so they do not identify the exact live compiler process or prove its duration.
- scripts/ci/linux_native.py:193 sets timed_out and sends SIGKILL to the Cargo process group, waits for Cargo, then :201 immediately calls quiescent before reporting the timeout at :202. launcher.stdout.log:1 therefore prints unexpected descendants remain instead of the primary timeout. Waiting for Cargo is not a wait for every descendant to finish exiting; this observation alone does not prove process-group escape or an isolation failure.
- launcher.json:5 says load_state_after=not-found, :6 query_exit_code=0, :7 cleanup_required=false, :9 unit_removed=true. Service collection succeeded. RuntimeMaxSec did not terminate this run: the driver returned its own failure after the command timeout. The closing memory.events snapshot was never saved, so this receipt cannot establish whether memory-limit events or OOM occurred before timeout. SIGKILL here is explicitly the harness timeout action, not evidence of OOM.

### x86_64: completed builds and ordinary tests, then strict memory-event refusal

- Every recorded command has return_code=0 and timed_out=false. build-cli elapsed_seconds=2472.084426403 (receipt.json:92); build-cli.stderr.log:825 reports Finished release profile in 41m 11s. Cargo build-finished=true is present.
- receipt.json:437 records memory.events initially as low=0, high=0, max=0, oom=0, oom_kill=0, oom_group_kill=0. :436 records closing max=337; the other fields remain zero. The equality assertion at scripts/ci/linux_native.py:283 correctly refuses this run under its declared strict policy.
- The kernel defines max as attempts to go over the memory maximum; OOM follows only if direct reclaim fails. Therefore these counters show memory-cap pressure, not an observed OOM kill. This distinction does not authorize converting the lane to PASS. See [Linux cgroup-v2 memory.events](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
- The receipt captures memory counters only around the entire run. It cannot locate those max events in the CLI build, integration compilation, or test phase. Heavy native dependency compilation is a plausible contributor, not a measured attribution.
- launcher.json records successful removal just as on AArch64. The job failed its payload resource condition; teardown did not fail. The systemd summary's Memory peak: 4.0M is not adequate evidence to override the recorded cgroup event counters; no independent memory.peak capture exists to explain that summary.

## Evidence retention and diagnostic defects

All recorded command stdout/stderr files exist and their downloaded bytes match their receipt SHA-256 fields on both architectures. There is no demonstrated upload-retention bug. Repository .gitignore:70 ignores *.log, so default rg --files omits those files; use an ignored-file-aware listing for inspection.

The actionable diagnostic defects are:

- scripts/ci/linux_scope.sh:42 treats a nonzero payload/service exit as grounds to overwrite receipt.error at :46 with launcher or teardown failed, even when unit_removed=true and the teardown query succeeded. Preserve the original payload error. Put launcher/service return status and actual teardown errors in separate fields; continue to fail the job.
- scripts/ci/linux_native.py:201 lets post-timeout quiescence mask the primary timeout, although commands already preserves timed_out=true. Keep the primary timeout/failure alongside any residual-descendant observation. A finite drain check after kill may remove an exit race; any surviving descendants still fail, and no later command runs after a timeout. Keep the outer owned-cgroup cleanup and receipt finalization requirements.
- scripts/ci/linux_native.py:282 takes the closing memory snapshot only on the success path. Capture memory.events and memory.peak on command boundaries and in the exception path before the service disappears. Capture residual PID status/command data on failure, bounded to the owned cgroup. Do not reset counters or weaken the existing unchanged-counter gate. This supplies the missing attribution for the next authorized observation.

## Smallest build change to consider, with explicit uncertainty

The current payload hardcodes CARGO_BUILD_JOBS=2 and RAYON_NUM_THREADS=2 (scripts/ci/linux_native.py:242), and strips CC/CXX from the inherited environment (:239). The workflow installs clang (.github/workflows/linux-native.yml:46) but does not select it. The CLI retains default prove features (tools/anubis/Cargo.toml:38); this pulls in the native RISC Zero kernel compilation even though the ordinary allowlist does not invoke a proof workload.

A narrow first candidate is to explicitly select an installed native C++ compiler, CXX=clang++, inside the sanitized payload environment and record its path, version, and digest. Keep the same Rust release build, default features, source binding, CPU/memory/swap/process limits, command timeout, and memory-event refusal. The source-supported reason this reaches the remaining work is that risc0-circuit-keccak-sys-4.0.2/build.rs:32 uses KernelBuild::Cpp for kernels/cxx/*.cpp; risc0-build-kernel-2.0.1/src/lib.rs:141 uses cc::Build.cpp(true), and cc-1.2.65/src/lib.rs:184 documents CXX selection. These dependency sources were read from the local Cargo registry; they are not hosted compiler identity evidence. The hosted run did not record its C++ compiler identity. A speed or memory improvement from changing compilers is a hypothesis requiring a new authorized hosted result, not an established fix.

CARGO_BUILD_JOBS=1 is a valid serialization candidate without relaxing caps and may reduce overlapping compiler working sets. It is not proven to eliminate the max events, and can worsen the AArch64 timeout. The current logs do not measure individual compiler memory or establish that simultaneous work rather than an individual compilation unit caused pressure. Do not promise that serialization alone solves both failures.

A keyed cold-build cache is a larger alternative. A cache must include native architecture, toolchain identity, dependency/lockfile and vendored-source content, release/features/compiler settings; all current-head compilation and exact tests still run and receive new receipts. Reusing a previous CLI or receipt as current-head evidence is not acceptable. Because the current harness requires a fresh target directory, introducing target-cache restore needs an explicit invariant/design change, not an incidental workflow cache addition. Cached success also cannot establish cold-build viability. No particular larger cold-build timeout is evidence-backed by these logs: the unfinished AArch64 job gives no completion time. Keep any proposal for a separate finite cold-build budget explicit and reviewed rather than silently raising the ordinary command limit.

No default-feature removal, RISC0_SKIP_BUILD_KERNELS, relaxed max-event check, host research/fuzz/crash execution, or fabricated Linux PASS is justified by this investigation.

## Approval boundary

The existing FAIL verdicts stand. Hosted runner admission, pinned Rust/Z3 startup, cgroup admission, harmless harness controls, and service collection worked in the observed jobs. x86_64 also completed its recorded ordinary command roster; it did not satisfy the declared resource gate. AArch64 did not produce a completed CLI build. Proposed compiler selection/serialization/cache improvements are unexecuted. Only this analysis file was written.

## Concrete first follow-up design

The proposed first controlled build experiment is the explicit native Clang compiler pair, with CARGO_BUILD_JOBS=2 and RAYON_NUM_THREADS=2 unchanged. Changing concurrency at the same time would confound attribution; serialization remains a separate candidate if measured pressure persists. Select CC from the absolute clang invocation path and CXX from the absolute clang++ invocation path in the sanitized payload environment. Keep the invocation paths distinct from symlink-resolved identity paths: resolve only for file hashing and ELF metadata, preserving the clang++ driver name when invoked.

Record each compiler's invocation path, resolved path, SHA-256, --version output, and native ELF identity. Receipt validation should require that compiler records match the declared compiler policy and payload environment, and reject missing/mismatched identities. This adds observed tool provenance; it does not invent an upstream preauthorized digest. Retain pinned Rust, default features, exact Cargo/test argv, classification, resource limits, command timeouts, unchanged memory-event policy, current-source comparison, and finalization after successful teardown.

Static compatibility support is the upstream dependency's use of cc::Build with C++ mode and flag_if_supported for its C++ flags. No compiler-specific blocker was found in that inspected path. This is source support for trying Clang, not a successful Clang build claim.

In the same follow-up, improve failure reporting without changing acceptance: keep timeout/failing-command status primary; retain post-kill residual descendants separately; perform only a finite exit-drain check before relying on service teardown; capture memory.events and memory.peak at command boundaries and on failure; preserve payload error when launcher records a nonzero service exit after successful collection. Add harmless mocked controls for error preservation, compiler identity substitutions, timeout/drain ordering, and resource snapshots on failure. No real compiler build, service, crash, or fuzz payload belongs in those controls.
