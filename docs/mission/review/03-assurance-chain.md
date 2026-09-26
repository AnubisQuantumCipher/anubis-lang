# 03 — Assurance chain map (solver, VCs, evidence, Lean, gates)

Read-only mapping at detached HEAD `c87ad1cf`, 2026-09-23. Nothing was built or executed: no
cargo, no lake, no gate runs. Every claim below comes from reading source. "Observed" means read
from a file or a grep/wc result. "Not verified" means inferred from code and not exercised.

**Read depth.** Read in full or close to it: `solver/README.md`, `solver/src/lib.rs` (1–470),
`solver/src/fragment.rs` (1–147), the `lrat.rs` header, `sat.rs` 205–290, `docs/PROOF_CORRESPONDENCE.md`,
`docs/SOLVER_PIPELINE_MAP.md`, `docs/TRUST_BOUNDARIES.md`, `docs/CI_TRUST_BOUNDARY.md`,
`docs/language/PROOF_SCALING.md`, `.github/workflows/ci.yml` (setup + gate steps), `run_formal_gate.sh`,
`run_formal_kernel_gate.sh`, `run_proof_correspondence_gate.sh`, `run_native_authoritative_gate.sh`,
`run_native_shadow_gate.sh`, and the `audit_unified.sh` roster/verdict logic.
Read in targeted ranges: `compiler/src/middle/mod.rs` (85–160, 6612, 6940–6960, 15213–15900,
16016–16130, 16330–16470, 16995–17030), `compiler/src/evidence/mod.rs` (180–830, 832–1122,
1520–1540, 1845–1990), `tools/anubis/src/evidence_verify.rs` (1–120, 220–520). Header or grep only:
`blast.rs`, `bv.rs`, `parse.rs`, `fp.rs`, the solver tests, every Lean module header, the other
`run_*_gate.sh` scripts, `publish_pin.sh`, `run_seal_checklist.sh`, and `scripts/lib/*`.
Not read: VC-construction bodies (`analyze_stmts`, `discharge_call_requires`, loop-invariant emission),
`expr_to_smt*` bodies, the `is_*_modelable` bodies, `sat.rs` search, and the `lrat.rs` RUP body.

---

## 1. File inventory

Line counts come from `wc -l`.

### solver/ (native QF_BV solver, std only; `Cargo.toml` has no dependencies)
| Path | Lines | Role |
|---|---:|---|
| solver/src/lib.rs | 887 | Entry points (`native_check_sat*`, `native_prove_with_artifacts`). Budgets. The single decision path `decide_formula_inner` (:340). SAT model replay. Unsat only with a checked certificate. |
| solver/src/parse.rs | 629 | Conservative SMT-LIB2 reader (`parse_smt2` :80). Returns `None` on anything outside its fragment. |
| solver/src/bv.rs | 341 | `Formula`/`Term`/`Pred` AST and the independent `eval` used for SAT replay. |
| solver/src/blast.rs | 752 | Tseitin bit-blaster (`blast_with_map` :181, `full_adder` :358). Pre-blast cost estimate `gate_cost` (:84). |
| solver/src/sat.rs | 1060 | CDCL engine. Emits a RUP certificate on Unsat. Load simplification is limited to dropping duplicate literals and tautologies (:228–249). |
| solver/src/lrat.rs | 461 | Independent RUP checker `check_proof` (:52). No deletions, no RAT. Carries its own adversarial unit tests. |
| solver/src/fragment.rs | 326 | The authoritative-fragment gate `is_proven_authoritative` (:47), implemented as a total match with no wildcard. Also the `PROVEN_OP_TAGS` string list (:56). |
| solver/src/fp.rs | 134 | Float64 comparison lowering to BV (monotonic key). |
| solver/tests/differential.rs | 556 | Differential test, native vs z3. **Returns early if z3 is not on PATH** (:6, :269–270). |
| solver/tests/fp_differential.rs, str_differential.rs | 152, 133 | FP and string differential tests. |
| solver/README.md | 148 | Design and rollout record. |

