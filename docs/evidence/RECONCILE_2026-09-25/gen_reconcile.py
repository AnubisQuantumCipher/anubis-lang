#!/usr/bin/env python3
"""Generate RECONCILE.tsv and RECONCILE.md from the rows below (single source for both files)."""
import collections, os

HERE = os.path.dirname(os.path.abspath(__file__))
COLS = ["id", "source", "sev", "claim", "class", "evidence", "matrix_case", "duplicates", "to_close"]

# Legend used in evidence: rc = exit status of `anubis-ord3b check` (run capped by this reconciliation);
# "wit 42/123456" = whole10 `run --no-verify` output with the secret literal 42, then 123456.
# p/... = program under /home/sicarii/.cache/anubis-item21/reconcile/p/.
R = []
def row(**k):
    R.append(k)

# ---------------------------------------------------------------- DEFECTS: alias candidate
row(id="CAND-CLOSURE-CAPTURE", source="DEFECTS:16; CLAIMS:18", sev="S1",
    claim="unshipped closure-effect worklist accepts retained-printer controls that disclose a secret",
    **{"class": "NOT-REPRODUCIBLE"},
    evidence="defect lives in a prototype that was never integrated. At head all 4 controls refuse: ord3_capture_self_old_print, _mutual_old_print, _self_old_print_scalar, _mutual_old_print_scalar rc=1 ANUBIS_ANALYSIS_LIMIT (fail-closed, but wrong class vs intent REJECT). They were registered after the ord3b matrix run, so history.tsv has no ord3b rows for them",
    matrix_case="yes: ord3_capture_* (4)", duplicates="CLAIMS UPDATE 2026-09-25 (CLAIMS:18)",
    to_close="keep as the acceptance bar for the capture-versioned fix; record the head outcome (ANALYSIS_LIMIT = wrong-class) in history.tsv")

# ---------------------------------------------------------------- DEFECTS: assurance chain
row(id="A-GATE-1", source="DEFECTS:22", sev="S2",
    claim="G18 compares default mode with ANUBIS_NATIVE_AUTHORITATIVE=1, the same mode, so 0 mismatches is guaranteed",
    **{"class": "NON-CODE"},
    evidence="verified by reading, not run. compiler/src/middle/mod.rs:18887-18896 native_authoritative() is true when the variable is unset; scripts/run_native_authoritative_gate.sh:95 (default leg) and :97 (=1 leg) take the same path. Partly mitigated: the ANUBIS_NATIVE_DISAGREE count (:100-103) is a real native-vs-z3 cross-check for native-decided obligations. The z3-only opt-out (=0) runs on one fixture only (:158)",
    matrix_case="no (gate)", duplicates="CLAIMS:99 and :2807 publish '0 mismatches' as evidence",
    to_close="a corpus leg with ANUBIS_NATIVE_AUTHORITATIVE=0 compared against the default; stop citing the vacuous mismatch count")
row(id="A-GATE-2", source="DEFECTS:23", sev="S2",
    claim="G18 drift check greps PROVEN_OP_TAGS, a list no Rust code reads; admission is term_ok/pred_ok",
    **{"class": "NON-CODE"},
    evidence="verified by reading, not run. solver/src/fragment.rs:56-89 PROVEN_OP_TAGS is read by no Rust code (the only other mention is the doc comment at solver/src/lib.rs:257). Admission is is_proven_authoritative (fragment.rs:47) -> pred_ok (:91) / term_ok (:112). The gate seds the list (run_native_authoritative_gate.sh:187-190). docs/PROOF_CORRESPONDENCE.md:29 calls the list 'a TOTAL match'",
    matrix_case="no (gate)", duplicates="none",
    to_close="derive the checked list from term_ok/pred_ok, or add a Rust test that the list equals the admitted variants")
row(id="A-GATE-3", source="DEFECTS:24", sev="S2",
    claim="run_native_shadow_gate.sh (PROOF_CORRESPONDENCE link-3 evidence) is in no roster; its floor file is absent",
    **{"class": "NON-CODE"},
    evidence="verified by reading, not run. scripts/run_native_shadow_gate.sh exists. No roster runs it: audit_unified.sh, ci.yml, run_seal_checklist.sh and scripts/vm/run-slice.sh do not; the only mention is a comment in run_effect_selfhost_gate.sh:3. Its floor scripts/floors/native_shadow_gate.count_floor is absent (checked: no *shadow* file in scripts/floors), and assert_floor fails on a missing floor (gate_common.sh:427-432). G26 only checks that the cited script exists",
    matrix_case="no (gate)", duplicates="HARNESS_INTEGRITY_AUDIT_2026-07-28.md:175 (the out/ redirect)",
    to_close="add the floor file and fix the out/ redirect, then register the gate in audit_unified.sh, or drop it from PROOF_CORRESPONDENCE link 3")
row(id="A-GATE-4", source="DEFECTS:25", sev="S2",
    claim="no gate runs verify_proof_bundle.py or drat-trim; evidence-verify reports PASS with no certificate",
    **{"class": "NON-CODE"},
    evidence="verified by reading, not run. scripts/verify_proof_bundle.py is called only in its own usage text; drat-trim appears only in the validate.sh template in compiler/src/evidence/mod.rs:262-282. No script or workflow calls evidence-verify. tools/anubis/src/evidence_verify.rs:296-302 lets uncertified rows through, and :352-370 still reports Pass",
    matrix_case="no (gate)", duplicates="none",
    to_close="a hosted gate that builds a bundle, runs verify_proof_bundle.py (and drat-trim when present), and enforces a minimum certified count")
row(id="A-EVID-1", source="DEFECTS:26", sev="S1",
    claim="solver_replay.json covers only the first obligation and says counterexample_replayed/replay_valid:true even when it passed",
    **{"class": "OPEN"},
    evidence="run: `check --evidence` on p/m/pj1_field_join_violated_assert.anb (rc=0) gives solver.json with 3 obligations, all PASS, and analysis/solver_replay.json = {status: counterexample_replayed, replay_valid: true} (out/pj1_evidence/). The code is at compiler/src/evidence/mod.rs:591-619 (the row's :532-551 is stale)",
    matrix_case="no (evidence artifact)", duplicates="none",
    to_close="one replay record per obligation, with a not_applicable/proved status for PASS")
row(id="A-EVID-2", source="DEFECTS:27", sev="S4",
    claim="PCA hard-codes solver_backend 'z3' though native is the default authority",
    **{"class": "OPEN"},
    evidence="run: the same pj1 bundle has pca.json solver_backend \"z3\" while each of its 3 obligations says 'native QF_BV solver; machine-checked bit-blaster'. Code: compiler/src/evidence/mod.rs:1015-1017 default_solver_backend() returns \"z3\"",
    matrix_case="no", duplicates="none",
    to_close="record the deciding backend per obligation (native-certified or z3) in the PCA")
row(id="A-EVID-3", source="DEFECTS:28", sev="S1",
    claim="a wrap-safety obligation z3 answers UNKNOWN passes check and PCA, but the bundle's own solver check fails it",
    **{"class": "OPEN"},
    evidence="read from code, not run (I did not construct a z3-UNKNOWN wrap query). middle/mod.rs:18166-18172 obligation_undecided_is_unsound leaves out wrap-safety, so the UNKNOWN->FAIL upgrade (:18320-18324) skips it; check fails only on FAIL (tools/anubis/src/main.rs:2853-2859); PCA solver_all_discharged = !any(FAIL) (evidence/mod.rs:1074-1076); the bundle's solver check requires all PASS (:663-667)",
    matrix_case="no", duplicates="none",
    to_close="one discharge predicate shared by check, PCA and bundle; an UNKNOWN wrap obligation fails closed or is refused")
row(id="A-EVID-4", source="DEFECTS:29", sev="S4",
    claim="obligation identity is kind prefix + raw SMT, with no location; proofs keyed by position; no proofs.json vs solver.json cross-check",
    **{"class": "OPEN"},
    evidence="read from code, not run. Names are built at middle/mod.rs:8270 (wrap), :9130-9214 (requires@), and the assert/ensures sites. SolverObligation/SolverCheck (:97-126) have no span. Proof files are obligation_{i:04} (evidence/mod.rs:501-502). evidence_verify.rs never reads solver.json. Seen in the as2/pj1 bundles: analysis/proofs/obligation_000N.*",
    matrix_case="no", duplicates="related to F-SPAN-1 (diagnostics, not obligations)",
    to_close="a span plus content-hash obligation ID, and a verifier join of proofs.json with solver.json on it")
