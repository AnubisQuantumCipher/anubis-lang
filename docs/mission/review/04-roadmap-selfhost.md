# Review 04 — roadmap authorities, done-criteria, open defects, self-host inventory

Date: 2026-09-23. Tree: `c87ad1cf` on branch `mission/anubis-1.0` (git status at review: only an
untracked `tests/soundness/` that this review did not create). This was a read-only review: no cargo, no
builds, no gate runs, and no state-changing git. The one command executed from the tree was
`python3 scripts/lib/docs_drift_derive.py .`. It prints to stdout and only counts inventory. It wrote no files.

**How deeply each source was read.** Read in full: `COMPLETION_BLUEPRINT.md`, `ROADMAP_AI_ERA.md`, `AGENTS.md`,
`SELFHOST.md`, `SELFHOST_DOGFOOD_PLAN.md`, `SELFHOST_REPRO_PLAN.md`, `SPEC_1_0_FREEZE.md`,
`LANGUAGE_COMPLETENESS.md`, `REPRODUCIBILITY.md`, `selfhost/README.md`, `selfhost/SUBSET.md`, and the
item-21 receipt.

Read in part:
- `docs/language/ROADMAP.md`: every line, cut at 240–260 chars. The cut hides most of the Phase 2, 3 and 7
  table cells (up to 77k chars each).
- `CLAIMS.md`: lines 1–290, 514–545, 1310–1632 and 2292–2570, plus headings and item headlines. Items 13–20
  were read at headline level only.
- `UNSUPPORTED.md`, `CAPABILITIES.md`, `INSTALL.md`: grep only.
- `anubis_sh.anb` (2,779 lines / 118,683 bytes): dispatch, top-level parse and type parse only.
- `selfhost_schema/mod.rs`: not read beyond its line count.
- `docs/evidence/*`: verdict lines only, except the item-21 receipt.

---

## 1. Authorities and roadmap conflicts

### Which documents are authorities

| Role | Document | Basis |
|---|---|---|
| **Defect registry (status authority)** | `docs/CLAIMS.md`, § "Known open issues" / "Open — load-bearing" | CLAIMS:12 ("single source of truth for current status"); BLUEPRINT:7-8; ROADMAP:3-4; AGENTS:21 |
| **Execution plan (phase order and stop protocol)** | `docs/COMPLETION_BLUEPRINT.md` | BLUEPRINT:5 says of itself that it "is not an authority on status" |
| **Graded feature status / phase narrative** | `docs/language/ROADMAP.md` | BLUEPRINT:8 and :37 name it an authority. ROADMAP itself says to re-run before quoting (:7, :20-23) |
| **Definition of done (proposal)** | `docs/ROADMAP_AI_ERA.md` | AI_ERA:3 says "Status: proposal. Nothing here is a claim about the current tree." Neither BLUEPRINT nor ROADMAP links to it. Only CLAIMS:2337, SPEC_1_0_FREEZE:49 and EDITIONS.md:65 cite it |
| Phase receipts | `docs/evidence/PHASE_*_COMPLETION_*.md`, `COMPLETION_PHASES_5_8_STATUS_2026-08-15.md` | Dated. Required by BLUEPRINT:69-80 |

### Conflicts, with line references

There are three numbering schemes for the same program, and none of them owns the other two:

- **Completion Phases 0–8** (BLUEPRINT:51-60)
- **Legacy phases 0–10** (ROADMAP:347-362 and the verbatim arc at :457-483)
- **Sequence steps 1–14 plus done-criteria 1–16** (AI_ERA:67-112)

ROADMAP:150-157 disambiguates the first two. Nothing maps AI_ERA onto either of them.

Specific conflicts:

1. **Certificates.** ROADMAP:116-117 says CDCL Unsat "now emits a verified RUP/LRAT certificate
   (`solver/src/lrat.rs`); the **certificate residual is CLOSED**". The other sources disagree:
   - AI_ERA:252-254: "the LRAT-with-hints upgrade … has not been done; the emitted certificates are still DRAT".
   - CLAIMS:236-237 (item 6, REG-002): full in-process certificate replay is "still not implemented".
   - `solver/src/lrat.rs:1` describes itself as "RUP … checker (LRAT-shaped emission, no deletions)".

   ROADMAP overclaims here.