### compiler/src/evidence/, compiler/src/middle/ (solver parts), tools
| Path | Lines | Role |
|---|---:|---|
| compiler/src/evidence/mod.rs | 2359 | Bundle builder, `validate_bundle` (:773), PCA claim (`derive_claim_block` :887, `verify_pca` :1040), Ed25519 `pca.sig`, embedded `validate.sh` (:201). |
| compiler/src/middle/mod.rs | 29994 | Typecheck (:3351), VC construction, `TaintPass::apply` (:15214), `check_obligations` (:15273), `Z3_ARGS` (:15459), `run_z3_obligation_with_smt` (:15543), `native_authoritative` (:15874), `replay_counterexample` (:16064), `classify_assertion_fail` (:16397), `expr_to_smt` (:17096), `is_int_modelable` (:17437), `expr_to_smt_with_width` (:18657), `substitute_vars` (:19265). |
| compiler/src/middle/security_label.rs | 786 | `SecurityLabel`. The only production code with a byte-for-byte Lean link. |
| tools/anubis/src/evidence_verify.rs | (not counted) | `anubis evidence-verify`, including the in-tree RUP replay `check_rup` (:183) and `verify_published_proofs` (:226). |
| scripts/verify_proof_bundle.py | 181 | RUP re-checker in Python that does not use the Anubis binary. |

### formal/ (Lean 4 v4.32.0, core only, no Mathlib)
18 files: 17 modules plus the root `Anubis.lean` (22 lines). `lakefile.toml` (14 lines) declares one lib and one exe, `security_label_observer`. Section 3 has per-module counts.

### Docs in scope
PROOF_CORRESPONDENCE.md 186 · SOLVER_PIPELINE_MAP.md 105 · CI_TRUST_BOUNDARY.md 56 · language/PROOF_SCALING.md 55 · TRUST_BOUNDARIES.md 21.

### CI and gates
- `.github/workflows/ci.yml` (279 lines) is the only workflow.
- `scripts/audit_unified.sh` (696 lines) holds the gate roster.
- There are 46 `scripts/run_*_gate.sh` files, 25–1218 lines each. Main ones: native_authoritative 206, native_shadow 58, formal 25, formal_kernel 69, proof_correspondence 133, security_label_correspondence 454, check_run_parity 492, proof_binding 82, pca 184.
- `publish_pin.sh` 1131, `run_seal_checklist.sh` 1246.
- `scripts/lib/`, largest files: host_resource_guard.sh 820, walker_completeness.py 1111, pin_manifest.py 631, gate_common.sh 498, docs_drift_scan.py 464, phase3_label_census.py 422, vm_battery_validate.py 364, seal_verdict_validate.py 356, offensive_evidence_validate.py 332, gate_run_ledger_promote.py 237, docs_drift_derive.py 221, bundle_manifest.py 155, vz_apply_validate.py 138, gate_evidence.sh 129, seal_log_score.py 103, native_corpus_inventory.py 86, pca_gate_harness.sh 64, read_exact_sha256.py 52.

---

## 2. The assurance chain