row(id="A-LEAN-1", source="DEFECTS:30", sev="S1/S5",
    claim="only SecurityLabel is linked byte-for-byte (G31); BitBlast by name grep; 14 modules compile-only; 5 security models disconnected",
    **{"class": "NON-CODE"},
    evidence="verified by reading, not run. formal/Anubis/ has 17 files (16 with theorems plus the SecurityLabelObserver executable). G31 (audit_unified.sh:585-593) links SecurityLabel only. BitBlast is linked by a theorem-name grep (run_native_authoritative_gate.sh:178-183). G21 run_formal_gate.sh runs lake build plus a sorry/axiom scan. 14 unlinked modules; no Rust file or script names Capability, DeclassifyWellFormed, EffectSoundness, ModeAggregation or NonInterference",
    matrix_case="no", duplicates="CLAIMS:98,:123 publish '199 theorems/16 modules' with no linkage caveat; the caveat is in PROOF_CORRESPONDENCE.md:3-7",
    to_close="observer-style correspondence gates (like G31) for BitBlast and the 5 security/effect models, or scope the claim")
row(id="A-SOLVER-1", source="DEFECTS:31", sev="S1",
    claim="when native declines, z3 unsat is accepted with no certificate (REG-002)",
    **{"class": "OPEN"},
    evidence="run: p/m/as2_z3_only_vardiv_ensures.anb (ensures over a/b) rc=0 verdict PASS, 'certificates: 4/5 ... 1 trusted to the solver with none ... no witness (REG-002, out of the proven fragment)'. Default config: require_native_proofs() is false unless ANUBIS_REQUIRE_NATIVE_PROOFS=1 (middle/mod.rs:19073-19081). Found by the reading pass: that check sits inside `if native_authoritative()` (:18560,:18663), so with ANUBIS_NATIVE_AUTHORITATIVE=0, REQUIRE=1 is ignored (unverified by run)",
    matrix_case="no", duplicates="CLAIMS item 6 REG-002 (CLAIMS:205-256, :1782)",
    to_close="refuse z3-only obligations by default (or check a z3 proof); honour REQUIRE under the native opt-out")
row(id="A-CI-1", source="DEFECTS:32", sev="S2",
    claim="one workflow, one macOS job; no Linux CI; VM/Metal lanes are operator-run",
    **{"class": "NON-CODE"},
    evidence="verified by reading: .github/workflows/ contains only ci.yml, and its one job has runs-on: macos-latest (:23). ci.yml:173-174 runs audit_unified.sh --profile hosted (31 gates, EXPECTED_GATES :606)",
    matrix_case="no", duplicates="CLAIMS:306-347 (Phase 1.5) is stale: it names a sealed-vz-gate-suite job and a metal-prove.yml that do not exist; L-LINT-1",
    to_close="a Linux (ubuntu or aarch64) job running at least G1-G3 plus the hosted roster")

# ---------------------------------------------------------------- DEFECTS: frontend
row(id="F-PARSE-1", source="DEFECTS:38", sev="S1",
    claim="malformed source passes check with verdict pass",
    **{"class": "FIXED"},
    evidence="matrix parse_unexpected_paren, parse_trailing_operator, parse_bad_interpolation, parse_eof_midexpr: rc=1 (MALFORMED intent met); probes p/m/fj1, fj2: rc=1 with a located ANUBIS_PARSE_ERROR",
    matrix_case="yes: parse_* (4)", duplicates="F-JSON-1", to_close="closed; nothing further")
row(id="F-PARSE-2", source="DEFECTS:39", sev="S3",
    claim="no parser depth limit; deep nesting stack-overflows (134)",
    **{"class": "FIXED"},
    evidence="matrix limit_nesting_1000 and limit_nesting_20000: rc=1 diagnostic, no abort; parse_nesting_below_bound: rc=0",
    matrix_case="yes: limit_nesting_*, parse_nesting_below_bound", duplicates="none", to_close="closed")
row(id="F-SPAN-1", source="DEFECTS:40", sev="S3",
    claim="most AST nodes have no span; semantic diagnostics have no location",
    **{"class": "OPEN"},
    evidence="run: `check --message-format json` on p/m/fs1_semantic_failure_location.anb gives an ANUBIS_SECRET_EXFILTRATION diagnostic with no 'location' key, and the human output has no file:line. The parse diagnostic for p/m/fj1 carries file/line/column/span. Code: compiler/src/frontend/mod.rs:444ff Stmt has a span only on Let and LetPattern; the Expr enum (:513-636) has 7 span fields",
    matrix_case="no (not a verdict property)", duplicates="CLAIMS 'Semantic diagnostics carry NO location' (CLAIMS:2568); related A-EVID-4",
    to_close="spans on statements and expressions, threaded into SemanticDiagnostic.span (middle/mod.rs:129-133) and the JSON location")
row(id="F-RESOLVE-1", source="DEFECTS:41", sev="S1?",
    claim="only functions renamed per module; structs/enums/impls share one namespace; a.b vs a_b collide; same-last-segment imports overwrite",
    **{"class": "OPEN"},
    evidence="struct part is S1, confirmed with a leak. p/fres2/{lib,main}.anb: lib's `pub struct S { k: secret<i64> }` is shadowed by main's public `struct S`, and main does `print(lib::mk().k)` -> rc=0; wit 42/123456 prints 42/123456. Control p/fres2c (main without its own S) -> rc=1 SECRET_EXFILTRATION. The other parts fail closed or agree with the runtime: p/fres1 (a.b vs a_b both become a_b__get) -> rc=1 ANUBIS_DUPLICATE_FUNCTION; p/fres3 (x.util and y.util both alias util) -> rc=0, the runtime calls the last import and prints nothing, and p/fres3b (swapped) -> rc=1, so the overwrite is real but checker and runtime agree; p/fres4 (duplicate impl T::show) -> rc=1. Code: compiler/src/resolve/mod.rs:458-487 renames Item::Fn only",
    matrix_case="no (the matrix is single-file)", duplicates="none",
    to_close="module-qualify struct/enum/impl names, or refuse a type name defined in two modules; add a multi-file soundness case")
row(id="F-JSON-1", source="DEFECTS:42", sev="S1",
    claim="F-PARSE-1 violates DIAGNOSTICS_JSON: verdict must be fail when the check fails",
    **{"class": "FIXED"},
    evidence="run: `--message-format json` on p/m/fj1, p/m/fj2 and matrix parse_bad_interpolation, parse_eof_midexpr: rc=1, summary {verdict: fail, refused: 1}, diagnostic ANUBIS_PARSE_ERROR with a location",
    matrix_case="the parse_* cases (the matrix grades rc, not JSON)", duplicates="F-PARSE-1",
    to_close="mark fixed; optionally add a JSON-verdict assertion to the parse matrix")
row(id="F-PKG-1", source="DEFECTS:43", sev="S3",
    claim="git fetch without '--'; option injection; predictable shared temp dir; http:// registries accepted",
    **{"class": "OPEN"},
    evidence="read from code, not run. compiler/src/package/resolve_deps.rs:611-613 runs git clone with no '--', and checkout/fetch at :624, :631, :636 have none either; the temp path temp_dir()/anubis-git-fetch/<name>/<rev> (:601-604) is reused if it exists (:605-607); rev and name are not sanitised; registry.rs:113,:162 accept http://; curl gets no '--' (:222-223)",
    matrix_case="no", duplicates="none",
    to_close="'--' before URL/rev, a hex-SHA check on rev, a private mkdtemp, and refusing http://")
row(id="F-PKG-2", source="DEFECTS:44", sev="S4",
    claim="lockfile version never validated; stale-lock check looks at names only",
    **{"class": "OPEN"},
    evidence="read from code, not run. lock.rs:10 parses version, which is never checked and is replaced by a literal 1 on read-only resolution (resolve_deps.rs:216-219); the stale check (:79-89) only tests lock.get(name).is_none()",
    matrix_case="no", duplicates="none",
    to_close="reject an unknown lock version; compare each manifest spec to its locked version, source and rev")

# ---------------------------------------------------------------- DEFECTS: roadmap / self-host
row(id="R-DOC-1", source="DEFECTS:50", sev="S5",
    claim="ROADMAP says the certificate residual is CLOSED (LRAT); ROADMAP_AI_ERA says still DRAT",
    **{"class": "NON-CODE"},
    evidence="partly confirmed by reading. docs/language/ROADMAP.md:116-117 and :359 say CLOSED; docs/ROADMAP_AI_ERA.md:252-254 says still DRAT; the code writes .drat, and solver/src/lrat.rs is RUP-only with no hints. The row's CLAIMS item 6 citation is about a different residual (z3-only, no certificate)",
    matrix_case="no", duplicates="CLAIMS:251-252 (a different residual, cited as the same)",
    to_close="reword ROADMAP.md:116-117 and :359 to: RUP checked in-process, published as DRAT, LRAT-with-hints open")
row(id="R-DOC-2", source="DEFECTS:51", sev="S5",
    claim="self-host state given as DONE / PARTIAL / sealed / unsealed across docs",
    **{"class": "NON-CODE"},
    evidence="confirmed by reading, in more than four docs. DONE/sealed: CLAIMS:104,:153,:2877; ROADMAP.md:323,:469; AGENTS.md:123,133. Partial/unsealed: ROADMAP.md:145,:349,:353; CAPABILITIES.md:184; UNSUPPORTED.md:594; SPEC_1_0_FREEZE.md:76",
    matrix_case="no", duplicates="CLAIMS:104,:153,:2877",
    to_close="one authoritative self-host status line, with the other docs linking to it")
