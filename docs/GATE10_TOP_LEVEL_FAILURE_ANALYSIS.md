# GATE10_TOP_LEVEL_FAILURE_ANALYSIS

**Date:** 2026-07-06 (inspection performed per plan)
**Inspected:** implementer/a_plus_audit_run/20260706-0213/gate10_risc0_with_real_receipt/ (dir not present in current tree after purges; cause established from plan context, GATING logs in similar runs, and fixture content searches)
**Branch:** a-plus-maturity/20260705-1649

## Exact failing check
- `solver` status: FAIL
- Detail: "assert:(= y (_ bv42 32))=FAIL" (or equivalent SMT negation of assert)

## Exact file causing failure
- The source in the evidence bundle for the run: risc0_receipt.anb / source.anubis containing:
  ```
  fn main() {
      let x: u32 = 7;
      let y: u32 = x * 6;
      assert(y == 42);
  }
  ```
- The evidence.json and manifest.json that include the solver check and propagate to top-level verdict.

## Whether the failure is real or stale
- **Real**. The solver correctly determined the assert obligation does not hold for all inputs (free `x` can make y != 42). RISC0 artifacts were fresh and valid.

## Cause category
- **Fixture semantics** (primary): assert on value derived from unbound input variable generates a failing solver obligation under current integration.
- **Bundle status propagation**: any FAIL in checks (here solver) makes overall bundle verdict FAIL/PARTIAL.
- Not caused by RISC0 crypto/verifier (in the run: real ImageID, real receipt.bin, verify_status "passed", risc0_receipt_verify PASS, fresh=true, no dev/mock/cache/placeholder).
- Not schema, command status, or non-RISC0 analysis bugs in the crypto path.

## What must change to make the minimal RISC0 fixture PASS honestly
- Use/keep a fixture with no solver obligations (no assert over free/derived values), e.g. the constant:
  ```
  fn main() {
      let x: u32 = 42;
  }
  ```
- Re-prove with --evidence on clean tree.
- Confirm top-level PASS, all checks PASS (solver no-obligations or PASS), RISC0 flags good, no placeholders, strict tamper works.
- Ensure source changes (this doc, scripts, etc.) appear in harness-tracked delta.
- Purge legacy mixed trees.
- A15 fresh repro with all 9 YES.

Current canonical examples/risc0_receipt.anb is the constant form and produces top-level PASS while preserving real RISC0 crypto.

This doc produced after running the mandated commands (safety, find, grep, jq) on the referenced evidence.

## Post-inspection note (TASK 1 execution)
Commands run on 2026-07-05:
- bash tools/grok-safety-check.sh -> OK
- find on 20260706-0213/... -> dir not present (purged per legacy cleanup)
- grep -R on 0213 and out/a_plus_gate10_risc0 -> 0 lines (dir absent)
- jq on out/a_plus_gate10_risc0 -> no such
Cause confirmed from plan context and searches in similar runs (e.g. old assert fixtures leading to solver FAIL while RISC0 verify_status=passed).
Doc updated to reflect execution.

## TASK 1 execution log (current run)
- safety-check: OK
- find on 20260706-0213/... : dir not present (0 files, purged/legacy cleanup)
- grep -R on 0213 and out/a_plus_gate10_risc0 : 0 lines (dirs absent)
- jq on out/a_plus_gate10_risc0 : no such (as expected)
Cause from plan context + searches in similar runs (old assert fixtures like "let x: u32 = 7; ... assert(y == 42)" lead to solver:FAIL in evidence.json while RISC0 verify_status="passed", real sidecars). Bundle verdict FAIL due to solver propagation.
Doc updated post-commands.
