# Changelog

All notable changes to Anubis are recorded here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Honesty rule.** This file follows `docs/CLAIMS.md`. A claim here stronger than the one there is a
> bug in the claim. Soundness closures are listed under **Fixed** only when they are backed by a
> 0-flip verdict-diff, an over-rejection guard fixture, a VM seal, and an empty audit re-run on that
> surface — not when a fixture goes green. Six items once passed the first three and were still open.

## [Unreleased]

`anubis check` passing means Anubis found no way for the program to violate its stated policy, and
refused what it could not decide. It does **not** yet mean the program cannot violate that policy —
see `docs/CLAIMS.md` item 21 and the phased blueprint.

### Added
- `.gitattributes`, `.github/CODEOWNERS`, `.github/dependabot.yml`, and this changelog — repository
  operating surface brought in line with how the project is actually developed.
- `tools/host_exec_guard.py`: the destructive rule is now pinned by the self-test in both
  directions (15 must-block, 8 must-allow cases).

### Fixed
- `tools/host_exec_guard.py` decided on raw command **text** instead of resolved **targets**, which
  failed in both directions: it missed `rm -rf "$VAR"`, `$HOME/…`, `${HOME}/…`, `-fr`, `-r -f`,
  `--recursive --force`, and `$(…)` substitution, while wrongly blocking every absolute path
  including `/tmp/…`. Now parses the invocation, resolves the target, and refuses when the target is
  unknowable — fail-closed-on-unknown applied to the project's own tooling.
- Three gate scripts (`run_promise_coherence_gate.sh`, `run_proof_correspondence_gate.sh`,
  `run_native_shadow_gate.sh`) were mode `0644`, so direct invocation returned exit 126 and read as
  a gate failure.

### Known open
- **The enforcement lanes are not total.** Confidentiality and integrity cover a subset of the AST
  while the effect and capability lanes cover all of it; carrier-routed programs can pass `check` and
  violate at runtime. Tracked as `docs/CLAIMS.md` item 21.
- `compiler/src/evidence/mod.rs` reports a taint field that is an alias of the typecheck result
  rather than an independently computed verdict.
- CI is red on the default branch and the sealed VZ suite has never run for want of a registered
  self-hosted runner.

---

## Historical tags

These predate this changelog and are recorded for provenance rather than as releases. **No GitHub
Release has been published yet**; see the blueprint's Phase 1.5.5.

- `selfhost-fixpoint-v1` — 2026-07-12 — the compiler dogfooded in Anubis; self-host reaches a
  byte-identical fixpoint.
- `pca-v0.1` — 2026-07-10 — proof-carrying artifact: Ed25519 software signing plus attributable
  verify.
- `pre-a-plus-capture-20260705-1649` — 2026-07-05 — baseline captured before the A-plus maturity arc.

[Unreleased]: https://github.com/AnubisQuantumCipher/anubis-lang/compare/selfhost-fixpoint-v1...HEAD