2. **Mislabelled criterion.** AI_ERA:252 says "The other half of **criterion 5**, the LRAT-with-hints
   upgrade". The LRAT upgrade is sequence **step** 5 (AI_ERA:101-103). Criterion 5 is load-determinism (:73).
3. **Phase 4 (self-host) status.**
   - ROADMAP:469 (verbatim arc) says "Phase four … **DONE (2026-07-21)**: all three semantic engines".
   - ROADMAP:353 (status table) says "🟡 PARTIAL … registry re-baseline unsealed".
   - CLAIMS:138-146 (item 3) says "VM seal DONE (corrected 2026-07-29)".
   - CAPABILITIES:184 says "post-registry VM fixpoint is currently **unsealed**".

   That is three different states.
4. **Phases 9 and 10.** ROADMAP:118-124 calls Phase 9 "🟢 DONE" and Phase 10 "remains 🟢 DONE". The table at
   :361-362 grades both 🟡. AI_ERA puts the "1.0 freeze" in the future (step 11, :109), while SPEC_1_0_FREEZE:3
   records a freeze dated 2026-07-22. README:166 says "Pre-1.0". The release is `v0.1.2-preview` (AGENTS:141).
5. **Self-host seal gate "cannot execute".** ROADMAP:118-121 and :443-446 say the gate "still calls that exact
   path" (`run --allow-research`). `scripts/run_selfhost_gate.sh:51-56` now omits the flag, so that text is
   stale. `SELFHOST.md:131,138` and `selfhost/README.md` still tell users to pass `--allow-research`.
6. **Where exit criteria live.** BLUEPRINT:62 says "detailed exit criteria are phase-owned". ROADMAP:159,
   :196, :209, :225, :246, :264 cite "blueprint §55/§66" and "mission §78/§96/§115/§141/§197".
   - BLUEPRINT line 66 falls in the "Mandatory phase stop" section, not a semantic domain.
   - The mission file (`brain/ANUBIS_COMPLETION_PHASE_3_…MISSION_2026-08-14.md`, cited at
     `PHASE_3_COMPLETION_2026-08-14.md:5`) is not in the repository.

   So the Phase 3 criteria have no in-repo normative source.
7. **Lean counts.** AI_ERA:131-133 warns that "Four different totals appear". That remains true:
   - ROADMAP:28-29 and :68 say 162/15.
   - ROADMAP:354 says "Lean 162/15".
   - AGENTS:100 and CLAIMS:83,108 say 199/16.
   - CLAIMS:1485,1624 say 162/15 (marked historical).

   The live derivation today gives 199 theorems / 16 modules.
8. **Linux vs macOS.** AI_ERA criterion 13 and step 7 require a sealed Linux fixpoint. Everything that counts
   as a seal is macOS-only:
   - BLUEPRINT's seal and ROADMAP's Phase 3/4 "VZ self-host seal PASS" (ROADMAP:168-171, :323) use a tart
     macOS guest. `scripts/vm/EXPECTED_FIXPOINT_VM` is a Mach-O baseline.
   - CI runs on `macos-latest` only (`.github/workflows/ci.yml:23`).
9. **Blueprint stop protocol vs Phases 5–7.** BLUEPRINT:70 requires a `PHASE_<n>_COMPLETION_<date>.md` for each
   phase. Phases 5–7 share one status note, `COMPLETION_PHASES_5_8_STATUS_2026-08-15.md`. Phase 4 criterion 14
   (stop before continuing) is marked "SUPERSEDED" by operator directive (ROADMAP:194).

### Recommendation

- Make **`docs/CLAIMS.md` the only status authority** and **one execution roadmap**. Rename
  `ROADMAP_AI_ERA.md` §2 as the product **definition of done**, and have BLUEPRINT own **phase order and stop
  protocol**. `docs/language/ROADMAP.md` then becomes a history file: freeze its legacy 0–10 arc and the
  2026-07 status blocks under a "historical" banner.
- Add one mapping table to BLUEPRINT: *AI-era criterion → Completion Phase → gate script → CLAIMS item*.
- Every "status" cell in any roadmap should be a link to a CLAIMS item or a dated receipt, never a colour
  glyph.
