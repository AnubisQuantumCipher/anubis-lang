# Anubis Reproducibility (Local Release-Candidate)

## Language core

- `bash scripts/run_language_fixtures.sh --out ...`
- `bash scripts/repro_language_core.sh --out ...`
- 25 fixtures, `fixture_report.json` + `repro_report.json` with `overall_verdict == "PASS"`.

## RISC0 / Metal

- CPU lane (`--lane cpu`) + Metal lane (`--lane metal-hybrid`) on the same pinned reference must produce matching journals for the parity workloads (Gate 11).
- Receipts are verified with the real `risc0_zkvm::Receipt::verify` API.
- Evidence bundles + `verify_bundle.sh` + `MANIFEST.sha256`.

## Full release candidate

`bash scripts/build_release_candidate.sh --metal-reference ... --require-metal --out ...`

Produces a stamp directory containing logs, reports, the release binary copy, and a manifest.

## What "reproducible on this host" means

Given the same:
- checkout at the sealed commit on `a-plus-maturity/20260705-1649`
- same `/Users/sicarii/Desktop/metal-hybrid-prover` (or equivalent via `--metal-reference`)
- same Rust + RISC0 toolchains

... the commands above must yield PASS verdicts and matching journal hashes for parity cases.

No claim is made about bit-for-bit identical binaries across unrelated machines or different OS/LLVM versions (only functional + evidence equivalence for the sealed surfaces).

See `A_PLUS_ACCEPTANCE_CRITERIA.md` and the claim matrix.