| Arrow | Implemented by | Evidence today | Trusted without evidence |
|---|---|---|---|
| **Source → AST** | `frontend/mod.rs:4504 parse_source` | Fixture gates G5, G6, G8, G11–G13 (corpus behaviour only). | The parser. There is no grammar spec or mechanized semantics. |
| **AST → resolved/typed IR** | `middle/mod.rs:3351 typecheck` → `TypedIR` (`constraints`, `solver_obligations`) | Fixture gates. The type/effect/taint/capset self-host differentials check the Rust engine against the Anubis-authored engines (seal/full profile). G23 carrier totality: a new `Expr` variant breaks the build. G19 walker completeness. | Typing rules and name resolution. PROOF_CORRESPONDENCE link 1, TCB item 1. |
| **Typed IR → dataflow (taint/secret/effects/caps)** | `TaintPass::apply` (:15214), `security_label.rs`, `effects.rs`, `capability.rs` | G31: the six `SecurityLabel` ops match `formal/Anubis/SecurityLabel.lean` byte-for-byte over an 83-row finite abstraction. G30 label-site census. Lean `NonInterference`, `DeclassifyWellFormed`, `EffectSoundness` and `Capability` prove properties of **models only**. | Walkers that call the label ops: which operands, at which sites. `docs/CLAIMS.md` item 21 lists open false-accept carriers. The pca_tests fixture at `evidence/mod.rs` ~1990 names a live, accepted taint leak. |
| **Dataflow → VCs** | `analyze_stmts` (:9585), `discharge_call_requires` (:7634), `push_branch_path_condition` (:9108), wrap-safety (`push_wrap_safety_binop` :6878, name at :6950), loop invariants (names at :21782, :21839), `substitute_vars` (:19265) | Lean `ContractComposition`, `PathCondition`, `LoopInvariant` justify the **rules** abstractly. The vacuity check (`assumptions_satisfiable`) fails a contract whose premises are contradictory. An unencodable precondition becomes `requires-unresolved@`, which is FAIL. | VC construction itself: link 1, "not started". Modelability decisions (TCB item 3). An env knob, `ANUBIS_WRAP_SAFETY=0` (:6612), removes every wrap VC and is **not recorded in evidence**. |
| **VCs → SMT text** | `check_obligations` (:15273): the logic is picked by sniffing the body (`"` → QF_S, `fp.` → QF_FP, arrays → QF_ABV, else QF_BV). Every int var is declared BitVec 64. Encoders are `expr_to_smt` (:17096) and `expr_to_smt_with_width` (:18657). | Lean `Encoding`, `IntSigned`, `CompareUnary`, `UnsignedMask`, `StringEncoding`, `ArrayEncoding` prove the **intended** operator denotations. No gate compares emitted text to them. | The emitter (link 2, TCB item 2). Theory routing works by substring sniff. |
| **SMT → solver result** | Native: `run_z3_obligation_with_smt` (:15568) → `anubis_solver::native_check_sat_model_authoritative` (lib.rs:245) → parse → fragment gate → blast → CDCL. Fallback: z3 with `Z3_ARGS` = `-in -smt2 rlimit=200000000 -T:120 -memory:2048`. | Native verdicts are cross-checked against z3 when z3 is on PATH. A disagreement is FAIL (`ANUBIS_NATIVE_DISAGREEMENT`). z3 SAT models get `replay_counterexample` (:16064). Native SAT models get `bv::Formula::eval` replay. Solver differential tests. G18. Lean `BitBlast` (44 theorems) proves the **gate model**. | z3 `unsat` outside the native fragment passes on z3's word (`PROVED_DETAIL_SOLVER_ONLY`, REG-002). This is opt-out only via `ANUBIS_REQUIRE_NATIVE_PROOFS=1`. The SMT parser (TCB item 5). The Rust blaster's refinement of the Lean relation (link 4). z3 replay re-runs **z3**, so it is not independent of z3. |
| **Solver result → certificate check** | `decide_formula_inner` (lib.rs:340–463): Unsat only if `lrat::check_proof` (lrat.rs:52) accepts. Over the cert-work bound the result is `None`. | `lrat.rs` adversarial tests. Every native Unsat is checked in-process. The bundle publishes DIMACS + DRAT per proven obligation. `evidence_verify::check_rup` re-derives them (in-tree Rust). `validate.sh` uses drat-trim/cake_lpr. `scripts/verify_proof_bundle.py` is an independent Python RUP checker. | That `cert.original` is the faithful CNF of the formula: it is produced in the same process by the blaster. **No gate runs `verify_proof_bundle.py` or drat-trim** (grep: it is referenced only from docs and itself). |
| **Checked verdict → compiled/runtime behaviour** | `backends/run.rs` (9128 lines) interpreter and native lowering. `verify_before_native_execution` (tools main.rs:6921). | `run_check_run_parity_gate.sh` checks that the check verdict equals the run-preflight verdict, **without executing**, and only in the operator seal. G17 stdlib fail-closed runtime fixtures. Lean `Encoding` et al. assert runtime `wrapping_*` = SMT ops in the model. | Runtime semantics (TCB item 6). Nothing ties executed behaviour to the checked model. PROOF_SCALING.md: the proof path lowers the whole program into RISC0, with no slicing. |
| **Artifacts → evidence verification** | `build_evidence_bundle_tree_inner`, `validate_bundle` (:773), `verify_pca` (:1040), `evidence_verify.rs`, `validate.sh` | Every file hashed into `MANIFEST.sha256`. PCA is re-derived by re-running the checker from `source.anubis`. `pca.sig` (Ed25519) is optional. Published refutations are replayed. | The CNF ↔ .smt2 ↔ source correspondence. `validate.sh` itself says it is "still the compiler's word". An unsigned manifest can be recomputed by whoever edits the bundle. PCA re-derivation re-trusts the same compiler and the verifier's own z3 and env. See section 5. |