- Move the Phase 3 mission criteria into the repo, or stop citing "mission §N".
- Add the AI-era criteria to the docs-drift owned list (currently `scripts/lib/docs_drift_scan.py`, 15
  owned docs) so criterion status cannot drift silently.

---

## 2. Roadmap requirements and done-criteria

Legend for "Backed?":
- **code+test**: a spot-checked source file and test exist.
- **receipt-only**: backed only by a dated evidence doc or out-of-repo artifact.
- **unmet**: no implementing code was found.
- **residual**: published as open.

### 2a. ROADMAP_AI_ERA.md — 16 done-criteria (lines 69-84; landed notes at :144-262)

| # | Criterion (short) | Claimed status | Evidence file | Backed? (spot-check) |
|---|---|---|---|---|
| 1 | Forged certificate fails the bundle | "Measured: forged certificate → FAIL exit 1" (:167-169) | `tools/anubis/src/evidence_verify.rs:1103-1107` (`the_documented_forgery_is_rejected`, input `"1 2 0\n0\n"`) | **code+test at unit level** (`check_rup`). No end-to-end bundle test with a recomputed MANIFEST was found. |
| 2 | Third party re-checks with no Anubis binary (pinned drat-trim + cake_lpr in CI) | Not claimed | — | **unmet**. `drat-trim` appears only in generated `out/…/validate.sh`. `ci.yml` has 0 drat/lrat references. No cake_lpr. |
| 3 | Coverage number in the verdict | "landed" (:214-246) | `compiler/src/middle/mod.rs` ("obligations discharged by a machine-checked") | **code** present. Tests are asserted by the doc, not opened here. |
| 4 | No "trust us" witness (incl. vacuity queries) | Open (:256-258, REG-002) | CLAIMS item 6 | **residual** (default z3 trust. Opt-in `ANUBIS_REQUIRE_NATIVE_PROOFS=1`) |
| 5 | Byte-identical verdicts at 1x/4x/8x load | "started" (:98, :175-181) | CLAIMS:2319-2337 | **unmet**. CLAIMS:2332 says "The verdict-diff … has NOT been run". Mechanism is `rlimit` (`middle/mod.rs:15384`). |
| 6 | Kernel denies an undeclared effect | Not started (step 6) | — | **unmet**. No `seccomp`/`landlock` in the tree. (VZ confinement is a macOS hypervisor lane, not this.) |
| 7 | Kernel policy is a pure function of the program (BPF diff) | Not started | — | **unmet**. No BPF code. |
| 8 | Lean theorem: open effect row → most restrictive policy | Not started | — | **unmet**. No such theorem found in `formal/Anubis/*.lean`. |
| 9 | Machine-actionable refusals; agent converges on 20-program corpus | Format shipped. "The criterion is unmet" (:208-212) | `docs/language/DIAGNOSTICS_JSON.md` | **partial**: format exists, corpus absent (SPEC_1_0_FREEZE:48-50 agrees). |
| 10 | Vacuous contract (`ensures(true)`) refused; adequacy score | Not started | — | **unmet**. No `ensures(true)` fixture and no `adequacy` symbol. Existing "vacuous" code is premise-vacuity. |
| 11 | Conformance suite with DEFER class | Not started | — | **unmet** (the "DEFER" hits are solver-fragment deferrals, not conformance) |
| 12 | Monotonicity as conformance (accept-set N+1 ⊆ N) | Not started | — | **unmet** |
| 13 | Sealed Linux bootstrap fixpoint (3 guests, CI job, Linux tarball) | Not started (step 7) | — | **unmet**. No Linux baseline file, and CI is macOS only. A SIA note (origin `[model]`, not evidence) records a 2026-09-21 Linux 9/9 run with no committed baseline. |
| 14 | Lean↔Rust binding stronger than a name grep | Not claimed | `scripts/run_proof_correspondence_gate.sh` (G26), `run_security_label_correspondence_gate.sh` (G31) | **partial**. G26 checks that a named theorem *exists*, which is exactly what the criterion rejects. G31 is byte-for-byte for 6 `SecurityLabel` methods only. |
| 15 | One implant written in Anubis | Not started | — | **unmet** (AI_ERA:134-136 says so) |
| 16 | Archived proof re-checks years later | Not started | — | **unmet** |

