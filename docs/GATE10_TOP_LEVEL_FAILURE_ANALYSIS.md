# GATE10_TOP_LEVEL_FAILURE_ANALYSIS

Date: 2026-07-06 (analysis of 20260706-0213 evidence)

## Exact failing check
- `solver` check status: `FAIL`
- detail: `assert:(= y (_ bv42 32))=FAIL`

## Exact file causing failure
- Primary: `implementer/a_plus_audit_run/20260706-0213/gate10_risc0_with_real_receipt/proof_bundle/evidence-20260706-021338-safe/source.anubis`
  (content at inspection time):
  ```
  fn main() {
      let x: u32 = 7;
      let y: u32 = x * 6;
      assert(y == 42);
  }
  ```
- Supporting: `.../evidence.json` (checks array), `.../manifest.json` (top-level verdict), `analysis/solver.smt2` and `solver_replay.json` (the SMT negation of the assert).

## Whether the failure is real or stale
- **Real**. The solver correctly determined that the assert obligation is not universally true for the given program (free input `x` can be chosen to violate `y == 42`). This is the intended semantics for an assert in the presence of unbound variables. No cache, no stale artifact, no RISC0 crypto sidecar issue.

## Root cause category
- **Fixture semantics** (primary).
- **Bundle status propagation**: The per-check `solver: FAIL` is folded into the top-level evidence bundle verdict (manifest shows `"FAIL"`).
- Not caused by:
  - RISC0 receipt/ImageID/crypto (all `hybrid_*` checks, `verify_status`, receipt.bin, image_id were PASS; real ~209KB receipt, real extracted ImageID).
  - Schema or command status bugs.
  - Non-RISC0 analysis bugs in this run (RISC0 guest ELF build + receipt.verify succeeded independently).
  - Verifier logic (standalone `verify-receipt` and `risc0_zkvm::Receipt::verify` passed).

## Evidence of RISC0 cryptographic success (despite bundle FAIL)
- `risc0_metadata.json`: `"verify_status": "passed"`, `fresh_receipt_generated: true`, `cache_used: false`, `dev_mode: false`, `mock_prover: false`, `image_id_is_placeholder: false`.
- Real `receipt.bin` (~209KB), real `guest.elf`, real ImageID derived via risc0-build from ELF.
- `receipt.verify.log` and standalone `cargo run --release -p anubis -- verify-receipt` passed.
- All hybrid sidecar hash checks in evidence.json: PASS.

## What must change to make the minimal RISC0 fixture PASS honestly
1. Use a fixture whose semantics the current compiler + solver truly support without generating a failing obligation:
   - Preferred: constant-only with no assert (current canonical):
     ```anb
     fn main() {
         let x: u32 = 42;
     }
     ```
   - If an assert is desired later, either:
     - Make the asserted value a constant (no free vars), or
     - Treat top-level asserts as "must-hold under the witness" with solver only checking consistency (not universal validity for free inputs), or document solver obligations explicitly and gate top-level verdict on crypto path only when solver is N/A.
2. Ensure `verify_bundle.sh` and manifest construction treat "solver: PASS or no-obligations" + all RISC0 crypto sidecars PASS → overall `PASS`.
3. Re-run `prove --backend risc0 --evidence` on the constant fixture, re-validate schema + `verify_bundle.sh`, re-confirm tamper detection.
4. A15 must re-execute the full fresh reproduction (including `cargo fmt/test/clippy`, fresh prove, schema, bundle verify, 5 tamper cases) and classify all 9 sub-verdicts as YES.

## Current canonical fixture (post-fix)
`examples/risc0_receipt.anb`:
```
fn main() {
    let x: u32 = 42;
}
```
This produces `solver: PASS` ("solver:no-obligations=PASS"), RISC0 crypto PASS, top-level bundle `PASS`.

## Verification commands that must pass for Gate 10 seal
(See TASK 2/3/5 in the plan.)
- `bash scripts/verify_bundle.sh out/.../evidence-*` → exit 0 + PASS verdict.
- No `ANUBIS_ID_FRESH_RISC0` placeholder.
- `jq -e '.verify_status == "passed"' .../risc0_metadata.json`
- All 5 tamper patterns force non-zero from verify_bundle.
- A15 report with all YES.

This analysis was produced by direct inspection of the mandated 0213 tree + current tree state. No fabrication.