**Verdict-aggregation note (observed, not exercised).** The obligation `status` is a `String`
(`"PASS"`, `"FAIL"`, `"UNKNOWN"`). `check_obligations` upgrades `UNKNOWN` to FAIL only for names
starting `ensures:`, `requires@`, `loop-invariant-base:`, `loop-invariant-step:` or `assert:`
(:15253). A `wrap-safety:` obligation that z3 returns `unknown` on (for example, rlimit exhausted)
stays `UNKNOWN`. Three consumers then disagree:

- `anubis check` (main.rs:2072, 3005) filters `status == "FAIL"`, so it passes.
- `derive_claim_block` uses `!any(FAIL)`, so the PCA says discharged.
- The bundle's own `solver` Check requires `all == "PASS"`, so the bundle verdict is FAIL.

The comment at evidence/mod.rs ~907 claims the PCA "rejected UNKNOWN", which contradicts the code
beside it. `run_z3_obligation_with_smt` calls this branch "effectively unreachable". The rlimit and
memory bounds make it reachable in principle. That was not verified with a fixture.

---

## 3. Lean development

Counts come from the proof-correspondence gate's own method: `^\s*theorem ` counted after stripping
`/- -/` comments. Total **199**, which matches PROOF_CORRESPONDENCE.md's "ships 199". Separately,
`private theorem` occurs 3 times, in BitBlast. The CI step label still says "162 theorems"
(ci.yml:171), which is stale.

`sorry` appears only in comments (StringEncoding:34, SecurityLabel:43, ModeAggregation:9). None
appears in code, and `run_formal_gate.sh` strips comments before scanning.

| Module | Theorems | What it models | Link to production | How enforced |
|---|---:|---|---|---|
| SecurityLabel | 37 | Finite abstraction of the six `SecurityLabel` ops, plus `observationRows_length`/`_nodup` | **Production-linked.** `compiler::middle::security_label` | G31 `run_security_label_correspondence_gate.sh`: Rust observer (cargo test) vs `lake exe security_label_observer`, `cmp` byte-for-byte, 83 rows, per-op counts (4, 2, 49, 14, 7, 7), and a 9-mutation `--self-test`. |
| SecurityLabelObserver | 0 | Emitter exe for the above | Same | Built by `lake build`. |
| BitBlast | 44 | Adder, comparators, eq, bitwise, sub/neg, ite, const/var mul, const/barrel shifts, extract/concat/zext over `List Bool` | **Name-linked only** to `solver/src/blast.rs` and `fragment.rs` | G18 drift check greps a hard-coded list of 22 theorem names in `BitBlast.lean` (plain `grep -q`, so a name inside a comment also matches) and checks that deferred tags are absent from `PROVEN_OP_TAGS`. No proof that the Rust blaster refines the Lean relation (link 4). |
| Encoding | 12 | QF_BV64 arithmetic = `i64::wrapping_*`; u32 mask | Model of `expr_to_smt*` and the runtime | Compile-only (G21). Referenced from `solver/src/bv.rs`/`blast.rs` comments. |
| IntSigned | 13 | Signed div/rem/shift encodings | Model | Compile-only. |
| CompareUnary | 9 | `>`, `>=`, `==`, `!=`, neg, not | Model | Compile-only. |
| UnsignedMask | 13 | u8/u16/u32 masks; truncating cast is not identity | Model | Compile-only. |
| StringEncoding | 12 | QF_S `=`, `str.len`, `++` over `List Char` | Model | Compile-only. |
| ArrayEncoding | 4 | select/store total model vs bounded Vec | Model | Compile-only. |
| ContractComposition | 3 | `substitute_vars` substitution lemma | Model of a checker step | Compile-only. |
| PathCondition | 4 | Adding a guard premise is sound | Model of `push_branch_path_condition` | Compile-only. |
| LoopInvariant | 3 | Hoare while rule (partial correctness) | Model | Compile-only. |
| NonInterference | 7 | Join-propagation NI over a small calculus | Disconnected model | Compile-only. |
| DeclassifyWellFormed | 9 | NI survives malformed declassify | Disconnected model (`wf` "mirrors" `declassify_wellformed`) | Compile-only. |
| EffectSoundness | 16 | Inferred ⊇ performed effects | Disconnected model | Compile-only. |
| Capability | 6 | Linear use-once caps | Disconnected model | Compile-only. |
| ModeAggregation | 7 | Safe < Research < Exploit lattice | Disconnected. Its header says the Rust traversal is covered by CLI regression tests. | Compile-only. |