AI-era sequence (AI_ERA:92-112): step 1 editions **landed** (`ANUBIS_EDITION_UNKNOWN`, EDITIONS.md). Steps 2–4
**started**. Step 5 coverage landed, but its LRAT half is not done. Steps 6–14 are not started.

### 2b. COMPLETION_BLUEPRINT.md phases (lines 51-60)

| Phase | Claimed status (where) | Evidence file | Backed? |
|---|---|---|---|
| 0 Define done / instruments | COMPLETE (AGENTS:129) | `PHASE_0_COMPLETION_2026-07-30.md`, `…07-31.md` | **receipt-only**. The 07-30 receipt itself says criterion 1 is "WORKING-TREE ONLY" (:41, :248). The gate exists (`run_promise_coherence_gate.sh`, G24). |
| 1 Evidence / isolation integrity | "bounded COMPLETE / ACTIVATED" (CLAIMS:67) | `PHASE_1_COMPLETION_2026-07-31.md:8` says "INCOMPLETE until the external finalization predicate" | **receipt-only**. The deciding receipt is `out/phase1_finalization_51f4_r2_…/receipt.md`, which is not tracked. |
| 1.5 GitHub as system of record | COMPLETE (`PHASE_1.5_COMPLETION_2026-08-13.md:3`) | same | receipt-only |
| 2 One total lane-parameterized walker | COMPLETE with waiver (`PHASE_2_…:3,7,59`) | same | **partial**: "walker families = 4" was waived to Phase 4 and is still 4 at Phase 4 (`PHASE_4_…:86`; ROADMAP:312). The target was not met. |
| 3 Security-label lattice | COMPLETE, 13/14 MET + 1 superseded (ROADMAP:162-194) | `PHASE_3_COMPLETION_2026-08-14.md`, `PHASE_3_VM_SEAL_2026-08-15.md` | **code+test**: `compiler/src/middle/security_label.rs`, G30 census, G31. The criteria source (mission file) is not in the repo. |
| 4 Close or publish residuals | COMPLETE (ROADMAP:290-323); receipt says "COMPLETE PENDING EXTERNAL SEAL" (`PHASE_4_…:3`) | `PHASE_4_COMPLETION_2026-08-15.md` | **receipt-only** for the "publish" half by design. The one closure (row 6) has code and fixtures. |
| 5 Complete language surface (optional) | OPTIONAL-COMPLETE (`COMPLETION_PHASES_5_8_STATUS:15`) | same | receipt-only. There is no per-phase receipt, contrary to BLUEPRINT:70. |
| 6 Permanent regression controls / CI | MET (`…STATUS:40`) | `scripts/audit_unified.sh:595` (31 gates, `EXPECTED_GATES`) | **code**. The gate count is quoted inconsistently: 29 (README:184), 30 (AGENTS:139), 31 in the script. |
| 7 Release evidence pack | PACK PRODUCED, `v0.1.2-preview` cut (`…STATUS:69`) | `docs/evidence/RELEASE_EVIDENCE_PACK_2026-08-15/` | receipt-only |
| 8 Mechanized correspondence research | UNSCHEDULED. Slice 1 only (`PHASE_8_SLICE_1_2026-08-18.md`) | `formal/Anubis/SecurityLabel.lean`, G31 | **code+test** for the bounded slice |

### 2c. docs/language/ROADMAP.md legacy phases (table at :347-362)

| Phase | Status cell (truncated read) | Backed? |
|---|---|---|
| 0 Trust spine | 🟡 PARTIAL, post-registry host fixpoint unsealed | Conflicts with CLAIMS item 3 ("VM seal DONE"). macOS-only. |
| 1 Real type system | 🟢 "engineered (re-verify types gate)" | CLAIMS:2392 "Generics are a STRING HEURISTIC — OPEN" (`ty.rs:258`) contradicts LANGUAGE_COMPLETENESS:11-12 "REAL". |
| 2 Capability & effect | 🔴 NOT soundness-complete | Consistent with CLAIMS item 21 |
| 3 Broaden verified surface | 🟡 PARTIAL | Float lane determinism is still OPEN (CLAIMS:2292-2337) |
| 4 Port checker into Anubis | 🟡 PARTIAL | Conflicts with :469 "DONE". See §4 below. |
| 5 Mechanized semantics | 🟢 "formal gate re-runnable (Lean 162/15)" | The count is stale (live 199/16). The gate is `run_formal_gate.sh`. |
| 6 Proof-carrying packages | 🟢 package gate | Not spot-checked |
| 7 Ma'at / TCB | 🟡 ADVANCED, division residual | CLAIMS item 4 |
| 8 DX | 🟢 historical 15/15 | Not re-run. CLAIMS B3 notes three `run_dx_gate.sh` checks graded on a substring. |
| 9 External reproduction | 🟡 WITNESSED 2026-07-22 | receipt-only (`phase9_independent_witness/`) |
| 10 Hardening + 1.0 freeze | 🟡 freeze published | SPEC_1_0_FREEZE exists. It is not a soundness seal. |

