# Anubis Reproducibility (Local Release-Candidate)

## Language core

- `bash scripts/run_language_fixtures.sh --out ...`
- `bash scripts/repro_language_core.sh --out ...`
- 25 fixtures, `fixture_report.json` + `repro_report.json` with `overall_verdict == "PASS"`.

## RISC0 / Metal

- CPU lane (`--lane cpu`) + Metal lane (`--lane metal-hybrid`) on the same pinned reference must produce matching journals for the parity workloads (Gate 11).
- Receipts are verified with the real `risc0_zkvm::Receipt::verify` API.
- Evidence bundles + `verify_bundle.sh` + `MANIFEST.sha256`.

## Legacy local release diagnostic (not publishable)

`bash scripts/build_release_candidate.sh --metal-reference ... --require-metal --out ...`

Produces a stamp directory containing local logs, reports, a mutable-build binary copy, and a
bounded manifest. It is explicitly **not** commit-bound release evidence and cannot authorize a tag
or release. The authoritative path is a clean full commit, `scripts/publish_pin.sh --release`
followed by `scripts/publish_pin.sh --verify-release`,
the source-current VM/offensive/diff refresh, and `bash scripts/run_seal_checklist.sh` as specified
by `docs/evidence/PHASE_1_COMPLETION_2026-07-31.md`.

## What "reproducible on this host" means

Given the same:
- exact sealed commit (normally reached through `main`; never substitute a mutable branch name for
  the recorded full SHA)
- same `/Users/sicarii/Desktop/metal-hybrid-prover` (or equivalent via `--metal-reference`)
- same Rust + RISC0 toolchains

... the commands above must yield PASS verdicts and matching journal hashes for parity cases.

No claim is made about bit-for-bit identical binaries across unrelated machines or different OS/LLVM versions (only functional + evidence equivalence for the sealed surfaces).

See `A_PLUS_ACCEPTANCE_CRITERIA.md` and the claim matrix.