"Compile-only" means `run_formal_gate.sh` runs `lake build` and a token scan for
`sorry|admit|native_decide|axiom`. That establishes that the theorems are proved. It does not
establish that their statements describe the Rust code. Of the 17 modules, exactly one has a
mechanical Rust↔Lean behavioural link (SecurityLabel, and only over its declared abstraction). A
second, BitBlast, has a name-existence link.

---

## 4. Solver authority

- **Default is native-authoritative.** `native_authoritative()` (:15874) returns true unless
  `ANUBIS_NATIVE_AUTHORITATIVE` is `0`, `false`, `off`, `no` **or empty**.
- **Authoritative fragment** (`fragment.rs` `term_ok`/`pred_ok`):
  - Terms: Add, Sub, Neg, Mul (const and var×var), Shl/Lshr (const and variable), And/Or/Xor/Not,
    Concat, Extract, ZeroExtend, Ite.
  - Predicates: all 8 comparators, Eq, Bool vars/consts, not/and/or.
  - **Deferred to z3:** Ashr, SignExtend, Udiv/Urem/Sdiv/Srem.
  - QF_FP comparison and string equality lower to BV. Native decides them on the un-gated path,
    but the fragment gate applies only to the BV AST, and z3 still handles FP arithmetic, string
    ops and arrays.
  - README (measured table): var×var mul is admitted but usually declines on cost.
- **`PROVEN_OP_TAGS` is not the code that decides admission.** It is a separate `&[&str]` that
  nothing in Rust consumes. The only consumer is the G18 shell drift check. Admission is decided by
  the `match` in `term_ok`/`pred_ok`, which is total, so a new `Term` variant is a compile error.
  The gate therefore inspects a mirror, not the decision.
- **How z3 is used.**
  - Cross-check on every native verdict when z3 is present (primary obligation at :15568–15630; the
    vacuity query likewise).
  - Sole authority when native declines (REG-002). There is opt-in audit logging
    (`ANUBIS_Z3_ONLY_LOG`) and an opt-in refusal (`ANUBIS_REQUIRE_NATIVE_PROOFS=1`).
  - Replay oracle for z3 SAT models.
  - Vacuity checks.
  - A z3 `(error` or stderr containing `error` gives FAIL. `out of memory` gives FAIL (undecided).
    Other output gives `UNKNOWN`.
  - A native Unsat is PASS if z3 says `unknown` or z3 is absent. Only z3 `sat` vetoes it.
- **UNSAT certificates.**
  - CDCL emits a RUP chain. `check_proof` checks well-formedness, the RUP property of each step,
    and an empty terminal clause.
  - Unsat is never returned without acceptance (lib.rs:406–428).
  - Artifacts (DIMACS + DRAT) come only from `native_prove_with_artifacts`, which the evidence
    builder calls as a **second, separate decision** after the checking run
    (evidence/mod.rs:475). They are not the certificate behind the recorded verdict.
- **SAT models.**
  - Native: read back via the blast map, then replayed by `bv::Formula::eval`. On mismatch the
    result is `None`.
  - z3: `parse_z3_model` (BitVec 64 and FP only), then pinned and re-run in z3. On mismatch the
    status is FAIL with `ANUBIS_REPLAY_MISMATCH`.