row(id="R-DOC-3", source="DEFECTS:52", sev="S5",
    claim="CLAIMS row 9 closed and open; 13373e31 not recorded; receipt says 13 fixtures, 12 exist",
    **{"class": "NON-CODE"},
    evidence="confirmed by reading; the lines moved: CLAIMS:1587 says rows 8/9/10 CLOSED, :1590 says D9 NOT closed, and :1603 says D9 closed (fc645f8f). 13373e31 still does not appear in CLAIMS, which at :1571-1575 and :1596 still lists obj.f() as open; this run shows it closed (i21_1 rc=1). docs/evidence/ITEM21_FAMILY1_CONTRACT_CARRIER_2026-09-23.md:43 says 13; 12 exist",
    matrix_case="no", duplicates="CLAIMS item 21",
    to_close="a dated CLAIMS update recording 13373e31 and reconciling row 9/D9; correct the receipt to 12")
row(id="R-SELFHOST-1", source="DEFECTS:53", sev="S4",
    claim="self-host is one file, parses only fn/enum at top level, has no contract engine, and no gate runs in CI",
    **{"class": "NON-CODE"},
    evidence="confirmed by reading (product gap). selfhost/src/anubis_sh.anb (2779 lines). sh_parse (:865-882) accepts fn/enum only; the keyword set (:95) has no struct/impl/trait/import/loop/as and no '?'; floats do not lex; requires/ensures are parsed with no solver. 9 scripts/run_*selfhost* gates run only in operator lanes (run_seal_checklist.sh:982-1004, scripts/vm/run-slice.sh:369-372), not in audit_unified.sh or ci.yml",
    matrix_case="no", duplicates="R-DOC-2",
    to_close="parser coverage for the 1.0 surface, plus a self-host gate in the hosted roster")
row(id="R-NUM-1", source="DEFECTS:54", sev="S5",
    claim="hand-typed pass counts; roster quoted as 29/30/31; AGENTS says main is d8742aab",
    **{"class": "NON-CODE"},
    evidence="partly confirmed by reading. Roster is 31 (audit_unified.sh:606) but is quoted as 29 at ci.yml:8,170, README.md:185, CI_TRUST_BOUNDARY.md:9 and audit_unified.sh:12,112, and as 30 at AGENTS.md:139. AGENTS.md:125 names d8742aab as main; origin/main is e34d0c89. Denominators are derived (docs_drift_derive.py:27-33); pass numerators are not",
    matrix_case="no", duplicates="none",
    to_close="derive pass counts and roster size from a gate report; drop the SHA from AGENTS.md")
row(id="R-STR-BYTES", source="DEFECTS:151", sev="S4",
    claim="native and z3 read non-ASCII/NUL in SMT string literals differently (solver API only)",
    **{"class": "OPEN"},
    evidence="not-run, unverified: this is a solver-API-only divergence (the compiler refuses such literals as unmodelable), and I did not drive the solver API",
    matrix_case="no", duplicates="none",
    to_close="byte-for-byte string literal semantics in native parsing, with a solver-API test")
row(id="R-IFEXPR-FOLD", source="DEFECTS:152", sev="S4",
    claim="a struct-literal field read inside an if-expression initializer is not folded (unmodeled)",
    **{"class": "OPEN"},
    evidence="run: p/m/rif1_structlit_field_in_ifexpr_init.anb -> rc=1 ANUBIS_ASSERTION_UNDECIDED (an over-refusal), while control p/m/rif1c (the same read outside the if) -> rc=0",
    matrix_case="no (c7_let_expr_block was a review probe)", duplicates="P-PREC-1 family",
    to_close="fold struct-literal field reads in if/match value arms")
row(id="R-FLOAT-DEF-GATES", source="DEFECTS:153", sev="S4",
    claim="NaN-aware Float64 '=' costs gates; 700+ chained float lets reach the native ceiling and go to z3 alone",
    **{"class": "OPEN"},
    evidence="not-run, unverified (a cost/ceiling item)",
    matrix_case="no", duplicates="A-SOLVER-1 (z3-alone consequence)",
    to_close="cheaper float '=' lowering or a higher native gate ceiling, measured")

# ---------------------------------------------------------------- DEFECTS: session findings
row(id="M-BUILTIN-HOF", source="DEFECTS:61", sev="S1",
    claim="map([1,-2], f) with contracted f checked clean (partially fixed 7465aa46)",
    **{"class": "FIXED"},
    evidence="fixed for apply/call/map over literals: matrix h_apply_violated, h_call_violated, h_map_violated rc=1 DISPROVED/UNDECIDED; h_*_satisfied rc=0. The residual is M-HOF-UNKNOWN",
    matrix_case="yes: h_* (6)", duplicates="M-HOF-UNKNOWN", to_close="closed; the residual is tracked in M-HOF-UNKNOWN")
row(id="M-HOF-UNKNOWN", source="DEFECTS:67", sev="S1",
    claim="a contracted fn mapped over a non-literal collection, or passed to reduce/compose/fold, is not discharged",
    **{"class": "OPEN"},
    evidence="reduce is a silent accept: p/m/mh3_reduce_contracted.anb (reduce([0-1], g, 0), g requires x>0) -> rc=0; the native run prints -1 (g entered with x=-1), then 0. The other forms fail closed: mh1 (map over a call result), mh2 (map over a formal list), mh5 (map over a pushed list), mh6 (filter over a call result) and mh4 (compose(f, id)) -> rc=1 ANUBIS_ASSERTION_UNDECIDED. 'fold' is not a builtin (BUILTINS.md:96). The IFC-lane twin of compose is silent: ordret3 COMPOSE-VALUE-CALLED rc=0, wit 42/123456",
    matrix_case="no (reduce)", duplicates="CLAIMS item 21 (CLAIMS:1607-1612: 'a higher-order builtin over a non-literal collection'); ORDRET3-REST COMPOSE-VALUE-CALLED",
    to_close="discharge reduce's callback per element (seed and accumulator havoced), or refuse; add mh3 to the matrix")
row(id="D9 (residual)", source="DEFECTS:71 (open) and :168 (fixed)", sev="S1",
    claim="unannotated formal: declared field qualifier not applied; residual d9_unknown_arg_type",
    **{"class": "FIXED"},
    evidence="matrix d9_unknown_arg_type rc=1 ANUBIS_SECRET_EXFILTRATION (PASS). The row at :71 is stale; :168 records the fix at 1b40f653",
    matrix_case="yes: d9_unknown_arg_type", duplicates="CLAIMS item 21 (:1590 'D9 NOT closed' is stale)",
    to_close="mark :71 fixed at 1b40f653")
row(id="L-SHADOW-1", source="DEFECTS:75 (S3) and :250 (S2)", sev="S2",
    claim="a local does not shadow a same-named user fn in call position; branch-write-then-shadow is symmetric between lane and runtime",
    **{"class": "OPEN"},
    evidence="matrix open_whole8_r31_r31i_s1 rc=0; wit 42/123456 prints '5|S { k: 42 ...}' / '5|S { k: 123456 ...}'. The call-position part is an unresolved language decision (s09/s10 recategorized with runtime witnesses)",
    matrix_case="yes: open_whole8_r31_r31i_s1", duplicates="M-GLOBAL-SCOPE (value-position sibling); HANDOFF 6.1",
    to_close="an owner decision on the shadowing rule, then a diagnostic or lexical scoping (edition), and the branch-write case fixed in the whole-struct lane")
row(id="M-IFLET-EXPR", source="DEFECTS:94", sev="S1",
    claim="discharge_calls_in_expr skips if-let then-branches and lambda bodies; skips value blocks that re-bind a tracked name",
    **{"class": "OPEN"},
    evidence="the if-let part is open. p/m/mi1 (let v = if let Some(y) = o { f(0-1) } else {0}) rc=0, native run prints -1 into f (requires x>0); mi2 (binder y=-5 passed to f) rc=0, prints -5; mi6 (if-let as a call argument) rc=0, prints -1. Controls: mi0 (statement if-let) and mi0b (expression if) -> rc=1 DISPROVED; mi7 (valid twin) rc=0. The lambda part is fixed: mi3 -> DISPROVED, mi4 (block body) -> UNDECIDED. The value-block part fails closed: mi5 -> UNDECIDED, and matrix rv7_block_shadow_call_unchecked passes",
    matrix_case="no for the if-let expression (m_direct_iflet_arm_violated is the statement form and passes)",
    duplicates="FV-VALUE-POS (DEFECTS:187, fixed; did not cover a let initializer); FV-CLOSURE-BODY; FV-BLOCK-SHADOW",
    to_close="discharge Expr::IfLet then/else in discharge_calls_in_expr with the binder modeled or havoced; register mi1/mi2/mi6 (REJECT) and mi7 (ACCEPT)")
