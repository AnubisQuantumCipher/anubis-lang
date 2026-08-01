# Anubis 1.0 — SemVer & stability policy

**Status:** FROZEN for the 1.0 surface defined in [`SPEC_1_0_FREEZE.md`](SPEC_1_0_FREEZE.md)  
**Effective date:** 2026-07-22  
**Branch of record:** `main` (operative only after the approved Phase-1.5 migration)

## Versioning

Anubis follows [Semantic Versioning 2.0.0](https://semver.org/):

| Bump | When |
|------|------|
| **MAJOR** (`2.0.0`) | Breaking change to the 1.0 frozen surface (language grammar, `anubis check` accept/reject for Safe mode, package PCA schema, CLI subcommands listed as stable) |
| **MINOR** (`1.1.0`) | Backward-compatible additions (new builtins, new optional flags, new gates) |
| **PATCH** (`1.0.1`) | Bug fixes that preserve 1.0 behavior; fail-closed security fixes that only *reject more* (never silent accept of previously rejected Safe programs) |

## Compatibility guarantees (1.x)

1. A program that **`anubis check` PASSes** under 1.0 Safe mode on the frozen surface continues to PASS on 1.x (unless a documented security fail-closed fix shrinks the accept set — those are PATCH with release notes).
2. **`anubis run`** of the frozen Safe subset continues to execute with the same observable results for pure programs (I/O paths may differ by host).
3. **Evidence bundles** (`anubis build --evidence`) remain verifiable by `anubis verify` across 1.x; new optional manifest fields are additive.
4. **Self-host fixpoint** may re-baseline only with a logged change to `scripts/vm/EXPECTED_FIXPOINT_VM` (never silent drift).

## Explicit non-guarantees

- Research / `@research` / PoC surfaces may change without MAJOR.
- RISC0/Metal proving performance and image digests may change; receipt **verify** API contract is stable.
- Hosted CI Metal *proving* is not part of 1.0 (local Apple Silicon only).
- Incomplete SMT models may still **defer** (fail-open completeness); they must never falsely ACCEPT.

## How to ship a 1.x change

Use a short-lived branch and pull request for each coherent slice. Keep CODE/config and DOCS in
separate commits, attach source-bound evidence, require the exact hosted check on the PR head, and
never force-push or self-merge around a red/missing check. Only the operator may authorize a tag or
Release; `v1.0.0` remains reserved for Phase 7.