The Phase 3 (14 rows, ROADMAP:179-194) and Phase 4 (5 rows, :317-323) exit tables are all "MET". Their
evidence pointers are PR numbers and receipts. Only criteria 1–3 and 5 of Phase 3 point at code or gates.

---

## 3. Open defects in CLAIMS.md

"Open — load-bearing" starts at CLAIMS:110, but items 9–21 continue under later headings (to :1632). There is
**no item 8** (the numbering jumps from 7 at :242 to 9 at :545).

| ID | Headline status | Notes |
|---|---|---|
| 1 | D1–D6 "closed; NOT claimed total" (:112) | **Falsified by item 21** (15 true accepts, :1336, :1362-1366). Its heading still says closed. The closure rests on 4 named fixtures (:117-122). |
| 2 | check/run (R) CLOSED; (B)=7 by design (:134) | **Falsified by item 21** (1 TA, :1337). Heading unchanged. |
| 3 | Self-host registry: VM seal DONE (:138) | macOS tart seal. Conflicts with CAPABILITIES:184 and ROADMAP:353. |
| 4 | Permanent external/TCB residuals — OPEN (:148) | Keychain/SE, DNS-rebind, DDC not TT-total, Metal outside CI, non-pow2 division |
| 5 | REG-001 CLOSED (PR #16) (:154) | Fixture-backed |
| 6 | REG-002 CONDITIONALLY MITIGATED (:190) | **Open by default.** Opt-in env var only. Test: `tools/anubis/tests/reg002_z3_only_mitigation.rs` |
| 7 | REG-003 CLOSED (PR #15) (:242) | Fixture-backed |
| 9 | withdrawn, NOT A DEFECT (:545) | — |
| 10 | `requires` via fn-value carrier CLOSED (:561) | **Falsified by item 21** (6 TA). Its own fixture flipped inside `if 1 == 1 {}` (:1354-1356). Now addressed by item-21 Family 1 (below). |
| 11 | Nested-call `requires` CLOSED (:605) | **Falsified by item 21** (1 TA + 2 deferrals) |
| 12 | Bare-builtin / trifecta CLOSED (:625) | **Falsified in the sink direction** (5 TA, :1358-1360). Still open per :1581. |
| 13 | `run` mutual-return cycle CLOSED (:675) | — |
| 14 | Aggregate path seeders — PARTIALLY CLOSED (:693) | residual named |
| 15 | Research-lane gate immunity — OPEN as boundary (:764) | — |
| 16 | Dual-use surface: ~77% unprobed (:780) | open |
| 17 | build/run research consent — SOURCE/VZ-CLOSED, "unlanded and unshipped" (:857) | open for release |
| 18 | Tag-lane defect factory CONVERGED; user-fn carrier class OPEN (:908) | open |
| 19 | Purple report false ATT&CK claims — 3 of 4 closed (:1055) | residual named |
| 20 | 48-command offensive VZ-guard bypass — closed for bounded dirty epoch (:1177) | epoch-bounded |
| **21** | SIX items published CLOSED are OPEN — 30 TA (:1310) | See below |
| (unnumbered) | Float lane non-determinism OPEN (:2292); semantic diagnostics have no location OPEN (:2344); generics string heuristic OPEN (:2392); secret-selected constants nested form OPEN (:1818); container-PARAM carrier OPEN (:1950) | No stable IDs |
| B1–B5 | B1 VZ = safety not security (open); B2 closed; **B3 harness integrity: class OPEN** (13 of 48 gates adopt `gate_common.sh`); **B4 CLOSED (`0eb5977`) but falsified by item 21 (9 TA)**; B5 closed | :2429-2548 |
| R1–R8 | Resolved | :2551-2562 |

### Item 21 — rows (root-cause table at :1388-1399) and their current state

| Row | Mechanism | State per CLAIMS / git | Closure depends on |
|---|---|---|---|
| 1/2 | Carrier `requires` via guarded bodies / local alias | CLOSED `84296cef` (:1576-1584) | `contract_carrier.rs` (1,473 lines) plus **12** `examples/security/carrier_*.anb`. The receipt (`ITEM21_FAMILY1_…:39`) says **13**; the commit adds 12. Its 61-case acceptance matrix lives in `scratchpad/repro` and `~/.cache/anubis-item21/matrix2.sh`, **outside the repo**. So the closure is reproducible in-tree only through the 12 fixtures. Still open: fn values via a local container, `push`, and callee return (item-10 join lane). |
| 3 | `obj.f()` direct method-call carrier | **Closed in code by `13373e31`** (3 fixtures `*_via_place_assign_callexpr_*`). **CLAIMS does not record this.** CLAIMS:1581 still lists "the direct `obj.f()` sink-direction bare-builtin row" as open, which also merges row 3 with item 12's sink direction. |
| 4/5 | Non-`Var` place-assign sets only label | Addressed "by an intervening commit" (:1505-1506). No commit named. |
| 6 | Place-assign identity never read | CLOSED PR #35 `0b0889da`, **for the `let`-bound read shape only** (:1503-1504) | Named fixtures (:1550-1553) |
| 7 | Taint duals via place handler | Not separately stated | — |
| 8 | `place_struct_type` has no `Index` arm | Annotated: closed `03210603`. Unannotated: CLOSED `029d5538` | `029d5538` infers only **unanimous struct-literal** arrays and factory returns. Mixed or non-literal returns "infer nothing" (commit message), so a leak through those remains. 4 fixtures back it. |
| 9 | Unannotated formal (`fn leak(s){print(s.k)}`) | **Contradiction:** CLAIMS:1572 heads "rows 8-unannotated / 9 / 10 — CLOSED", while :1574-1575 says "The unannotated formal (D9) is NOT closed". The receipt (:59-60) confirms D9 is open. | — |
| 10 | Unannotated fn has `""` return type | CLOSED `029d5538` (factory shape only) | 2 fixtures |

Stale text inside item 21: CLAIMS:1608-1609 still says "no fix is claimed and none has been attempted",
above later closures.

The receipt's summary "9 of 11 reproduced leaks now closed" (`ITEM21_…:59`) counts an 11-leak scratch corpus.
It is not the 30-TA population item 21 measured. The remaining 30-TA population has not been re-measured.

**Closed items whose closure rests only on specific fixtures:** 1, 2, 10, 11, 12, B4. All six were
demonstrated false by item 21's falsifiers after being closed against their fixtures (:1376-1382). The new
closures (rows 1/2, 3, 8, 10) follow the same pattern: RED→GREEN on named fixtures plus a 0-flip corpus
diff. Only rows 1/2 add a structural "total collector". No independent falsification pass is recorded for
`13373e31` or `029d5538`.

---

## 4. Self-hosting inventory

**Anubis-authored components.** There is one file, `selfhost/src/anubis_sh.anb`. Its subcommands, dispatched
at :2624-2742, are `version`, `lex`, `parse`, `check`, `effects`, `capset`, `taint`, `types` and `compile`.

| Component | Coverage | Source |
|---|---|---|
| Lexer | SH token stream, byte spans. Matches `selfhost/golden/tokens/*` (3 goldens) | SELFHOST.md:49 |
| Parser | Recursive descent. Top-level items are **only `fn` and `enum`** (`sh_parse`, :865-882) | code |
| Check | Names, arity, annotated let mismatch, builtin allowlist | SELFHOST.md:51 |
| Effect engine | Transitive rows plus direct name-charge, lambdas/HOF closures, method calls | ROADMAP:79-95 |
| Capset | Whole-program capability set (`eff_program_capset`). 199-name builtin registry mirror | ROADMAP:95-103 |
| Type engine | Let/argument/return assignability only. `sh_infer_type` under-approximates Call, Index, FieldAccess, IfExpr and Match | `run_type_selfhost_gate.sh` header |
| Taint engine | Intraprocedural plus interprocedural param→sink / return-taint. Composite, closure, container and method interproc return `""` (deferred) | `run_taint_selfhost_gate.sh:21-25` |
| Contract / Z3 | **None.** "the SH checker … does not run Z3" (SELFHOST.md:197-199) | — |
| Codegen | Emits an **interpreter package** (Rust runtime seed plus JSON AST payload), not native lowering | SELFHOST.md:53, :281-288 |
| Runtime | `selfhost/runtime/anubis_sh_interp_rt.rs` (Rust seed). C port `selfhost/backend_c/anubis_sh_interp_rt.c`. C parser `anubis_sh_parse.c`. `backend_independent/token_scan.c` | — |
| Host schema | `compiler/src/selfhost_schema/mod.rs` (297 lines; `anubis selfhost dump-tokens/dump-ast`) | not read in depth |

**Constructs the SH grammar cannot parse** (spot-checked by grep on `anubis_sh.anb`, not by running it):
- `struct` declarations (no top-level arm)
- `impl` and `trait`
- `import` / modules
- attributes (`@verified`, `@research`, `@safe`)
- generic or qualified types: `parse_type` (:445-451) takes a **single token**, so `secret<T>`, `list<S>` and
  `<T: Bound>` fail
- float literals (`1.5` lexes as `1 . 5`, :395)
- `as` casts
- `loop`
- `?`
- `invariant` clauses
- `if let` / `while let` (not verified)

ROADMAP:110-113 names HM, generics/traits, typed-`?`, struct-field typing and float narrowing as structurally
unreachable. ROADMAP:109 records 208 of 490 corpus files as SH parse errors (SKIP).

**Doc conflicts on self-host scope:**
- SUBSET.md:28 and UNSUPPORTED.md:600-604 (d) say the "taint engine" is out of scope or not in Anubis.
  SELFHOST.md:52 and ROADMAP:469 say the taint engine *is* Anubis-authored.
- UNSUPPORTED.md (a) says `anubis_sh.anb` does not use full-language constructs. SELFHOST.md:203-206 says that
  was closed on 2026-07-12 (enum Stmt/Expr).
- DOGFOOD_PLAN:84 and REPRO_PLAN:81-84 still list DDC and the C-native parser as future or residual.
  SELFHOST.md:175-184 says both were closed.
- Gate counts disagree:
  - selfhost gate: 8/8 (`selfhost/README.md`) vs 9/9 (SELFHOST.md:112, SPEC_1_0_FREEZE:76)
  - DDC: 20 checks (REPRO_PLAN:71) vs 34/34 (SPEC_1_0_FREEZE:78)
- SELFHOST.md:52 says taint "SH soundly under-reports". Under-reporting taint is the fail-open direction.
  That is safe only because SH is non-authoritative, which the wording hides.

**Gates comparing SH with the Rust compiler:**

| Gate | Corpus | Granularity | Oracle |
|---|---|---|---|
| `run_effect_selfhost_gate.sh` | `tests/fixtures/effects_selfhost` (27 .anb) | **per-function** (function, cap) pair set | exact equality. Its SCOPE comment (:28-30) still says "no Lambda/CallExpr variant", which is stale per ROADMAP slice 2. |
| `run_capset_selfhost_gate.sh` | `capset_selfhost` (5) | **whole-program** capability set plus `effects_bounded` bit (vs `anubis vz confine`) | exact |
| `run_capset_corpus_failclosed.sh` | whole SH-parseable corpus | whole-program | DISAGREE=0 (no over-grant). CONSERVATIVE allowed. Not in the VM battery. |
| `run_type_selfhost_gate.sh` | `types_selfhost` (20) | **per-file** message set ("SH diags carry no fn attribution yet") | exact on the curated corpus |
| `run_taint_selfhost_gate.sh` | `taint_selfhost` (13) | **per-file** message set | exact on the curated corpus |
| whole-corpus SH ⊆ Rust (type, taint) | "a separate sweep" | aggregate | **No script found in `scripts/`.** The claimed 0-spurious result is not reproducible from the repo. |
| `run_selfhost_gate.sh` | stage0→1→2→3 | stage2.rs ≡ stage3.rs, plus binary cmp | Binary normalization is Mach-O only (`codesign`, `macho_normalize.py`, both `|| true`). It uses BSD `stat -f` (:40). |
| `run_selfhost_fulllang_gate.sh` | `examples/*.anb` | per-program stdout + exit code vs host `run` | "18 pass / 9 skip" (SELFHOST.md:67) |
| `run_selfhost_ddc_gate.sh` | anubis_sh + corpus | byte-identical emitted compiler | `pick_cc` accepts only gcc-12..15/tcc (:109) |
| `run_selfhost_repro_gate.sh` | fixpoint source | binary determinism; Docker lane optional | — |

**CI wiring.** None of the self-host gates is in `EXPECTED_GATES` (`audit_unified.sh:595`, G1–G31), so hosted
CI never runs them. They run only in `scripts/vm/run-slice.sh:369-374` (macOS tart guest) and
`scripts/run_seal_checklist.sh:240-242, :982`.

---

## 5. Stamped numbers that should be generated

`scripts/lib/docs_drift_derive.py` derives only **disk inventory**. Current output: security 356, language
259, stdlib fail-closed 104, doc_ok 23, stdlib modules 13, native corpus 956, builtins 213, Lean 199 / 16.
It cannot derive **pass counts**, so a "356/356" stamp is half-derived: the numerator is typed by hand.

Commit `c87ad1cf` ("stamp reconciliation") hand-edited README:174-175 from 337/937 to 356/956. README:173
says "Numbers are re-derived by command, never typed by hand."

| Location | Stamp | Problem |
|---|---|---|
| README:174-175, CLAIMS:78-84, AGENTS:151-155 | security N/N, language N/N, stdlib N/N, native N files, 213 builtins, 199/16 | Denominators are derivable. Pass numerators should come from the gate's verdict JSON. |
| README:116-118 | `certificates: 9/10 …` | Example output. It should be generated from a pinned fixture run, or marked illustrative. |
| README:155 | "213 builtins" | derivable; keep it bound to the deriver |
| README:184; AGENTS:139 | "29-gate roster" / "30-gate ci.yml" | The script has 31. Derive from `EXPECTED_GATES`. |
| AGENTS:6, :125 | "Last verified 2026-07-31", "Latest `main` is `d8742aab`" | Stale: HEAD is `c87ad1cf`. It should be generated from git. |
| AGENTS:97-102, CLAIMS:108 | "Builtins are 213", "Lean is 199 / 16" | derivable (already in the deriver). Remove the prose copies. |
| AGENTS:152, CLAIMS:1479-1486, :1618-1625 | cargo-test 1179/0, 1245/0, 766/766, 351/351, 357/357 | Not derivable today. They should come from a test-summary artifact. |
| CLAIMS:85, :89, :141 | "22/22" unified / VM battery | pin/epoch-bound. Link a receipt instead. |
| CLAIMS:81-82 | capset 5/5; "0 disagreements each" | Should come from the self-host gate outputs |
| ROADMAP:28-32, :68, :354 | Lean 162/15; security 228/228; language 244/244; native 681/0; 445-file corpus | Stale. Mark them historical or delete. |
| SPEC_1_0_FREEZE:76-82 | selfhost 9/9, repro 6/6, DDC 34/34, package 9/9, DX 15/15 | not derived; the counts disagree with REPRO_PLAN and the selfhost README |
| SELFHOST.md:67, :112; selfhost/README.md | fulllang 18/9; selfhost 9/9 vs 8/8 | same |
| REPRODUCIBILITY.md:7, :31 | "25 fixtures"; `/Users/sicarii/...` | stale; macOS path |
| ITEM21 receipt:39, :33-35 | "13 new fixtures"; "61 PASS" | 12 fixtures on disk; the matrix is out-of-repo |

**Recommendation.** Have the docs-drift gate consume gate verdict files (a pass/total pair from each
`run_*.sh` summary) rather than `find | wc -l`. Add `ROADMAP_AI_ERA.md`, `COMPLETION_BLUEPRINT.md`,
`SELFHOST.md`, `REPRODUCIBILITY.md` and `docs/evidence/*` (as historical-exempt) to the owned-doc list.
Replace every prose copy in AGENTS.md with a pointer to the deriver.