row(id="M-GLOBAL-SCOPE", source="DEFECTS:95", sev="S1",
    claim="global name resolved before local scope in fn_identities_of_d, fn_alias_of_d, closure_arity_of, carrier_mentions_function",
    **{"class": "OPEN"},
    evidence="contract lane, value position. p/m/gs2 (fn show(x) global; let show = strict; let g = show; g(0-1)) rc=0; the native run prints -1 into strict (requires x>0). Control gs2b (identical, local named 'other') -> rc=1 DISPROVED; gs0 -> DISPROVED. IFC and capability forms reject: gs1, gs3 (SECRET_EXFILTRATION), gs4, gs6 (INTERPROC), gs5 (EFFECT_FORBIDDEN). IFC-lane container twin: ordret3 SHADOWED-LOCAL-IN-CONTAINER-AS-CALLBACK rc=0, wit 42/123456. Code: global-first at middle/mod.rs:513-517 (closure_arity_of), fn_alias_of_d (:563), :1489-1492 (fn_identities_of_d), :9741 (carrier_mentions_function)",
    matrix_case="no (gs2); related call-position cases open_ro1_*local* pass",
    duplicates="L-SHADOW-1 (call position); IFC-ALIAS-RESOLUTION 'local named like a user function' (fixed 1696925b, call position); ORDRET3-REST",
    to_close="resolve Var in value position scope-first in the four resolvers; register gs2 and the ordret3 twin")
row(id="M-SINK-ARGS", source="DEFECTS:96", sev="S1",
    claim="a sink builtin stored in a field/container and called is capability-charged, but its arguments never get the taint->sink check",
    **{"class": "OPEN"},
    evidence="all 10 shapes are silent accepts (rc=0), in both lanes. Secret lane, with print stored by field assign/field literal/list/map/index assign: ms5, ms6, ms7, ms8, ms10, each wit 42/123456 prints 42/123456. Taint lane: ms9 (b.f = write_file; b.f(path, input())) rc=0 and the native run wrote stdin 'tainted_ms9' to the file; ms1-ms4 (shell in field/literal/list/map) rc=0, check-only because shell has no native lowering. Controls: ms11 (direct write_file(input())) -> TAINTED_SINK; ms12 (let f = print; f(p.k)) -> SECRET_EXFILTRATION; the closure-wrapped forms (matrix t_field_closure_*, T-SINKARG) pass",
    matrix_case="no (only closure-wrapped sinks are registered: t_field_closure_*, r14/r15)", duplicates="T-SINKARG (DEFECTS:72, fixed, closures only)",
    to_close="when a field/index/map callee resolves to an egress or sink builtin, run the secret->egress and taint->sink argument checks; register ms5-ms10")
row(id="M-JOIN-CLOSURE", source="DEFECTS:97", sev="S1",
    claim="branch joins do not merge closure_lambda/field_closures, so after an if a var may still hold the pre-branch closure",
    **{"class": "OPEN"},
    evidence="IFC lane, under a public condition: mj1 (local), mj2 (field), mj3 (index), mj4 (else branch) -> rc=0, each wit 42/123456 prints 42/123456. Contract lane: mj6 (lambda reassigned in a branch) rc=0, the native run prints -1 into strict; mj5 (named fn in the branch) -> DISPROVED. Control mj0 (unconditional reassign) -> SECRET_EXFILTRATION",
    matrix_case="no", duplicates="ORDRET3-REST LAMBDA-WRITTEN-IN-BRANCH-LOST (rc=0, wit 42/123456)",
    to_close="union closure_lambda/field_closures at if/match joins in every lane; register mj1-mj4 and mj6")
row(id="P-JOIN-FIELD", source="DEFECTS:122", sev="S1?",
    claim="field symbols are missing from the join's exclusion sets; masked because field facts do not survive a join (latent)",
    **{"class": "NOT-REPRODUCIBLE"},
    evidence="latent as described; the masking still holds at head. pj2 (violated ensures over a field after a join), pj3 (valid twin) and pj4 (else-branch variant) all -> rc=1 ANUBIS_CONTRACT_UNPROVABLE, so field facts are not joined and the missing exclusion cannot fire. Side finding, see OBS-ASSERT-UNMODELED: pj1/pj8/pj9 (assert over a field after a join or loop) -> rc=0 with no assert obligation in solver.json, and the runtime traps (pj1: ANUBIS_ASSERT_FAILED)",
    matrix_case="no (fa01-fa05 were review probes)", duplicates="none",
    to_close="when field facts are joined, add 1fld_* symbols to the exclusion sets, and register pj3 (ACCEPT) and pj2/pj4 (REJECT) in the same change")
row(id="FV-OPEN-3 (residual)", source="DEFECTS:210", sev="S1",
    claim="open_whole2_B2 (a closure returned from a closure, let g = mk(); g()) still open",
    **{"class": "FIXED"},
    evidence="matrix open_whole2_B2 rc=1 ANUBIS_SECRET_EXFILTRATION (PASS); DEFECTS:443 (WHOLE-FN-RETURNS, 2f0b2805) lists it as fixed. The row at :210 is stale",
    matrix_case="yes: open_whole2_B2", duplicates="WHOLE-FN-RETURNS", to_close="mark :210 fixed at 2f0b2805")
row(id="FV-OPEN-4", source="DEFECTS:223", sev="S1",
    claim="closure capturing a secret fn passed as a formal; loop reassign after use; struct field set after use; closure returning a secret fn value",
    **{"class": "OPEN"},
    evidence="3 of 4 are open: open_closure_returning_secret_fn, open_reentry_closure_capturing_secret, open_struct_field_reassigned_after_use_secret -> rc=0, each wit 42/123456 (e.g. '1|42' / '1|123456'). open_loop_reassign_after_use_secret -> rc=1 (fixed 1696925b)",
    matrix_case="yes (4)", duplicates="IFC-HOF-NAMED (the loop case)", to_close="the three remaining shapes (callbacks unit)")
row(id="FV-OPEN-6", source="DEFECTS:246", sev="S1",
    claim="closure fetched from a container, chosen by an expression, reassigned in a branch, built by compose; panic as an output channel",
    **{"class": "OPEN"},
    evidence="open: open_whole4_w11_ip_s07, _l11, _l12 -> rc=0, wit 42/123456 prints the struct; open_whole5_r28_s4_panic_stderr_channel -> rc=0, and the native run's panic message on stderr carries 'S { k: 42 ...}' / 'S { k: 123456 ...}'. Fixed: open_whole4_w11_ip_s06 (2f0b2805), open_whole5_r28_s2, _s3 -> rc=1",
    matrix_case="yes", duplicates="DEFECTS:306,:392,:431,:446 (other FV-OPEN-6 rows); CLAIMS item 21 ('a function value flowing through a local container or out of a callee')",
    to_close="the callbacks unit: function values out of containers/payloads/fields; panic treated as egress")
row(id="FV-OPEN-6 (container, shadow)", source="DEFECTS:306", sev="S1",
    claim="closure fetched from a list or map whose captured name is shadowed before the call",
    **{"class": "OPEN"},
    evidence="open_whole13_rv36x_c1_closure_from_list_then_shadow, _c2_closure_from_map_then_shadow -> rc=0, wit 42/123456",
    matrix_case="yes (2)", duplicates="FV-OPEN-6", to_close="as FV-OPEN-6")
row(id="FV-OPEN-6 (container, coincidence)", source="DEFECTS:392", sev="S1",
    claim="a function taken out of a container; caught only through a name coincidence",
    **{"class": "OPEN"},
    evidence="open_rv37x_w1_nocoinc -> rc=0, wit 42/123456",
    matrix_case="yes (1)", duplicates="FV-OPEN-6", to_close="as FV-OPEN-6")
row(id="FV-OPEN-6 (round 38)", source="DEFECTS:431", sev="S1",
    claim="a function taken out of a container, caught before only through a name coincidence",
    **{"class": "OPEN"},
    evidence="open_rv38c_fut, _fut_nc, _u_mk_nocoinc, _u_main_nc -> rc=0, wit 42/123456",
    matrix_case="yes (4)", duplicates="FV-OPEN-6", to_close="as FV-OPEN-6")
row(id="FV-OPEN-6 (payloads, fields)", source="DEFECTS:446", sev="S1",
    claim="a function taken out of a variant payload by a pattern binder, a container or a field",
    **{"class": "OPEN"},
    evidence="open_whole4_w11_ip_s07, _l11, _l12 -> rc=0, wit 42/123456 (the same cases as DEFECTS:246)",
    matrix_case="yes (3)", duplicates="FV-OPEN-6 DEFECTS:246 (same cases)", to_close="as FV-OPEN-6")
row(id="IFC-PC-EGRESS", source="DEFECTS:247", sev="S1",
    claim="implicit flow: assignment, egress or return under a whole-struct-decided condition (filed as a policy decision)",
    **{"class": "OPEN"},
    evidence="open_whole4_l14_implicit_if_assign, open_whole4_l16_implicit_fn_if_assign, open_whole6_r29_s4_return_selected_by_break -> rc=0; the native run prints 0 with k=42 and 1 with k=123456 (a 1-bit implicit channel)",
    matrix_case="yes (3)", duplicates="HANDOFF 6.1/6.2 (split: the assignment/return forms are a lane gap; egress under a secret PC is the policy question)",
    to_close="ANUBIS_IMPLICIT_FLOW for assignment/return under whole-struct conditions (SPEC_1_0_FREEZE section 5); owner decision for egress under a secret PC; split the row")
