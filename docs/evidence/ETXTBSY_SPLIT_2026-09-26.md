# ETXTBSY spawn-retry extraction from PR 44

Code commit: `c04d2a59655649af54bf77e7a28574ceb90330ba`, based directly on
`main` at `e34d0c89c2181e8bd02ff52b0ab5d50cf97bac4b`. Only
`compiler/src/backends/run.rs` and `compiler/src/lib.rs` changed in the code
commit. Follow-up code-and-test commit
`d764222badbc71f028532af3eee37a6afcd83ee9` restores the exact REPL's
prior stdin behavior in both timeout modes. These commits extract the
executable-busy test/build repair from the larger draft integration branch
for independent review.

The helper retries `ETXTBSY` only while spawning a child, with a finite budget.
Other spawn errors return immediately. Once a spawn succeeds, output capture or
wait failure cannot execute that program again. The unbounded and timed
`run_child_capped` paths both capture stdout and stderr; the timed path retains
its existing watchdog. Current production callers choose stdin explicitly,
including null stdin for the Cargo build path and the unbounded exact REPL
path. The timed exact REPL path inherits stdin as before. A caller that leaves
`Command` stdin unspecified inherits stdin under `spawn`, as the function
documentation states.

At the respective code commits, the lead ran focused release-library tests
at `c04d2a59` and the exact REPL test at `d764222b`
under a memory-capped user scope on Arch Linux ARM/AArch64 with
`rustc 1.97.0-nightly (82bee9650 2026-05-09)`:

| Filter | Observed result |
|---|---|
| `exec_busy_` | exit 0; both synthetic retry/budget tests passed |
| `run_child_capped_unbounded_captures_output` | exit 0; passed |
| `run_child_capped_returns_output_when_program_is_fast` | exit 0; passed |
| `read_line_reads_stdin` | exit 0; passed |
| `build_of_program_with_main_emits_faithful_runnable_artifact` | exit 0; passed |
| `cargo test --release -p anubis --test repl_exact_stdin -- --test-threads=1` at `d764222b` | exit 0; timed inherited-input and unbounded EOF controls passed |
| `cargo fmt --check` at `d764222b` | exit 0 |
| `bash scripts/run_docs_drift_gate.sh` on this documentation tree | exit 0; 53 stamps checked, zero drift |

The compiled release-library test binary had SHA-256
`a236d32f8326c3ead93e2511b7d68e7ff3af25d9928bd1dfe2f2969ae417ed5e`.
The final exact REPL test binary had SHA-256
`04600ca7ba82ebc59fe35e8c1a8fa142c5ac16d8c473d614cb13a941b44daa9b`.
Focused command logs are retained in the local ignored `out/etxtbsy-split/`
folder; the source identities, commands, exit codes, and relevant results are
recorded here because that folder is not durable evidence. This is a focused
local source/build observation, not an independent reproducible-build or
release seal. An independent read-only agent reviewed the final code diff and
found no actionable issue; human review is still needed.

`cargo clippy --release -p anubis-compiler --lib --tests -- -D warnings`
exited 101 at the code commit: `clippy::needless_return` on the Linux path in
`compiler/src/backends/run.rs` (the `return compile_and_run_items_with_mono`
line). The same line exists unchanged in base `main`; this extraction does not
claim a Clippy pass. The broader mission branch already carries a separate
Linux lint correction. Full workspace, macOS, disposable-guest, and release
gates were not run for this main-based slice. No native stress, exploit, or
crash-capable test was run on the host.
