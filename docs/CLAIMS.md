# Anubis Claims (1.0 freeze — 2026-07-22)

See `MATURITY_CLAIM_MATRIX.md` for historical gate rows. Living freeze:
[`docs/language/SPEC_1_0_FREEZE.md`](language/SPEC_1_0_FREEZE.md) ·
[`docs/language/SEMVER_1_0_POLICY.md`](language/SEMVER_1_0_POLICY.md).

## Portable 1.0 status (grounded)

| Claim | Status | Evidence |
|-------|--------|----------|
| Evidence-native compiler/toolchain | **REAL** | `cargo build -p anubis`; sealed branch CI front door |
| Safe taint enforcement | **REAL** | `examples/security/*`; security fixtures |
| Declassification policy | **REAL** | declassify accept/reject pairs |
| Solver correctness (supported core) | **REAL** | native authoritative + formal gate |
| Evidence bundle + tamper detection | **REAL** | `scripts/run_package_gate.sh` **PASS 9/9** |
| RISC0 receipt path (in-process) | **REAL** | prove/verify path; shape + verify API |
| Metal parity (local Apple Silicon) | **REAL** | local Tier-2; not hosted GPU prove |
| Language core (fixtures + repro) | **REAL** | `run_language_fixtures.sh` **PASS 244/244** |
| Backend portability / doctor / CLI | **REAL** | `anubis doctor`; DX gate **15/15** |
| Ordinary `anubis run` Safe subset | **REAL** | frozen surface in SPEC_1_0; e.g. `hello_normal.anb` runs |
| Runtime planning (probe) | **REAL PLAN-ONLY** | plan surfaces; not plan-observed exec enforcement |
| Runtime plan-observed enforcement | **DEFERRED** | not 1.0-blocking; named residual |
| In-repo package / PCA ecosystem | **REAL** | package gate 9/9; `import` + evidence deps |
| Public package registry (crates.io-like) | **NOT CLAIMED** | no public index; in-repo only |
| Third-party / multi-party reproduction | **REAL** | 2 independent strangers, hash agreement — [`phase9_independent_witness/`](language/phase9_independent_witness/) |
| DDC toolchain diversity (max) | **REAL** | DDC **34/34**; residual: same-author C sources (not TT-total) |
| Hosted CI front door (15-gate, no Metal prove) | **REAL** | `.github/workflows/ci.yml` on `macos-latest` |
| A+ front door (2026-07-24 A15 re-seal) | **REAL** | `bash scripts/audit_a_plus.sh --out out/a_plus_a15_frontdoor_20260724-154145` → **15/15 PASS**; G14 **34/34** tart guest |
| A+ label (full gates + current A15 hostile audit) | **REAL** | `implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` + STEP_STATUS + gate_report (F1–F4 remediated) |
| Hosted CI Metal **proving** | **NOT CLAIMED** | needs Apple Silicon GPU runners |
| Production-grade (1.0 frozen surface) | **REAL** | SPEC_1_0 + ≥2 showcase domains (NEXUS, Vault, settlement) |
| General-purpose language (all features forever) | **PARTIAL** | 1.0 freeze is scoped; residuals listed in SPEC_1_0 §5 |

## Independent reproduction (Phase 9)

| Party | Commit | Selfhost | Repro | DDC |
|-------|--------|----------|-------|-----|
| Stranger 1 | `4b19c48` / witness set | 9/9 | 6/6 | 34/34 |
| Stranger 2 | `7c5bf06` | 9/9 | 6/6 | 34/34 |

Agreed hashes: binary fixpoint `9030e24b…`, macOS repro `c94fd5b1…`, Linux hermetic `6211f8c9…`, DDC output `3830edc6…`.  
See [`language/phase9_independent_witness/WITNESS.md`](language/phase9_independent_witness/WITNESS.md) and [`WITNESS_2.md`](language/phase9_independent_witness/WITNESS_2.md).

## Forbidden overclaims

- “Trusting-trust closed” / “backdoor-free”
- “Hosted Metal proving”
- “Public package registry”
- Infinite multi-party coverage beyond recorded witnesses