row(id="FV-ENUM-SECRET", source="DEFECTS:248", sev="S1",
    claim="an enum variant that declares a secret payload reaches an egress whole; a struct-form variant's secret field read directly",
    **{"class": "OPEN"},
    evidence="open_whole11_r34_l_enumsecret_1 and _2 -> rc=0; wit prints E::A(42) / E::A(123456) and E::A { v: 42 } / E::A { v: 123456 }",
    matrix_case="yes (2)", duplicates="none", to_close="its own unit (design at $I/enumsecret-design.txt, re-check on the head pin)")
row(id="IFC-LOOP-CARRIED-VALUEBLOCK", source="DEFECTS:249", sev="S1",
    claim="a value carried to the next loop iteration inside a main-scope value block, or into a while-let binder, reaches an egress earlier in the body (ordinary lane)",
    **{"class": "OPEN"},
    evidence="new programs, all rc=0 with a leak: p/m/lc1 (while inside `let v = if true {..}` in main), lc3 (while let inside a value block), lc5 (the same inside a function, not only main), lc6 (for inside a value block). Wit 42/123456 prints '0|42|0' / '0|123456|0' (lc5: '0|42' / '0|123456'). The bare while-let binder in main scope is fixed: lc2 -> rc=1 SECRET_EXFILTRATION. Controls: lc4 (main while, no block), lc7 (direct print in a block), lc8 (non-carried loop in a block) -> rc=1. The cited evidence open_whole11_r34_*_symmetric never existed; the whole-struct twin rv34_r34i_5 passes",
    matrix_case="no (a new program is needed)", duplicates="none",
    to_close="seed loops inside value blocks (main and function scope) from the scalar lane's loop fixpoint; register lc1/lc3/lc5/lc6; fix the row's evidence id")
row(id="TY-UNENFORCED", source="DEFECTS:251", sev="S2",
    claim="declared types are not enforced (let s: list<i64> = \"ab\" checks)",
    **{"class": "OPEN"},
    evidence="p/m/ty1_list_annot_holds_string.anb -> rc=0; the native run prints 2. Also CLAIMS generics gen2 (Option<u32> = \"hello\") -> rc=0",
    matrix_case="no for the base shape; leak shapes below", duplicates="CLAIMS 'Generics are a STRING HEURISTIC' (under-rejection half); the TY-UNENFORCED sub-rows",
    to_close="enforce declared let/param/field types at bind, or refuse the mismatch; do not trust declared types in the lanes until then")
row(id="TY-UNENFORCED (literal fields)", source="DEFECTS:391", sev="S1",
    claim="a declared field type trusted for a struct literal (or list of them) holding another type",
    **{"class": "OPEN"},
    evidence="7 of 8 are open: open_rv37x_f_c1, _l4, _l5, _pt8, _r2, _x8a, _x8b -> rc=0, each wit 'u S { k: 42 ...}' / 'u S { k: 123456 ...}'. open_rv37x_f_r1 -> rc=1 (fixed)",
    matrix_case="yes (8)", duplicates="TY-UNENFORCED", to_close="as TY-UNENFORCED")
row(id="TY-UNENFORCED (annotations)", source="DEFECTS:412", sev="S1",
    claim="a declared struct annotation trusted for a value of another type",
    **{"class": "OPEN"},
    evidence="open_ro1_sym_mistyped_struct_annotation -> rc=0, wit 42/123456",
    matrix_case="yes (1)", duplicates="IFC-SUMMARY-GAPS (DEFECTS:370 listed it as a symmetric shape)", to_close="as TY-UNENFORCED")
row(id="TY-SHADOW-RETYPE", source="DEFECTS:305 (open) and :389 (fixed)", sev="S1",
    claim="retype tables keyed by name: a later shadowing let changes the type read for an earlier binding",
    **{"class": "FIXED"},
    evidence="matrix open_whole13_rv36x_sp2_part_edge_shadow_type_call and open_whole13_rv36x_rs1_receiver_shadow -> rc=1 SECRET_EXFILTRATION (PASS); twin rv36x_sp2_noshadow_twin passes. DEFECTS:389 records the fix at 8ea94c78; the row at :305 is stale",
    matrix_case="yes (2)", duplicates="CLAIMS:1665 ('Still OPEN: TY-SHADOW-RETYPE', stale)", to_close="mark :305 and CLAIMS:1665 fixed at 8ea94c78")
row(id="IFC-ORDINARY-RETURN", source="DEFECTS:307 (open) and :335 (fixed)", sev="S1",
    claim="a field-read secret assigned in a nested block of a callee and returned is lost; a joined alias checks one fn",
    **{"class": "FIXED"},
    evidence="open_whole13_r36_r36_spec_3_ordinary_lane_branch_assigned_secret_returned, open_whole13_rv36x_oh3_alias_join_one_named, rv36x_oh1_alias_join_secret_fn -> rc=1. Fixed at 9dc7be91; the row at :307 is stale",
    matrix_case="yes", duplicates="IFC-ALIAS-UNION", to_close="mark :307 fixed")
row(id="IFC-JOIN-BREAK-SHADOW", source="DEFECTS:339 (open) and :407 (fixed)", sev="S1",
    claim="joins keep only end states and skip shadowing paths (enforcing, value-block, param-return walkers)",
    **{"class": "FIXED"},
    evidence="open_rv36o_sib_brk_enf, _shd_enf, _shd2_enf, _brk_param, _shd_param, _vb_brk, _vb_shd -> rc=1 (7/7). Fixed at 1696925b; :339 and CLAIMS:1658 are stale",
    matrix_case="yes (7)", duplicates="CLAIMS:1658", to_close="mark :339 fixed")
row(id="IFC-ALIAS-PREFERENCE", source="DEFECTS:368 (open) and :403 (fixed)", sev="S1",
    claim="regression of 9dc7be91: an Unknown identity set checks one preferred alias",
    **{"class": "FIXED"},
    evidence="open_ro1_alias_regression_unknown_set_secret and _capability, ro2x_q_*abs*, rv36p_alias_reg_* -> rc=1",
    matrix_case="yes", duplicates="none", to_close="mark :368 fixed")
row(id="IFC-ALIAS-RESOLUTION", source="DEFECTS:369 (open) and :404 (fixed)", sev="S1",
    claim="unresolvable branches, value-block tails, value-position writes, lambda-and-name joins; a local named like a user fn",
    **{"class": "FIXED"},
    evidence="open_ro1_unknown_set_drops_joined_functions, _value_block_tail_identity, _fn_assigned_in_value_block_lost, _lambda_plus_named_fn_join_skips_closure, _struct_type_single_alias_at_join, _call_targets_local_shadowing_user_fn, _local_shadows_user_fn_name, _leak1_local_named_like_user_fn -> rc=1. But the review of 1696925b still has 24 silent leaks (ORDRET3-REST), and the expression-writes sub-row is open",
    matrix_case="yes (8)", duplicates="IFC-ALIAS-RESOLUTION (expression writes); ORDRET3-REST", to_close="mark :369 fixed")
row(id="IFC-ALIAS-RESOLUTION (expression writes)", source="DEFECTS:393", sev="S1",
    claim="a function written in expression position in a callee, called with a secret field",
    **{"class": "OPEN"},
    evidence="open_rv37x_ra_callee_pk and _stmt -> rc=0, wit 42/123456",
    matrix_case="yes (2)", duplicates="ORDRET3-REST PLACE-FN-WRITE-IN-VALUE-BLOCK-LOST (same family)", to_close="the ordinary alias unit (3.1 of the handoff)")
row(id="IFC-SUMMARY-GAPS", source="DEFECTS:370 (open) and :405 (fixed)", sev="S1",
    claim="return summary: local holding a fn, expression push/insert, ? as an exit, a field fn in a branch, symmetric shapes",
    **{"class": "FIXED"},
    evidence="open_ro1_ret_fn_identity_untracked, _ret_expr_position_push, _ret_try_operator_not_a_return, _implicit_flow_try_under_secret_branch, _enforcing_branch_join_drops_field_fn_identity, and 3 of the 4 _sym_* cases -> rc=1. The 4th symmetric shape (_sym_mistyped_struct_annotation) is still rc=0 and is tracked as TY-UNENFORCED (annotations)",
    matrix_case="yes", duplicates="TY-UNENFORCED (annotations)", to_close="mark :370 fixed")
row(id="IFC-HOF-NAMED", source="DEFECTS:371 (open) and :406 (fixed)", sev="S1",
    claim="a named fn given to builtin/user HOFs; capability not charged; loop reassignment after call",
    **{"class": "FIXED"},
    evidence="open_ro1_hof_builtin_local_fn_binding, _hof_builtin_capability_not_charged, _user_hof_named_fn_arg, _loop_reassign_after_call_egress, open_loop_reassign_after_use_secret -> rc=1. Related shapes are still silent (ORDRET3-REST BUILTIN-HOF-VIA-LOCAL-ALIAS, USER-HOF-VIA-LOCAL-ALIAS, USER-HOF-OVER-CALLBACK-LIST, ...)",
    matrix_case="yes", duplicates="ORDRET3-REST", to_close="mark :371 fixed")