- **Budgets** (lib.rs:64–131), deterministic by default:

  | Budget | Default | Override |
  |---|---|---|
  | Conflicts | 20,000 | `ANUBIS_NATIVE_CONFLICT_BUDGET` |
  | Gate ceiling | 400,000 | `_GATE_CEILING` |
  | Clause ceiling | 2,000,000 | `_CLAUSE_CEILING` |
  | Cert-work ceiling | 50,000,000 | `_CERT_WORK` |
  | Wall clock | off (0) | `_TIME_BUDGET_MS` |

  - Each bound can only turn a verdict into a decline.
  - z3 is bounded by rlimit (the deterministic one), `-T:120` as a hang guard, and `-memory:2048`.
  - The comments claim load-independence and say it is "not yet demonstrated".
  - **None of these env overrides is recorded in the evidence bundle.** `environment.json` records
    only os, arch, rustc, cargo, z3 and the anubis version (evidence/mod.rs:1520).

---

## 5. Evidence bundle format

**Files.**
- `source.anubis` (or `source-merkle-leaves.json`), `build.log`, `hir.json`, `mir.json`,
  `taint-traces.json`, `solver.json`, `mono_specializations.json`, `environment.json`,
  `checks.sarif`, `bounty-report.md`, `validate.sh`, `source-tree.json`, `evidence.json` and its
  duplicate `manifest.json`, `pca.json`.
- Optional: `artifact`, `dep_closure.json`, risc0 sidecars, confinement/entitlement manifests,
  program-evidence v3 (best-effort).
- `analysis/proofs.json` and `analysis/proofs/obligation_NNNN.{smt2,cnf,drat}`.
- `analysis/solver.smt2` and `analysis/solver_replay.json` (first obligation only).
- `MANIFEST.sha256`, and `pca.sig` outside the manifest.

**Hashing.**
- `MANIFEST.sha256` recursively hashes every file except itself (:1895).
- `evidence.json` also carries per-file hashes and `manifest_sha256 = sha256(source:buildlog:sourcetree:verdict)`.
  `validate_bundle` never recomputes that composite.
- `pca.sig` signs `sha256(pca.json) || sha256(MANIFEST.sha256)`.

**What `anubis evidence-verify` checks** (evidence_verify.rs):
- `verify_pca`, which runs `validate_bundle`: manifest entries re-hash, per-field hashes, all Checks
  PASS, verdict PASS, then a **full re-run of parse, typecheck and solver** from `source.anubis`
  compared to `pca.json`, plus confinement/entitlement re-derivation.
- The optional signature, required when `--pubkey` is given.
- `verify_published_proofs`: fails if `proofs.json` is absent or empty, re-derives every
  `rup_refutation` row in-tree, and lists the other rows as "uncertified" while still reporting PASS.

**Known gaps (observed in code).**
1. **Replay label.** `solver_replay.json` covers only the first obligation. When that obligation is
   not FAIL-with-model (for example a PASS), `replay` defaults to `true`. The file then says
   `"status": "counterexample_replayed", "replay_valid": true` for an obligation that had no
   counterexample (evidence/mod.rs:532–551).
2. **Obligation identity.**
   - A `SolverObligation.name` is a kind prefix plus the raw SMT text (`assert:{smt}`,
     `ensures:{smt}`, `wrap-safety:(op x y)`). There is no source span or stable id.
   - Proof files are keyed by position (`obligation_{i:04}`).
   - Nothing checks that `proofs.json` rows match `solver.json` entries (count, name, smt text, or
     the status vs proof-kind pairing, for example a FAIL row with a `rup_refutation`).
