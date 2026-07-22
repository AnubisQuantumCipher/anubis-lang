# Phase 9 — Independent stranger reproduction witness

**Role:** independent stranger (clean clone; no developer `out/` reused)  
**Date (UTC):** 2026-07-22  
**Sealed commit:** `4b19c4819b04c14b7e12970220db96d4b1bf8567`  
**Branch:** `a-plus-maturity/20260705-1649`  
**Worktree:** `/tmp/anubis-phase9-stranger-*/anubis-lang` (clone of the sealed commit)

## Procedure followed (stranger recipe)

```bash
# 1. Clean clone of the sealed branch/commit (no pre-existing out/)
git clone --branch a-plus-maturity/20260705-1649 <repo> /tmp/anubis-phase9-stranger/anubis-lang
cd /tmp/anubis-phase9-stranger/anubis-lang
git checkout 4b19c4819b04c14b7e12970220db96d4b1bf8567

# 2. Build the CLI
cargo build --release -p anubis

# 3. Self-host fixpoint seal
bash scripts/run_selfhost_gate.sh out/selfhost_gate
# → SELFHOST_GATE: PASS (9/9)

# 4. External reproducibility (macOS + hermetic Linux; Docker required)
export ANUBIS_REPRO_DOCKER=1
bash scripts/run_selfhost_repro_gate.sh out/selfhost_gate
# → SELFHOST_REPRO_GATE: PASS (6/6)

# 5. Diverse double-compiling (rustc/LLVM vs gcc non-LLVM)
export ANUBIS_DDC_CC=gcc-15   # must not be clang
bash scripts/run_selfhost_ddc_gate.sh out/selfhost_ddc_gate
# → SELFHOST_DDC_GATE: PASS (34/34)

# 6. Language core fixtures + formal gate
bash scripts/run_language_fixtures.sh --out out/stranger_lang
# → Overall: PASS (244/244)
bash scripts/run_formal_gate.sh
# → FORMAL_GATE: PASS
```

## Results (all green)

| Gate | Verdict | Notes |
|------|---------|--------|
| `run_selfhost_gate.sh` | **PASS 9/9** | stage2.rs ≡ stage3.rs; binary fixpoint normalized |
| `run_selfhost_repro_gate.sh` (`ANUBIS_REPRO_DOCKER=1`) | **PASS 6/6** | determinism + 0 host paths + hermetic Linux |
| `run_selfhost_ddc_gate.sh` | **PASS 34/34** | cA/cB byte-identical; both negative controls diverged |
| Language fixtures | **PASS 244/244** | `fixture_report.json` |
| Formal Lean gate | **PASS** | no sorry/admit/axiom/native_decide |

## Sealed hashes re-derived by the stranger

| Artifact | sha256 |
|----------|--------|
| Self-host **binary** fixpoint (normalized LC_UUID + ad-hoc sig strip) | `9030e24b4105e02ddeb1bb68932c8e0e5fc9959dcb3db9f4b0e6f64504f5780c` |
| Fixpoint **source** (`stage2.rs` / DDC agreed output) | `3830edc6aff9b5960b365435c742109065451fa861853bafa20774466d97b63e` |
| macOS **reproducible** binary (normalized, remapped paths) | `c94fd5b110418773d70f595a11c3863e11c5be346e81ea8cce29297db87dfc15` |
| Hermetic **Linux ELF** (Docker `rust:1.83-slim-bookworm`) | `6211f8c9de22e0a85007680dfcb59fb48a883d30b53f4c413554e7b446947e2e` |
| Docker image digest used | `rust@sha256:540c902e99c384163b688bbd8b5b8520e94e7731b27f7bd0eaa56ae1960627ab` |
| DDC payload AST | `9c736749d62c78fd0469927c956f6cea54bb8a681b3811d567f029bf4342e378` |

Manifests checked in beside this file:

- `repro_manifest.json` — third-party pin for re-deriving bytes  
- `ddc_manifest.json` — dual-toolchain agreement  
- `environment.txt` — host/toolchain versions  
- `selfhost_gate_excerpt.txt` — seal excerpt  

## Negative controls confirmed (load-bearing)

1. **Repro machine-independence:** remapped build contains **0** `$HOME` / `/Users/` strings; gate fails if paths leak.  
2. **DDC interpreter:** one-token perturbation of the C interpreter **diverges** the capstone.  
3. **DDC parser:** `-DANUBIS_DDC_NEG_CONTROL` **diverges** the C-derived payload.  

## Claim boundary (honest — Phase 9 complete for this DoD)

**This witness proves:** an independent party, from a clean clone of the sealed commit, on separate hardware path and hermetic Linux, can re-derive the fixpoint source, reproducible binaries, dual-toolchain agreement, language fixtures, and formal gate — all green.

**This witness does not claim:**

- infinite multi-party coverage (further strangers can re-run the recipe; this is the first full recorded independent run)  
- “trusting-trust closed” / backdoor-free (DDC narrows toolchain subversion; see `SELFHOST.md`)  
- bit-identical host binaries across arbitrary OS/LLVM versions (functional + sealed-hash equivalence under pinned toolchains)  

## How another stranger re-runs

```bash
git clone https://github.com/AnubisQuantumCipher/anubis-lang.git
cd anubis-lang && git checkout 4b19c4819b04c14b7e12970220db96d4b1bf8567
# then the procedure block at the top of this file
# compare your repro_manifest.json / ddc_manifest.json shas to this witness
```