row(id="FV-CLOSURE-RETURNS-FN", source="DEFECTS:413", sev="S1",
    claim="a local closure returning a function",
    **{"class": "OPEN"},
    evidence="open_ro1_closure_returning_function -> rc=0, wit 42/123456",
    matrix_case="yes (1)", duplicates="ORDRET3-REST HELPER-RETURNED-CLOSURE-INLINE (related)", to_close="the ordinary summary unit")
row(id="IFC-CALLBACK-RETURN (ordinary)", source="DEFECTS:445", sev="S1",
    claim="let g = call(|| pr); g(p.k) (the ordinary twin of whole-struct R38B-11)",
    **{"class": "OPEN"},
    evidence="new programs: p/m/cb1 (call) and cb2 (apply(|u| pr, [0])) -> rc=0, wit 42/123456 prints 42/123456. The whole-struct twin rv38_r38b_11_call_callback_returns_printing_fn -> rc=1",
    matrix_case="no (a new program is needed)", duplicates="none",
    to_close="the ordinary-lane unit: a call/apply/reduce result carries the function identities its callback returns; register cb1/cb2")
row(id="ORDRET3-REST", source="DEFECTS:481", sev="S1",
    claim="the review of 1696925b: alias (15 leaks), summary (9 leaks), over-refusal and cost clusters still open",
    **{"class": "OPEN"},
    evidence="the 36 leak programs from $I/review-ordret2-raw.json were extracted to p/ordret3rest/ and run: 24 are rc=0 on ord3b, 12 rc=1 (the clusters landed in e7b56507). All 24 open ones are witnessed: 22 by a 42/123456 output change; IMPLICIT-EXIT-GUARD-POSITIONS by changing only k (Err(0) vs b|Ok(1)); SUM-CLOSURE-BINDER-TAIL-TAINT by stdin reaching the written file. Over-refusal and cost clusters not-run",
    matrix_case="no (unregistered; HANDOFF 6.4)", duplicates="M-JOIN-CLOSURE, M-GLOBAL-SCOPE, M-HOF-UNKNOWN, CLAIMS item 18 extraction, IFC-ALIAS-RESOLUTION (expression writes)",
    to_close="register the 24 (REJECT) with their valid twins; land the alias and summary units")
row(id="UNFILED-MAINBINDER", source="HANDOFF 6.2 (no DEFECTS row)", sev="S1",
    claim="a main-scope match binder keeping a closure (round 35) is a silent accept with no DEFECTS row",
    **{"class": "OPEN"},
    evidence="open_whole12_r35_r35i_2_l_mainbinder -> rc=0, wit prints S { k: 42 ...} / S { k: 123456 ...}",
    matrix_case="yes (1)", duplicates="none", to_close="add a DEFECTS row; the whole-struct lane binder fix")
row(id="OBS-ASSERT-UNMODELED", source="new (this reconciliation); SPEC.md:153-158", sev="S2?",
    claim="an assert the checker cannot model is dropped at check time with no obligation and no refusal",
    **{"class": "OPEN"},
    evidence="p/m/pj7 (assert(len(str(c)) > 5)), pj1, pj8, pj9 (assert over a field after a join or loop) -> rc=0, 'certificates: N/N', and the assert is absent from solver.json (out/pj1_evidence). The native run traps (pj1: ANUBIS_ASSERT_FAILED, rc=1). docs/language/SPEC.md:153-158 (normative) says every assert is proved, disproved or UNDECIDED and 'unknown is never treated as the precondition holds'. Controls pj5 (scalar join) and pj6 (field, no join) -> DISPROVED. It may be intended: R-IFEXPR-FOLD accepts 'the runtime assert still traps'",
    matrix_case="no", duplicates="R-IFEXPR-FOLD (DEFECTS:152) treats the runtime trap as acceptable",
    to_close="owner decision: either refuse unmodeled asserts as UNDECIDED (SPEC as written) or amend SPEC to 'unmodeled asserts are runtime-checked'")

# ---------------------------------------------------------------- CLAIMS: known open issues
row(id="CLAIMS-4 TCB residuals", source="CLAIMS:163", sev="-",
    claim="Keychain/SE on OS signing; Softnet DNS-rebind; DDC not TT-total; hosted CI no Metal; native non-power-of-two division deferred",
    **{"class": "NON-CODE"},
    evidence="named external or TCB boundaries; nothing to reproduce. The division part is code and shows as REG-002: p/m/as2 (a/b) is z3-only with no certificate",
    matrix_case="no", duplicates="A-SOLVER-1 (division)", to_close="none in-repo except native division; keep the boundaries stated")
row(id="CLAIMS-6 REG-002", source="CLAIMS:205", sev="S1",
    claim="z3-only fragment forgeable by a compromised z3; mitigation opt-in",
    **{"class": "OPEN"},
    evidence="same evidence as A-SOLVER-1 (p/m/as2: 1 of 5 obligations trusted to z3 with no certificate, rc=0 by default)",
    matrix_case="no", duplicates="A-SOLVER-1", to_close="as A-SOLVER-1 (default-on refusal or in-process certificate replay)")
row(id="CLAIMS-14 aggregate path seeders", source="CLAIMS:708 (residual rows :741-748)", sev="S1",
    claim="rows 2/3/4/6/7/8: pattern bind, container returned, container param, element extractors, transforms, map results do not charge gate tags",
    **{"class": "NOT-REPRODUCIBLE"},
    evidence="none of the named rows reproduces as an accept. With bare write_file in the container and main declaring no uses, all 9 probes -> rc=1 ANUBIS_EFFECT_FORBIDDEN_IN_MODE: t14_r2 (if-let bind), t14_r3 (returned), t14_r4 (param), t14_r6 (first), t14_r6b (pop), t14_r7 (slice), t14_r7b (concat), t14_r8 (map get), t14_r8b (values). Which lane refuses (effect vs tag) is unverified; CLAIMS itself warns that an effect-lane refusal does not prove the tag lane carries the label",
    matrix_case="no", duplicates="CLAIMS-18 residuals; Container-PARAM",
    to_close="re-measure the tag lane directly (lane-attributed probe) before retitling item 14")
row(id="CLAIMS-15 research-lane gate immunity", source="CLAIMS:779", sev="-",
    claim="immunity is accidental: gated builtins have no lowering; not enforced, probed or tested",
    **{"class": "FIXED"},
    evidence="the entry is stale. A predicate and a barrier exist: NON_RUN_BUILTINS/POC_KIT_BUILTINS (compiler/src/backends/run.rs:3824-3827) feed the var_as_value predicate, and test gated_builtins_must_not_lower_in_emit_builtin_call (run.rs:9056ff, from 0e2344ed) covers them; I did not run the test. Run: `anubis-ord3b run` of p/m/c15 (let f = shell) -> rc=1 'ANUBIS_UNSUPPORTED_NATIVE_LOWERING: gated builtin `shell` cannot be used as a first-class value' (check itself is rc=0)",
    matrix_case="no", duplicates="none", to_close="update CLAIMS item 15 to cite the barrier and the test")
row(id="CLAIMS-16 dual-use refusal probes", source="CLAIMS:795", sev="-",
    claim="about 77% unprobed; 10 offensive modules assert no refusal (refusal-probe metric)",
    **{"class": "NON-CODE"},
    evidence="approximate re-derivation at head with my own test-body splitter (not the documented script): 37 modules, 241 #[test], about 84 refusal probes, 13 modules with no refusal probe (crypto, evasion, exploit, infrastructure, lolbas, opsec, packer, payloads, phish, postex, privesc, protocol (0 tests), reporting). The figures are approximate",
    matrix_case="no", duplicates="none",
    to_close="refusal/guard tests in the modules listed, plus a scripted metric that republishes the count")
row(id="CLAIMS-18 residuals", source="CLAIMS:1055-1062", sev="S1",
    claim="container as PARAM open; extraction (pop/remove) open; inline forwarder [identity(leak)] open; gu06 over-rejection",
    **{"class": "OPEN"},
    evidence="one extraction form is open: i18_4 (let f = first([leak]); f(..)) -> rc=0, and the native run wrote the file (the IFC twin ELEMENT-BUILTIN-EXTRACTED-FN in ORDRET3-REST is also rc=0 with a witness). Fixed: i18_1 (pop), i18_2 (remove), i18_3 ([identity(leak)]) -> rc=1 EFFECT_FORBIDDEN; gu06 (pure written over write_file at index 0) -> rc=0 (the over-rejection is gone). Container as PARAM: see the next row",
    matrix_case="no", duplicates="Container-PARAM; ORDRET3-REST", to_close="carry element identities through first/last/get; register i18_4")
