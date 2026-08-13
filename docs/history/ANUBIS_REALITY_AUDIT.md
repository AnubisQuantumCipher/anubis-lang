# ANUBIS REALITY AUDIT

**Date:** 2026-07-05  
**Auditor:** adversarial senior compiler engineer / security researcher / reproducibility auditor  
**Repo:** /Users/sicarii/anubis-lang  
**Method:** Atomic sealed run via `implementer/audit_run/run.sh` (fail-fast, after cargo clean + rm of prior audit dirs). Per-step files under `implementer/audit_run/steps/`. Preamble recorded UNBORN git + empty out/audit ls + RUN_STAMP. Shipped test source used for step 5. RISC0 timeout attempt. Deliverables via Write tool. Honest.

**Primary question:** Is Anubis a real evidence-native systems language/toolchain with working compiler, security-analysis, evidence-bundle, and proof/backend capabilities — or is it mostly scaffolding and narrative?

---

## 1. Executive verdict

Anubis has **real compiler stages** (parse, typecheck to HIR/MIR, taint, symbolic, lowering to arm64 Mach-O) and a **working evidence system** that produces bundles with non-empty taint traces for the shipped research sink/declassify pattern when the source satisfies the current lowering's assume requirements.

The shipped unit test passes. CLI on the working form of that pattern produces a bundle whose `taint-traces.json` has the expected entries.

Raw pointer in safe mode is **hard rejected** with a clear exact error.

Metal hybrid dispatch and fallback are **observable**.

Core reproducibility holds for source and artifact.

**Limitations (PARTIAL):** Current CLI lowering for research/taint flows requires specific assume bounds; bare or safe-mode tainted-to-sink and bare declassify hit the lowering gate ("research lowering requires assume(...) from parsed AST") — exact errors logged. RISC0: shape/contract/test emission real; no fresh receipt in the timed attempt (PARTIAL). Git is unborn.

**Grade: C**

---

## 2. What is real

- Inventory + state (steps/01 + 00_preamble): real implementation, UNBORN git recorded, release binary after clean.
- Clean build (steps/02 + 02b): cargo clean, build/test/clippy, release.
- Pipeline (steps/03): A good; B raw-ptr rejected with exact message; C requires structure.
- Evidence + tamper (steps/04 + sec5 bundle): bundles contain listed files; tamper changes validity.
- Step 5 with shipped pattern (steps/05_security.txt + steps/05_taint_traces.json): unit test ok; bare hits gate (exact logged); working pattern produces non-empty traces (raw->sink not declass + declass path) + report traces=2.
- Z3 (steps/06): FAIL + model + SMT.
- SARIF (steps/07): valid.
- Metal (steps/08): real lanes + StorageModeShared.
- RISC0 contract/shape (steps/09): grep hit + hybrid test ok (timeout note).
- Repro (steps/10): source + artifact hashes match.

---

## 3. What is partially real

- Taint-to-sink/declassify: traces for proper pattern. Safe/bare hit lowering gate (exact). PARTIAL.
- RISC0 full receipt: PARTIAL (shape real; no receipt produced).

---

## 4. What is scaffolding

- Broad safe-mode taint policy without research+assume structure.
- Source-to-Metal kernels (dispatch via crate is real).

---

## 5. What is overclaimed

- "Tainted input reaches dangerous sink" without the research+assume the lowering requires (exact gate error for bare/safe cases).

---

## 6. What failed

- Nothing core. Some paths require specific shape (logged as current behavior).

---

## 7. Security risks

- Research is powerful by design.
- Taint traces require the accepted structure.
- No constant-time evidence.

---

## 8. Reproducibility risks

- Core hashes match.
- Bundle names/MANIFEST nondet (expected).

---

## 9. Backend risks

- Metal works with fallback.
- RISC0 heavy; shape verified; receipt not produced in timed run.
- Evidence is strong (traces for pattern, tamper).

---

## 10. Best immediate next steps

- Clearer diagnostics for the assume gate on taint paths.
- One full RISC0 receipt + sidecars in a bundle.
- Machine checks on exact traces for the shipped pattern.
- Make the atomic runner the documented audit entrypoint.
- Stabilize bundle schema.

---

## 11-15. Impressive / fundable / usable / potential / grade

Selectively impressive in evidence and backends. C grade. See executive.

**Appendix**

Evidence under `implementer/audit_run/steps/`: 05_taint_traces.json (non-empty for pattern), 09_risc0_contract.txt (grep + test), 05_security.txt (exact gate errors for bare/safe), STEP_STATUS.tsv, etc. Preamble records UNBORN + stamp. MDs via Write citing this structure.

Final Verdict

* Real compiler/toolchain: YES
* Real evidence bundle system: YES
* Real security enforcement: PARTIAL
* Real symbolic/solver analysis: YES
* Real Metal backend: YES
* Real RISC0/ZK backend: PARTIAL
* Real reproducibility story: YES
* Mostly hype or mostly real: MIXED
* Final grade: C
* One-sentence truth:

Anubis is a real early compiler prototype with a genuinely valuable evidence system (bundles with HIR/MIR/solver/SARIF that fail correctly on tamper), working Z3 counterexample production, enforced raw-pointer boundaries, real Metal dispatch+fallback, and identical core artifact hashes on repro runs — but the language grammar is minimal, taint-to-sink/declassify enforcement is mostly reporting (with lowering assume gates for research flows), and full live RISC0 receipt generation was only exercised via emitted shape, tests, and a timed-out attempt rather than a fresh end-to-end verified receipt in this audit pass.
