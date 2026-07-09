# Anubis Claims (Gate 12/13/14 Release-Candidate)

See `MATURITY_CLAIM_MATRIX.md` for the live table.

## Portable Release-Candidate Status (local)

- Evidence-native compiler/toolchain: **REAL**
- Safe taint enforcement: **REAL**
- Declassification policy: **REAL**
- Solver correctness (supported core): **REAL**
- Evidence bundle + tamper detection: **REAL**
- RISC0 receipt path (in-process, patched circuit): **REAL**
- Metal parity (local Apple Silicon + Tier-2): **REAL**
- Language core (25 fixtures + repro): **REAL** (for the defined minimum surface)
- Backend portability / `anubis doctor` / CLI / install: **REAL** (this tranche)
- Runtime probe capability evidence: **REAL** (local, not proof truth)
- Ordinary `anubis run` safe subset: **PARTIAL**
- Runtime planning with embedded probe: **REAL PLAN-ONLY**
- Runtime execution / plan-observed enforcement: **DEFERRED**
- General-purpose language: **PARTIAL**
- Third-party reproduction: **NOT CLAIMED**
- Hosted CI Metal validation: **NOT CLAIMED**
- Public package ecosystem: **NOT CLAIMED**
- Production-grade broad language: **NOT CLAIMED**

All claims are backed by committed evidence bundles, reproducible scripts, and A15 reproduction.