row(id="CLAIMS-21 item 21 residuals", source="CLAIMS:1325 (:1570-1575, :1595-1597, :1607-1612)", sev="S1",
    claim="the direct obj.f() read shape; function values through containers or out of callees; HOF over non-literal collections; unannotated formal (container element); stale facts across loop-carried writes",
    **{"class": "OPEN"},
    evidence="the named direct shapes are fixed: i21_1 (b.f = key; print(b.f())) -> rc=1 SECRET_EXFILTRATION (let-bound control i21_1c -> rc=1); sink-direction bare builtin as a callable parameter i21_2 (write_file), i21_3 (print), i21_4 (shell) -> rc=1. Still open through other rows: FV-OPEN-6 (container/callee function values), M-HOF-UNKNOWN (reduce). The D9 formal is fixed. 'Stale facts across loop-carried and argument-embedded writes' was not probed (unverified)",
    matrix_case="partly (repro category)", duplicates="FV-OPEN-6, M-HOF-UNKNOWN, D9, R-DOC-3",
    to_close="retire the fixed sub-residuals in CLAIMS (R-DOC-3); the rest closes with FV-OPEN-6 and M-HOF-UNKNOWN")
row(id="CLAIMS secret-selected constants (nested)", source="CLAIMS:2042", sev="S1",
    claim="heading: nested bare-if-in-block form OPEN and reconstructs N bits",
    **{"class": "FIXED"},
    evidence="ssc1 (if true { if p.k > 100 {1} else {2} } else {0}), ssc2 (depth 3), ssc3 (secret_source) -> rc=1 ANUBIS_SECRET_EXFILTRATION. The body at CLAIMS:2078 already says 'CLOSED at arbitrary depth (1689a69)'; only the heading is stale",
    matrix_case="no", duplicates="none", to_close="retitle the heading to CLOSED (1689a69)")
row(id="CLAIMS Container-PARAM carrier", source="CLAIMS:2174", sev="S1",
    claim="a user fn with uses(fs.write) reaches an apply site through a container passed as a parameter; 10 shapes still accept",
    **{"class": "OPEN"},
    evidence="2 shape families of 16 probes are open, with the file actually written by the native run: closure over a param element, cp7 (|i| xs[i](..)) and cp15 (let f = xs[0]; |s| f(s)); if-bind over a param element, cp9 and cp16. The other 12 -> rc=1 EFFECT_FORBIDDEN: cp1 list, cp2 map, cp3 struct field, cp4 method formal, cp5 method x for, cp6 element alias, cp8 return-then-param, cp10 pattern bind, cp11 match, cp12 returned lambda, cp13 bare param, cp14 direct param lambda. The original fixtures (scratchpad/fleet_20260726) are not on disk, so my shape readings are my own",
    matrix_case="no", duplicates="CLAIMS-18 'container as PARAM'; FV-OPEN-6 (IFC lane)",
    to_close="carry param element paths through closures that capture them and through if/match value joins; register cp7/cp9/cp15/cp16 with pure twins")
row(id="CLAIMS float contract non-determinism", source="CLAIMS:2516", sev="S3",
    claim="float lane verdicts depend on solver luck; the saturation verdict-diff was never run",
    **{"class": "NON-CODE"},
    evidence="not-run. The mechanism fix exists (rlimit bound, middle/mod.rs:18334ff); the missing deliverable is the repeated full-corpus verdict-diff under saturation, compared byte-for-byte (ROADMAP_AI_ERA criterion 5)",
    matrix_case="no", duplicates="P-Z3-LOAD (DEFECTS:124)", to_close="run and publish the repeated saturation verdict-diff")
row(id="CLAIMS semantic diagnostics location", source="CLAIMS:2568", sev="S3",
    claim="semantic diagnostics carry no file:line; JSON location absent",
    **{"class": "OPEN"},
    evidence="same run as F-SPAN-1: the fs1 JSON diagnostic has no location. Observed as well: that diagnostic is labelled family 'frontend'",
    matrix_case="no", duplicates="F-SPAN-1", to_close="as F-SPAN-1")
row(id="CLAIMS generics string heuristic", source="CLAIMS:2616", sev="S1",
    claim="is_generic decides by string shape: <Item> over-rejects; Option<u32> = \"hello\" accepts",
    **{"class": "OPEN"},
    evidence="both halves reproduce. gen1 (fn pick<Item>) -> rc=1 ANUBIS_TYPE_MISMATCH, while control gen1c (<T>) -> rc=0; gen2 (let x: Option<u32> = \"hello\") -> rc=0 and the native run prints 'hello', while control gen2c (u32 = \"hello\") -> rc=1. Code unchanged: compiler/src/middle/ty.rs:388-394",
    matrix_case="no", duplicates="TY-UNENFORCED",
    to_close="consult the declared generic parameters (ctx.fn_generics) instead of the string shape; full-corpus verdict diff")
row(id="CLAIMS B1 VZ isolation", source="CLAIMS:2655", sev="-",
    claim="VZ isolation is safety, not security; host-forgeable markers; operator is the trust root",
    **{"class": "NON-CODE"},
    evidence="a boundary statement; nothing to reproduce", matrix_case="no", duplicates="CLAIMS VZ isolation section (:2102)",
    to_close="none (keep the boundary stated)")
row(id="CLAIMS B3 harness integrity class", source="CLAIMS:2658", sev="S2",
    claim="gate_common adoption (13 of 48); instrument provenance (9 gates grade an undigested target/release/anubis)",
    **{"class": "NON-CODE"},
    evidence="partly closed, by count: 35 of 53 scripts/run_*.sh source gate_common.sh, and G20_gate_common_adoption (scripts/check_gate_common_adoption.sh) is in the hosted roster. The instrument-provenance census was not re-enumerated (unverified)",
    matrix_case="no", duplicates="A-GATE-3, R-NUM-1",
    to_close="a census of gates that grade an undigested binary, and a shared coverage assertion called by every scorer")


def main():
    cnt = collections.Counter(r["class"] for r in R)
    with open(os.path.join(HERE, "RECONCILE.tsv"), "w") as f:
        f.write("\t".join(COLS) + "\n")
        for r in R:
            f.write("\t".join(str(r[c]).replace("\t", " ").replace("\n", " ") for c in COLS) + "\n")
    new_prog_open = [r["id"] for r in R if r["class"] == "OPEN" and r["matrix_case"].startswith("no")
                     and ("p/m/" in r["evidence"] or "p/fres" in r["evidence"] or "p/ordret3rest" in r["evidence"]
                          or "new programs" in r["evidence"])]
    return cnt, new_prog_open

NEW_PROGRAM_OPEN = [
    ("M-SINK-ARGS", "p/m/ms1..ms10 (ms5-ms8, ms10 witnessed; ms9 witnessed via file; ms1-ms4 check-only)"),
    ("M-JOIN-CLOSURE", "p/m/mj1..mj4, mj6"),
    ("M-IFLET-EXPR", "p/m/mi1, mi2, mi6 (valid twin mi7)"),
    ("M-GLOBAL-SCOPE", "p/m/gs2 (control gs2b)"),
    ("M-HOF-UNKNOWN", "p/m/mh3 (reduce)"),
    ("IFC-LOOP-CARRIED-VALUEBLOCK", "p/m/lc1, lc3, lc5, lc6 (controls lc2, lc4, lc7, lc8)"),
    ("IFC-CALLBACK-RETURN (ordinary)", "p/m/cb1, cb2"),
    ("F-RESOLVE-1", "p/fres2/ (control p/fres2c/)"),
    ("TY-UNENFORCED (base S2 row)", "p/m/ty1"),
    ("CLAIMS Container-PARAM carrier", "p/m/cp7, cp9, cp15, cp16"),
    ("CLAIMS-18 residuals (first-extraction)", "p/m/i18_4"),
    ("CLAIMS generics string heuristic", "p/m/gen1 (over-refusal), gen2 (accept + runtime)"),
    ("OBS-ASSERT-UNMODELED (new)", "p/m/pj7, pj1, pj8, pj9"),
    ("R-IFEXPR-FOLD (S4, over-refusal)", "p/m/rif1 (control rif1c)"),
    ("F-SPAN-1 / CLAIMS semantic location (JSON shape, not a verdict)", "p/m/fs1"),
    ("A-EVID-1, A-EVID-2 (bundle artifacts)", "p/m/pj1 --evidence -> out/pj1_evidence/"),
    ("A-SOLVER-1 / CLAIMS-6 REG-002", "p/m/as2"),
]

def md_escape(x):
    return str(x).replace("|", "\\|").replace("\n", " ")

