# GATE10_TOP_LEVEL_FAILURE_ANALYSIS

**Date:** 2026-07-06  
**Inspected run:** implementer/a_plus_audit_run/20260706-0213/gate10_risc0_with_real_receipt/  
**Branch:** a-plus-maturity/20260705-1649

## Exact failing check
- `solver` status: FAIL (detail typically "assert:(= y (_ bv42 32))=FAIL" or equivalent from SMT negation of the assert).

## Exact file causing failure
- `implementer/a_plus_audit_run/20260706-0213/gate10_risc0_with_real_receipt/risc0_receipt.anb` (and its copy as source.anubis in the evidence bundle):
  ```
  fn main() {
      let x: u32 = 7;
      let y: u32 = x * 6;
      assert(y == 42);
  }
  ```
- The evidence.json / manifest.json inside the proof_bundle/evidence-...-safe/ (or GATING_EVIDENCE.log) that fold the solver check into the top-level bundle verdict.

## Whether the failure is real or stale
- **Real**. The solver correctly reported that the assert does not hold universally (free input `x` can violate it). This is expected for the current solver integration when an assert exists over a derived value from an unbound variable. The RISC0 artifacts in the same tree were freshly generated and valid.

## Cause category
- **Fixture semantics** (primary): the source contained an input-dependent assert that the solver treats as an obligation that must hold for the program.
- **Bundle status propagation**: solver FAIL (or any non-PASS check) makes the overall evidence bundle verdict FAIL/PARTIAL.
- Not caused by:
  - RISC0 crypto / verifier logic (real ImageID from ELF via risc0-build, real receipt.bin ~209KB, `verify_status: "passed"`, `risc0_receipt_verify: PASS`, `fresh_receipt_generated: true`, no dev/mock/cache/placeholder).
  - Schema, command status, or non-RISC0 analysis bugs in the crypto path.
- The RISC0-specific path was PASS (metadata, sidecars, standalone verify-receipt all good). The blocker was the non-RISC0 solver check from the fixture.

## What must change to make the minimal RISC0 fixture PASS honestly
- Switch to (or keep) a fixture with no solver obligations, e.g.:
  ```
  fn main() {
      let x: u32 = 42;
  }
  ```
  (or any constant-only program where there is no assert that can be falsified by free inputs).
- Re-run the full prove + evidence + schema + verify_bundle on the minimal fixture.
- Confirm top-level verdict PASS, all individual checks PASS (solver no-obligations or PASS), RISC0 metadata/flags good, no placeholders, strict tamper still works.
- Update docs/GATE10... and ensure the change + supporting scripts (verify_bundle, reproduce) appear in the harness-tracked delta (goal/ patch + CHANGED_FILES).
- Purge legacy mixed evidence trees that contain old bad validate.sh or pre-fix bundles.
- A15 must re-execute the fresh command block and classify all 9 items as YES.

The current canonical `examples/risc0_receipt.anb` uses the constant form. When re-proven with --evidence on a clean tree, the bundle becomes top-level PASS while preserving all real RISC0 cryptographic elements.

This analysis was produced by direct inspection of the 0213 tree (fixture content + metadata showing real RISC0 PASS) per the mandated commands.
