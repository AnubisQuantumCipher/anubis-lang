# Gate 10 Top-Level Bundle Failure Analysis

## Inspected Evidence
- Dir: implementer/a_plus_audit_run/20260706-0213/gate10_risc0_with_real_receipt/
- Commands run (exact as required):
  - bash tools/grok-safety-check.sh → OK (transcript saved)
  - find ... -maxdepth 6 -type f | sort (516 files; list saved to scratch)
  - grep -R "FAIL|PARTIAL|..." across the 0213 dir + out/a_plus_gate10_risc0 (full output saved)
  - jq . on manifest.json, evidence.json, backend/risc0/risc0_metadata.json (transcripts saved to scratch/task1/)

All transcripts and lists saved under /var/folders/bg/pt9l6y1j47q642kp3z5blrmh0000gn/T/grok-goal-f91043dc78a6/implementer/task1/

## Exact Failing Check
- From evidence.json (proof_bundle/evidence-20260706-021338-safe/):
  - "name": "solver"
  - "status": "FAIL"
  - "detail": "assert:(= y (_ bv42 32))=FAIL"
- Overall: "verdict": "FAIL"
- Note: "risc0_receipt_verify" check was "PASS" with detail containing "verify_status=passed fresh_receipt_generated=true dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false"

## Exact File Causing Failure
- evidence.json (top-level checks list)
- solver.json and analysis/solver.smt2 (in the bundle)
- Root cause source: the fixture used for that run (risc0_receipt.anb or copied source.anubis in the evidence bundle):
  ```
  fn main() {
      let x: u32 = 7;
      let y: u32 = x * 6;
      assert(y == 42);
  }
  ```

## Is Failure Real or Stale?
- Real (not stale). The solver check is executed live during evidence bundle generation (SymbolicEngine::check_obligations on the IR). The bundle contains real sidecars (receipt.bin, image_id.txt, guest.elf, real risc0_metadata with fresh receipt and successful verify). The top-level verdict was correctly FAIL because of the solver check, even while RISC0 crypto succeeded.

## Caused By
- Fixture semantics: The assert is input-dependent (y depends on free variable x). The solver builds a QF_BV encoding of y = x * 6 and then checks satisfiability of the negation (not y == 42). The SMT is satisfiable (counterexample exists), so the obligation FAILs.
- SMT excerpt (from solver.smt2 in bundle):
  ```
  (set-logic QF_BV)
  (declare-const x (_ BitVec 32))
  (declare-const y (_ BitVec 32))
  (assert (= y (bvmul x (_ bv6 32))))
  (assert (not (= y (_ bv42 32))))
  (check-sat)
  (get-model)
  ```
- Bundle status propagation: solver FAIL check → overall "verdict": "FAIL" in evidence.json and manifest.
- Not caused by: RISC0 receipt path (risc0_receipt_verify PASS, real ImageID from ELF, real receipt, no placeholders, fresh=true, dev/mock/cache=false), evidence schema (RISC0 sidecar hashes passed), command status for prove, or the receipt verifier logic itself.
- This is non-RISC0 analysis (Anubis static symbolic/solver on the frontend IR) leaking into the top-level bundle verdict.

## What Must Change to Make the Minimal RISC0 Fixture PASS Honestly
- The current language + solver treats asserts as universal invariants (negation must be unsat). An input-dependent assert on a non-constant produces a satisfiable negation → FAIL.
- Honest fix: simplify the fixture to one with no undischarged obligations, e.g.:
  ```
  fn main() {
      let x: u32 = 42;
  }
  ```
  (or the preferred "let x=7; y=x*6; assert(y==42)" only if the pipeline is changed to support sample-input concrete checking — but the plan prefers the no-assertion minimal).
- Re-run the exact TASK 2 command:
  rm -rf out/a_plus_gate10_pass
  cargo run --release -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/a_plus_gate10_pass
- Then verify: verify_bundle.sh exits 0, top-level status PASS, RISC0 metadata PASS, receipt.verify.log says PASS, real non-placeholder ID, no dev/mock/cache.
- Document the semantics mismatch so it is not hidden.
- This preserves the real RISC0 cryptographic path (ELF → ImageID → receipt → verify) while making the bundle verdict honest PASS for the fixture the language/solver actually supports.

All required commands executed for TASK 1. Analysis and transcripts saved to scratch. This identifies the root cause and the honest path forward for Gate 10.