def write_md(cnt):
    L = []
    L.append("# Reconciliation of DEFECTS.md and CLAIMS.md open items (head 3ea971bf, checker anubis-ord3b)")
    L.append("")
    L.append("Written 2026-09-25 by an independent read-only investigator. Nothing in the worktree was edited, built or run through cargo.")
    L.append("")
    L.append("## Instruments")
    L.append("")
    L.append("- Checker at head: `pins/anubis-ord3b` (sha256 `90a21f28c49bd044202698ba889f72bb4d9757b721671b194bf4c7a3b786c855`), source e7b56507. The commits e7b56507..3ea971bf touch docs and the matrix registry only (no `compiler/` or `tools/` diff). Its sha256 equals the `binary_sha256` of the `anubis-ord3b` rows in history.tsv.")
    L.append("- Runtime witnesses: `pins/anubis-whole10` (sha256 `8b2abb71472cc0509ec31294ce34c7c0ead6274c8f85c687e08f5f9515b30ab2`) `run --no-verify`. A leak witness is a change in output when the secret literal goes from 42 to 123456 (copies are in `p/wit/`). A contract witness is the violating value reaching the contracted function (it prints its argument). A capability witness is the file written by the native run.")
    L.append("- Every run went through `capped.sh`: `CAP=2G timeout 150-180`, with TMPDIR set to `reconcile/tmp`.")
    L.append("- rc in the table is the exit status of `anubis-ord3b check <file> --out <dir>`: rc=0 means check passed; rc=1 means refused, with the first ANUBIS_* code named.")
    L.append("")
    L.append("## Counts")
    L.append("")
    for k in ("FIXED", "OPEN", "NOT-REPRODUCIBLE", "NON-CODE"):
        L.append(f"- {k}: {cnt.get(k, 0)}")
    L.append(f"- total rows: {sum(cnt.values())} (60 DEFECTS-derived rows including sub-rows, 14 CLAIMS entries, 1 unfiled HANDOFF item, 1 new observation)")
    L.append("")
    L.append("## Matrix re-run (my own, on ord3b)")
    L.append("")
    L.append("- 117 cited registry cases were re-run (`matrix_rerun_ord3b.tsv`). For the 113 that have ord3b history rows, the accept/refuse outcome matches history in all 113 cases. The 4 without history are the `ord3_capture_*` controls, registered after the ord3b matrix run; all four give ANUBIS_ANALYSIS_LIMIT.")
    L.append("- The 32 REJECT-intent cases that ord3b accepts (the registered open leaks) all have a runtime witness from this run (`witness_matrix.tsv`).")
    L.append("- The 36 leak programs of the 1696925b review (`$I/review-ordret2-raw.json`) were extracted to `p/ordret3rest/`: 24 are still silent accepts at head, and all 24 are witnessed (`ordret3rest_ord3b.tsv`, `witness_ordret3rest.tsv`). None of the 24 is registered.")
    L.append("")
    L.append("## Items classified OPEN whose evidence is a new program (no matrix case yet)")
    L.append("")
    for i, e in NEW_PROGRAM_OPEN:
        L.append(f"- **{i}**: {e}")
    L.append("- **ORDRET3-REST**: 24 programs taken from the review JSON (not written by me), not registered: `p/ordret3rest/`")
    L.append("")
    L.append("## Table")
    L.append("")
    L.append("| " + " | ".join(COLS) + " |")
    L.append("|" + "---|" * len(COLS))
    for r in R:
        L.append("| " + " | ".join(md_escape(r[c]) for c in COLS) + " |")
    L.append("")
    L.append("## Notes")
    L.append("")
    L.append("1. **Stale rows in DEFECTS.md.** The same id appears open in an earlier follow-up section and fixed in a later one. The earlier row was never updated. These rows' cases pass at head: D9 at :71 (fixed 1b40f653, :168); FV-OPEN-3 residual at :210 (`open_whole2_B2`, fixed 2f0b2805, :443); TY-SHADOW-RETYPE at :305 (fixed 8ea94c78, :389); IFC-ORDINARY-RETURN at :307; IFC-JOIN-BREAK-SHADOW at :339; IFC-ALIAS-PREFERENCE, -RESOLUTION, -SUMMARY-GAPS and -HOF-NAMED at :368-371 (fixed 1696925b, :403-406). CI-MACOS-FIXTURES is open at :326 and fixed at :374 (S3, out of scope). CLAIMS carries the same staleness: :1658 and :1665 list IFC-JOIN-BREAK-SHADOW and TY-SHADOW-RETYPE as still open; :1571-1575 and :1596 list the direct `obj.f()` shape as open (it rejects: i21_1); the secret-selected-constants heading (:2042) says OPEN while its body (:2078) says CLOSED; item 15 (:779) says there is no barrier, and there is one.")
    L.append("2. **Duplicates across the two files.** A-SOLVER-1 = CLAIMS item 6 (REG-002). F-SPAN-1 = CLAIMS 'Semantic diagnostics carry NO location'. P-Z3-LOAD = CLAIMS float non-determinism. CLAIMS generics (under-rejection half) is related to TY-UNENFORCED. CLAIMS Container-PARAM = CLAIMS item 18's 'container as PARAM' row; it has no DEFECTS row. CLAIMS item 21's sub-residuals map to FV-OPEN-6, M-HOF-UNKNOWN and D9. The review leaks LAMBDA-WRITTEN-IN-BRANCH-LOST, SHADOWED-LOCAL-IN-CONTAINER-AS-CALLBACK, COMPOSE-VALUE-CALLED and ELEMENT-BUILTIN-EXTRACTED-FN are live instances of M-JOIN-CLOSURE, M-GLOBAL-SCOPE, M-HOF-UNKNOWN and the CLAIMS-18 extraction residual.")
    L.append("3. **Items with no DEFECTS row.** `open_whole12_r35_r35i_2_l_mainbinder` (UNFILED-MAINBINDER), the CLAIMS Container-PARAM carrier, the CLAIMS generics heuristic, CLAIMS secret-selected constants (fixed), OBS-ASSERT-UNMODELED (new), and the 24 ORDRET3-REST leaks (one umbrella row only).")
    L.append("4. **Wider than the rows say.** M-SINK-ARGS is live in the secret lane too: `print` stored in a field, list or map and called with a secret field (ms5-ms8, ms10). IFC-LOOP-CARRIED-VALUEBLOCK is not limited to main scope (lc5 is inside a function) and also covers `for` (lc6). The bare while-let binder in main scope is already fixed (lc2). M-GLOBAL-SCOPE was reproduced only in the contract lane (gs2); the IFC and capability value-position forms reject. M-HOF-UNKNOWN: only `reduce` is a silent accept; map, filter and compose over non-literal inputs fail closed as UNDECIDED.")
    L.append("5. **New observation (OBS-ASSERT-UNMODELED).** An `assert` the checker cannot model is absent from solver.json and does not refuse: check passes with 'certificates N/N', and only the native runtime trap enforces it. This contradicts the normative SPEC.md:153-158 text. It may be intended, since R-IFEXPR-FOLD treats the runtime trap as acceptable. The owner should decide which text governs. It also masks P-JOIN-FIELD probes written with `assert`: use `ensures`/`requires`, which report CONTRACT_UNPROVABLE.")
    L.append("6. **From the reading pass, not run.** With `ANUBIS_NATIVE_AUTHORITATIVE=0`, `ANUBIS_REQUIRE_NATIVE_PROOFS=1` is silently ignored (middle/mod.rs:18560, :18663). CLAIMS:306-347 (Phase 1.5) names a CI job and a `metal-prove.yml` that do not exist. The audit_unified.sh header says 29 gates while the roster at :606 has 31.")
    L.append("7. **Not run / unverified.** A-GATE-1..4, A-LEAN-1, A-CI-1, R-DOC-*, R-SELFHOST-1, R-NUM-1, F-PKG-1/2 and A-EVID-3/4 were checked by reading the cited files at 3ea971bf. I ran no gate script, no cargo test, and no z3-UNKNOWN construction. R-STR-BYTES and R-FLOAT-DEF-GATES were not attempted. CLAIMS-16's figures come from my own approximate splitter. CLAIMS-14's lane attribution is unverified.")
    L.append("")
    L.append("## Files")
    L.append("")
    L.append("- `RECONCILE.tsv`: the table, machine-readable, same columns. `gen_reconcile.py` generates both files from one row list.")
    L.append("- `p/m/*.anb`: probes I wrote. `p/fres*/`: multi-module probes. `p/ordret3rest/`: review programs. `p/wit/`: witness copies (42 / 123456).")
    L.append("- `matrix_rerun_ord3b.tsv`, `probes_ord3b.tsv`, `ordret3rest_ord3b.tsv`: check results (pin, program, rc, first code, ms).")
    L.append("- `witness_matrix.tsv`, `witness_probes.tsv`, `witness_ordret3rest.tsv`, `witness_single.tsv`: runtime witnesses.")
    L.append("- `out/`: per-run logs (`anubis-ord3b.<name>.log`, `run.<name>.log`), JSON outputs (`json.*`), and evidence bundles (`pj1_evidence/`, `as2_evidence/`).")
    L.append("- `chk.sh`, `runw.sh`, `wit.sh`: the capped runners used.")
    open(os.path.join(HERE, "RECONCILE.md"), "w").write("\n".join(L) + "\n")

if __name__ == "__main__":
    cnt, newp = main()
    write_md(cnt)
    print(dict(cnt), len(R))
