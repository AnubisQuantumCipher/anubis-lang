# Phase 9 — Second independent stranger (multi-party)

**Role:** second independent stranger  
**Date (UTC):** 2026-07-22  
**Worktree:** `/tmp/anubis-phase9-stranger2-10811/anubis-lang`  
**Sealed commit:** `7c5bf06ea2ef498cf63db296d63c75cf183bc8fe`  
**Branch:** `a-plus-maturity/20260705-1649`

## Procedure

Identical to [`WITNESS.md`](WITNESS.md): clean clone → `cargo build -p anubis` →  
`run_selfhost_gate` → `ANUBIS_REPRO_DOCKER=1 run_selfhost_repro_gate` →  
`ANUBIS_DDC_CC=gcc-15 run_selfhost_ddc_gate`.

## Results

| Gate | Verdict |
|------|---------|
| SELFHOST_GATE | **PASS 9/9** |
| SELFHOST_REPRO_GATE | **PASS 6/6** |
| SELFHOST_DDC_GATE | **PASS 34/34** (both negative controls diverged) |

## Hash agreement with first stranger (`WITNESS.md`)

| Artifact | Stranger 1 | Stranger 2 | Match |
|----------|------------|------------|-------|
| Binary fixpoint (normalized) | `9030e24b…5780c` | `9030e24b…5780c` | **YES** |
| macOS repro binary | `c94fd5b1…dfc15` | `c94fd5b1…dfc15` | **YES** |
| Hermetic Linux ELF | `6211f8c9…47e2e` | `6211f8c9…47e2e` | **YES** |
| Docker image digest | `540c902e…` | `540c902e…` | **YES** |
| DDC agreed output | `3830edc6…b63e` | `3830edc6…b63e` | **YES** |

## Multi-party claim

Two independent clean-clone runs (different absolute paths, separate `out/` trees), same
pinned toolchains, re-derived **byte-identical** sealed hashes. Phase 9 multi-party
reproduction is **REAL** for this host class (Apple Silicon macOS + Docker Linux hermetic).