3. **Typed vs prose outcomes.**
   - Status is a string.
   - The obligation kind comes from name prefixes (`starts_with`).
   - `classify_assertion_fail` and `refusal_locus` classify by `detail.contains(...)` ("work
     budget", "undecided", "ANUBIS_REPLAY_MISMATCH", ...).
   - `certificate_status` keys off exact detail strings.
   - Only `UNRESOLVED_PRECONDITION_DETAIL` is matched by exact equality.
   - `evidence_verify` has a typed `CheckStatus`. The compiler side does not.
4. **Proof artifacts are re-decided, not captured.** The CNF/DRAT come from a second native run
   under the verifier-side env limits (evidence/mod.rs:475), so they may not be the certificate
   that produced the recorded verdict. Writes use `let _ = std::fs::write(...)`, so a failed write
   is silently ignored. The manifest then hashes whatever exists.
5. **`solver_backend` is hard-coded `"z3"`** in every PCA claim (evidence/mod.rs:881). The comment
   says z3 is authoritative "unless `ANUBIS_NATIVE_AUTHORITATIVE=1`", but the product default has
   been native-authoritative since 2026-07-25.
6. **Verifier disagreement.** With no `analysis/proofs` directory, `validate.sh` exits 0
   ("integrity only"), while `evidence-verify` FAILs on a missing `proofs.json`. Separately,
   `validate.sh` is shipped inside the bundle it validates.
7. **PCA re-derivation is consistency, not independence.** It re-runs the same compiler and
   solver, and its result depends on the verifier's z3 version and `ANUBIS_*` env.
   `validate_manifest_hashes` checks only the listed entries; an extra unlisted file is not
   detected. Unsigned bundles verify.
8. **An empty expected hash disables its check.** `validate_bundle` skips any per-field check whose
   expected hash field is empty (`is_empty() ||`).
9. The bundle adds a `symbolic` Check that is FAIL when `tainted.constraints` is empty
   (evidence/mod.rs ~590). Its effect on contract-free programs was not exercised.

---

## 6. CI and gates

**Where things run.**
- **GitHub CI:** one job, `hosted-gate-witness`, on `macos-latest`, with pinned nightly-2026-05-10,
  z3 4.15.4 (sha-pinned zip), elan 4.2.3 (sha-pinned) and the Lean version from
  `formal/lean-toolchain`. It runs `run_formal_gate.sh` and then
  `audit_unified.sh --profile hosted`.
- **There is no Linux CI job.** This Linux aarch64 box has no hosted lane.
- **Operator-run on macOS:** the sealed Tart/VZ battery (`run_seal_checklist.sh`,
  `vm/run-slice.sh --release`) and require-Metal parity. These are not in CI
  (CI_TRUST_BOUNDARY.md).
- **Doc drift:** CI_TRUST_BOUNDARY.md and the ci.yml header say "29-gate roster". `EXPECTED_GATES`
  in `audit_unified.sh:595` lists 31. TRUST_BOUNDARIES.md names `/Users/sicarii/...` paths
  (macOS operator).

**Hosted roster (`audit_unified.sh`).** A hosted run passes only as `HOSTED_PASS` with
`G9_poc_kit` exactly EXTERNAL, zero FAIL and zero SKIP. A SKIP makes the verdict FAIL, and any
missing, duplicate or extra gate also fails.

| Gate | Checks | Where |
|---|---|---|
| G1 fmt, G2 clippy, G3 test, G4 build_release | Rust hygiene and the full `cargo test` (includes the solver differential, which **passes vacuously without z3**; CI installs z3) | CI macOS |
| G5 language, G6 turing_core, G8 security fixtures | Expected check/run outcomes over fixture corpora | CI |
| G7 pca | PCA emit + `verify` honesty | CI |
| G9 poc_kit | Crash PoC and fuzz, in VZ | EXTERNAL in hosted, operator VZ |
| G10 prove | RISC0 receipt binding + cold verify (committed fixture exists) | CI |
| G11 enum_match, G12 for_in, G13 lang_trio | Language features | CI |
| G14 offensive | Hosted: 5/5 host-isolation witness. Full: 34/34 in Tart guest | CI (witness) / VZ |
| G15 dogfood_feel | `examples/feel/*` run | CI |
| G16 docs_drift | Re-derives doc stamps by command | CI |
| G17 stdlib_failclosed | Runtime fail-closed fixtures | CI |
| G18 native_authoritative | Cert unit tests, corpus rc-equivalence, z3-hidden demo, Lean-name drift | CI |
| G19 walker_completeness, G20 gate_common_adoption, G22 fixture_preflight, G23 carrier_totality, G24 promise_coherence, G27–G29 fault suites, G30 label census | Harness/walker structure and doc/promise coherence | CI |
| G21 formal | `lake build` + no sorry/admit/axiom/native_decide in code | CI (also a separate step) |
| G25 formal_kernel | Pure-Anubis SAT *demo program* + Python oracle (12 cases) | CI |
| G26 proof_correspondence | Citations in PROOF_CORRESPONDENCE.md resolve; theorem count matches; TCB non-empty | CI |
| G31 security_label_correspondence | Rust↔Lean byte-for-byte, 83 rows | CI |

**Outside the hosted roster.**
- Seal checklist (operator, pinned binary): security, language, runtime, check_run_parity,
  stdlib/run fail-closed, capset parity, formal, formal_kernel, native_authoritative,
  proof_correspondence, security_label_correspondence, the selfhost family (including DDC and
  repro when opted in), instrument_hygiene, gate_run_freshness.
- Never run by any roster: `run_native_shadow_gate.sh`, `verify_proof_bundle.py`.
- Hardware/VM-specific: proof_binding, metal_prove, keychain_se, nexus, power, package, dx,
  lifeline, vz_apply, check_confine_run, author_diversity, poc_kit, offensive_platform.
- Floors: `.gate_floors/` and `scripts/floors/`. `assert_floor` refuses when a floor file is
  missing and fails when coverage falls.

**Gates that can pass without asking their question** (all from code reading, none executed):

1. **G18 verdict equivalence is self-comparison.**
   - It compares `anubis check` (default) against `ANUBIS_NATIVE_AUTHORITATIVE=1`. Since the
     2026-07-25 flip both legs run the same configuration, so `mismatches=0` holds by construction.
   - Native-vs-z3 is asked only through `ANUBIS_NATIVE_DISAGREE` stderr lines. Those fire only on
     obligations native decides while z3 is present. z3-only obligations (REG-002) are never
     compared.
   - The audit labels a PASS "native solver agrees with reference across the corpus".
2. **G18 drift check inspects a mirror.** `PROVEN_OP_TAGS` is not read by `term_ok`/`pred_ok`.
   The theorem check is `grep -q` for 22 names anywhere in `BitBlast.lean`, comments included. It
   is not per-tag, and a new tag outside the six-name deferred list is not caught.
3. **`run_native_shadow_gate.sh`:**
   - It floors files, not obligations, so zero comparisons passes.
   - `native_shadow_compare` runs only on the z3 fall-through. Under the default mode a native
     Unsat returns before it, so the certified lane is mostly unobserved.
   - `scripts/floors/native_shadow_gate.count_floor` does not exist, so the gate would refuse as
     written.
   - It is in no roster, yet PROOF_CORRESPONDENCE cites it as link 3's evidence.
4. **G26 proof_correspondence checks names, not the claims.**
   - It validates only backtick names ending in `_correct`, so `rippleCarry_spec`, `shlConst`,
     `barrelShl` and `observationRows_*` are not checked.
   - It checks existence, not the stated status of each link.
   - Its `--self-test` exercises `grep` in a temp dir, not the gate script.
5. **G21 formal** proves the theorems. Statement drift and weakening, and any link to Rust (except
   through G31), are out of its scope.
6. **Link 5 "independently re-checkable" is not gated.** No gate or CI step runs
   `verify_proof_bundle.py` or drat-trim on a produced bundle. `evidence-verify` replays with
   in-tree Rust and PASSes with every row uncertified.
7. **`validate.sh` returns 0 (integrity only)** when `analysis/proofs/` is absent.
8. **The link 7 gate (`check_run_parity`) compares verdicts, never behaviour**, and runs only in the
   operator seal.
9. **`solver/tests/differential.rs` returns early without z3.** Any `cargo test` on a z3-less host
   reports the differential as passed.
10. **No verdict-affecting env knob is captured.** `ANUBIS_WRAP_SAFETY`,
    `ANUBIS_NATIVE_AUTHORITATIVE` and the `ANUBIS_NATIVE_*` budgets go unrecorded, so a green
    bundle or gate cannot show which configuration produced it. G25 refuses
    `ANUBIS_WRAP_SAFETY=0`; other gates do not check it.

**Stale or contradictory surfaces worth fixing:**
- ci.yml:171 "162 theorems" vs 199.
- "29-gate" vs 31.
- SOLVER_PIPELINE_MAP §4 says per-width BV declarations ("u8 now BitVec 8"), while
  `check_obligations` declares every int var as BitVec 64. §4/§6 also describe z3 as the primary
  path.
- The evidence/mod.rs:881 comment on who is authoritative.
