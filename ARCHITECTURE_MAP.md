# ARCHITECTURE_MAP.md — Anubis A+ (anubis-lang)

**Repo root (for this map):** `/Users/sicarii/anubis-lang/`

**Date of cartography:** 2026-07-05 (initial), reconciled 2026-07-11.

**Scope:** Non-generated source only. Excludes `target/`, vendored `vendor/risc0-circuit-rv32im/` (except noted templates/vendoring), generated `out/*/target/`, run outputs in `implementer/`, and empty placeholder dirs.

## 1. Crate / Workspace Structure

Workspace (see `/Users/sicarii/anubis-lang/Cargo.toml`):
- Members:
  - `compiler` (package `anubis-compiler`)
  - `tools/anubis` (the `anubis` CLI binary)

Top-level layout (non-generated source tree):

```
/Users/sicarii/anubis-lang/
├── Cargo.toml (workspace)
├── Cargo.lock
├── README.md
├── AGENTS.md
├── A_PLUS_ACCEPTANCE_CRITERIA.md
├── MATURITY_CLAIM_MATRIX.md
├── docs/history/   (2026-07-28: ROADMAP_A_PLUS.md, ANUBIS_REALITY_AUDIT.md,
│                    ANUBIS_CAPABILITY_CLAIM_MATRIX.md, ANUBIS_BUILD_MISSION.md,
│                    A_PLUS_FINAL_REPORT.md, A_PLUS_CLOSEOUT.md — archived, none current)
├── compiler/
│   ├── Cargo.toml
│   ├── CHANGES.md (records canonical edits to compiler/src)
│   └── src/
│       ├── lib.rs (reexports + crate test suite; 265 unit tests across the compiler crate)
│       ├── frontend/mod.rs (lexer + parser; 3924 LOC)
│       ├── middle/mod.rs (typecheck, TypedIR, TaintPass, SymbolicEngine, contracts; 3736 LOC)
│       ├── middle/ty.rs (static type checking; B1-B3 refinement types)
│       ├── backends/
│       │   ├── mod.rs
│       │   ├── run.rs (whole-program interpreter/transpiler; 5835 LOC — the execution backend)
│       │   └── native/
│       │       ├── mod.rs (lower_to_native + research gate + hybrid dispatch)
│       │       └── hybrid/
│       │           ├── mod.rs
│       │           ├── emit.rs (template emission)
│       │           ├── build.rs (cargo orchestration + methods export; 1 test)
│       │           └── templates/ (9 files: *.toml + *.rs for fast/full hybrid)
│       ├── evidence/mod.rs (bundle build + validate + PCA + ZK binding; 1262 LOC)
│       ├── fmt/mod.rs (Anubis source formatter)
│       └── resolve/mod.rs (name resolution)
├── tools/
│   └── anubis/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs (CLI entry; 5154 LOC; build/doctor/verify/prove/run/report/keygen/sign)
│           ├── poc_kit.rs (bounty-grade PoC harness)
│           ├── proof_input.rs (RISC0 proof inputs)
│           └── offensive/ (engagement-scoped C2/red-team platform)
│               ├── mod.rs, agent.rs, engagement.rs, exploit.rs
│               ├── lateral.rs, listener.rs, packer.rs, persistence.rs
│               ├── receipts.rs, rop.rs, scope.rs, vz.rs (VZ isolation)
│               └── console.rs (RBAC operator console)
├── examples/ (102 files: 58 .anb, 3 .anubis, 41 .anub across 9 subdirs)
│   ├── hybrid_stub.anubis
│   ├── research_poc.anubis
│   ├── safe_hello.anubis
│   ├── proof/ (proof_factorial_input, proof_fib, proof_enum_status, etc.)
│   ├── security/ (poc_local_overflow, poc_packing_smoke, vz_modular_exploit, etc.)
│   ├── feel/ (8 dogfood programs: lexer, engagement ledger, etc.)
│   ├── tour/ (22 language-tour programs: arithmetic through generics/traits)
│   ├── programs/ (11 standalone programs: BFS, BST, VM, fractions, etc.)
│   ├── industry/ (2 industry-domain programs)
│   └── physics/ (2 physics-domain programs)
├── docs/
│   ├── spec.md
│   ├── hybrid-reference-patterns.md
│   ├── repo-hygiene.md
│   └── adr/
│       ├── 0001-bootstrap-rust-host.md
│       └── 0002-evidence-format.md
├── scripts/ (25 gate/fixture/audit scripts)
│   ├── audit_unified.sh (master gate runner — 15 gates, one command)
│   ├── run_language_fixtures.sh, run_turing_core_fixtures.sh
│   ├── run_pca_gate.sh, run_prove_gate.sh, run_poc_kit_gate.sh
│   ├── run_offensive_platform_gate.sh, run_enum_match_gate.sh
│   ├── run_for_in_gate.sh, run_lang_trio_gate.sh, etc.
├── tools/grok-safety-check.sh
├── .github/workflows/ci.yml (fmt/clippy/test + reproducibility on research_poc + verify)
├── src/ (empty placeholder)
├── frontend/ (empty placeholder)
├── middle/ (empty placeholder)
├── native/ (empty)
├── metal/ (empty)
├── riscv/ (empty)
├── backends/ (empty)
├── evidence/ (empty)
├── stdlib/ (empty)
├── tests/ (empty)
├── bounties/ (empty)
├── out/ (mix of committed test fixtures + generated artifacts; see §5)
├── implementer/a_plus_audit_run/ (audit run outputs)
└── vendor/ (risc0 vendored for full-hybrid; not primary source)
```

**Active compiler source is exclusively under `compiler/src/`** (per CHANGES.md and lib.rs). Root-level empty dirs appear to be legacy scaffolding.

**Binaries / entrypoints:**
- Library: `anubis-compiler` (pub API: `parse_source`, `typecheck`, `TaintPass`, `SymbolicEngine`, `lower_to_native`, `build_evidence_bundle`, `validate_bundle`).
- CLI: `tools/anubis/src/main.rs` → `anubis build <file> [--evidence|--bounty] [--full-hybrid]`

## 2. Call Flow (CLI → parse → typecheck → TaintPass → SymbolicEngine → lower_to_native)

Exact flow (primary paths):

1. **CLI entry** (`/Users/sicarii/anubis-lang/tools/anubis/src/main.rs:89`):
   ```rust
   let src = std::fs::read_to_string(&input)?;
   let ast = parse_source(&src)...;
   let mode = first_mode(&ast.items).unwrap_or(Mode::Safe);
   let typed = typecheck(ast, mode)...;
   let tainted = TaintPass::apply(typed.clone());
   let _constraints = SymbolicEngine::generate_constraints(&src);
   let art = lower_to_native(tainted, &out, "anubis_out", full_hybrid)...;
   if do_evidence { build_evidence_bundle(...) }
   ```

2. **Parse** (`compiler/src/frontend/mod.rs`):
   - `parse_source(source)` (1051) → `parse_source_detailed` → `lex_spanned` (spans, keywords for `research`/`assume`/`assert`/`hybrid`/`tainted`/`symbolic`/etc.) → `Parser::parse_output` / `parse_fn` / `parse_stmt` / `parse_assume_or_assert` / `parse_hybrid` etc.
   - Produces `AST { items: Vec<Item> }` with `Fn { body: Vec<Stmt> }`, `Stmt::ResearchBlock`, `Expr::Assume(Box<Expr::Binary>)`, `Expr::Tainted`, `Stmt::HybridBlock { gpu, cpu, prove }` etc.
   - Span-aware; recoverable diagnostics.

3. **Typecheck** (`compiler/src/middle/mod.rs:111`):
   - `pub fn typecheck(ast: AST, mode: Mode) -> Result<TypedIR, String>`
   - `collect_items` → `analyze_function` → `analyze_stmts` (250) + `analyze_expr_effect` (410).
   - Populates `TypedIR { body, hir, mir, symbols: Vec<BindingInfo>, taint_labels, taint_traces: Vec<TaintTrace>, constraints, solver_obligations, has_research, ... }`.
   - Mode inference in frontend (`infer_mode` 1006) based on presence of blocks; rawptr rejection in safe (269); taint tracking + sink diagnostics (430).

4. **TaintPass** (`compiler/src/middle/mod.rs:462`):
   ```rust
   pub struct TaintPass;
   impl TaintPass {
       pub fn apply(mut typed: TypedIR) -> TypedIR { ... }
   }
   ```
   - Adds `derived_from:...` labels from tainted symbols.
   - Pushes `trace: ...` labels from `taint_traces`.
   - Called after typecheck in CLI (95), evidence (132), lib tests (e.g. 390, 472).

5. **SymbolicEngine** (`compiler/src/middle/mod.rs:494`):
   - `generate_constraints(source)` (497) — reparses + typechecks (Safe) for `ir.constraints`.
   - `check_obligations(ir)` (503) — runs `run_z3_obligation` for each `SolverObligation`.
   - Called in CLI (97), evidence (137), tests (e.g. z3_solver_reports... 491).

6. **lower_to_native** (`compiler/src/backends/native/mod.rs:69`):
   ```rust
   pub fn lower_to_native(ir: TypedIR, out_dir: &Path, name: &str, _full_hybrid: bool) -> Result<String, String>
   ```
   - Detects `is_research` (82) or `is_hybrid` (via `has_hybrid_block` 52).
   - **Research path (83-123)**: `collect_research_driver` (30) + `extract_assume_bound` (9) → **THE RESEARCH ASSUME-BOUND GATE**:
     ```rust
     // lines 89-98
     let var_name = source_x.ok_or_else(|| "research lowering requires a tainted source variable".to_string())?;
     let bound_lit = source_bound.ok_or_else(|| {
         format!("research lowering requires assume({} < bound) from parsed AST", var_name)
     })?;
     // then emits: let write_idx = if {} < {} {{ {} as usize }} ...
     ```
     Exact error strings and gate logic cited in lib.rs test `research_lowering_requires_ast_assume_bound` (257).
   - Hybrid path (124-183): delegates to `hybrid::emit_hybrid_project` + `hybrid::build_hybrid_host`.
   - Safe path: simple stub.
   - Also called directly from lib.rs tests (208, 271, 328, 375, 734, 802).

**Evidence path** (`compiler/src/evidence/mod.rs:130`):
- Reparses + `typecheck` + `TaintPass::apply` + `SymbolicEngine::check_obligations(&tainted)`.
- Writes `hir.json`, `mir.json`, `taint-traces.json`, `solver.json` etc.

## 3. Taint Data Flow (labels + traces → bundles)

- **Population sites** (all in `middle/mod.rs`):
  - `analyze_function` (200): param taint from `is_tainted_type`.
  - `analyze_stmts` (260):
    - Let: `init_taint = expr_taint_source`, explicit `tainted<...>`, declassify source tracking (279), `taint_labels.push` for sources/derived (289-310).
    - Research/Exploit/Hybrid: set `has_research`, recurse.
    - Assume/Assert: constraints + obligations.
    - Assign: taint trace (377).
    - If: tainted-branch effect.
  - `analyze_expr_effect` (410):
    - Sink calls (`is_sink`): push `TaintTrace { source, sink, steps, declassified }` (419); safe-mode diagnostic if not declassified (430).
    - Declassify: trace + effect (440).
    - Tainted calls: effect.
  - `expr_taint_source`, `expr_is_declassified`, `declassify_source` helpers (not shown in excerpts but referenced).

- **TaintPass::apply** (462): augments `taint_labels` with `derived_from` and `trace: src -> sink (declassified)?`.

- **Emission points**:
  - `TypedIR.taint_labels` / `.taint_traces` carried through lowering (used in comment string at native/mod.rs:78).
  - Evidence bundle (`evidence/mod.rs:132-137, 152-159`): `taint_json = ...taint_traces...`; check "taint" if labels/traces non-empty.
  - Written to `taint-traces.json` (239).
  - CLI summary (tools/...:149) references it.
  - Tests: `taint_propagates` (386), `taint_tracks_sink_and_declassify_traces` (459) — asserts specific traces for `raw -> sink` (non-declass) and declass path.

- **When emitted to bundles**: Only on `--evidence` / `--bounty` or explicit `build_evidence_bundle`. Always includes taint-traces for patterns that pass typecheck + lowering (research + assume structure required for full lowering in research cases).

Traces are **observational** (populated on recognized patterns); enforcement is partial (diagnostics in safe + lowering gate for research bare cases).

## 4. Evidence Bundle Contents and Validation Points

**Bundle dir** (timestamped under `out/` or provided `--out`, e.g. `evidence-YYYYMMDD-HHMMSS-research/`):

Core files (always):
- `source.anubis` (exact input snapshot)
- `build.log`
- `artifact` (the lowered binary, if produced)
- `hir.json`, `mir.json`, `taint-traces.json`, `solver.json`
- `environment.json` (os/arch/rustc/cargo/z3/anubis version)
- `checks.sarif` (SARIF from non-PASS checks)
- `bounty-report.md` (human readable)
- `validate.sh` (chmod +x; runs `anubis verify`)
- `source-tree.json` (hashes + sizes of tracked files)
- `evidence.json` (EvidenceManifest)
- `MANIFEST.sha256` (hash lines for all files)

Hybrid sidecars (if present, copied from alongside artifact):
- `guest.elf`, `image_id.txt`, `generated-methods.rs`
- Extra checks: `hybrid_..._hash`

**Manifest** (`EvidenceManifest`): timestamp, tool, mode, source_hash, artifact_hash (Option), lane, env/source_tree/sarif/report hashes, manifest_signature, `checks: Vec<Check>`, `verdict: "PASS"|"FAIL"`.

**Checks populated** (evidence/mod.rs:110+):
- parse, typecheck, taint, symbolic, solver, source_hash, build_log_hash, artifact(_hash), hybrid_*, artifact_hash strict PASS/FAIL.

**Validation points** (`validate_bundle` 316 + `validate_manifest_hashes` 539):
- MANIFEST.sha256 lines match file hashes.
- source.anubis hash == manifest.source_hash
- build.log / environment / source-tree / sarif / report hashes (if present)
- artifact hash (if present)
- ALL `checks[].status == "PASS"`
- `manifest.verdict == "PASS"`
- Used in: CLI `verify`/`validate` (199), CI reproducibility job, lib tests `validate_bundle_rejects_*` (583+), `evidence_bundle_captures_and_hashes_hybrid_sidecars` (630).

Tamper test fixture: `out/audit/evidence-tampered/` (with pre-tampered files + validate.sh).

## 5. All .anubis Fixtures and Locations

Committed / primary (used by tests + docs):
- `/Users/sicarii/anubis-lang/examples/research_poc.anubis` — main research POC (tainted symbolic + assume(x < 191) + assert; included via `include_str!` in lib.rs:201,642; README/CI).
- `/Users/sicarii/anubis-lang/examples/hybrid_stub.anubis` — hybrid { gpu(metal) ... cpu ... prove } (lib.rs:726,795).
- `/Users/sicarii/anubis-lang/examples/safe_hello.anubis` — safe baseline.

Out/ fixtures (test data, audit cases, copies; used in manual runs, evidence-tampered, sec*):
- `out/audit/program_a.anubis`, `program_b.anubis`, `program_c.anubis` (some empty or minimal).
- `out/audit/safe_tainted_sink.anubis`, `sec_research_sink.anubis`, `sec_research_sink_full.anubis`, `declassify_bare.anubis`, `z3_bad.anubis`.
- `out/audit/evidence-tampered/source.anubis`.
- Top-level `out/` copies: `program_a_safe.anubis`, `program_b_tainted_unsafe.anubis`, `program_c_research_poc.anubis`, `sec_raw_safe.anubis`, `sec_bad_assert.anubis`, `sec_tainted_sink.anubis`, `sec_research_ok.anubis`, `sec_bad_assert.anubis`, `z3_bad.anubis`, plus subdirs `out/audit/run1/`, `sec5/`, `z3/`, `hybrid-test/`, `c-research/` etc. containing .anubis or references.
- `out/sec5/sec_research_sink.anubis` etc.

Many `out/*/ *.anubis` are either committed audit cases or copied during `anubis build` / test runs. Tests prefer `include_str!` from `examples/` for reproducibility.

## 6. Exact Locations of Lowering, Taint Enforcement, Solver Bridge, Hybrid Emission

- **Lowering entry + research assume-bound gate**:
  - `/Users/sicarii/anubis-lang/compiler/src/backends/native/mod.rs:69` (`pub fn lower_to_native`)
  - Helpers: `extract_assume_bound:9`, `collect_research_driver:30`
  - Gate: `89-98` (exact error: "research lowering requires assume({} < bound) from parsed AST")
  - Called from: tools/anubis/main.rs:102, compiler/src/lib.rs (multiple: 208,271,328,375,734,802), evidence indirectly via artifact.

- **Taint enforcement**:
  - Typecheck + analysis: `/Users/sicarii/anubis-lang/compiler/src/middle/mod.rs:111` (typecheck), `250` (analyze_stmts), `410` (analyze_expr_effect), `430` (safe sink diagnostic).
  - Pass: `462` (TaintPass::apply).
  - Rawptr safe rejection: `269`.
  - Traces: `TaintTrace` struct `49`; population for sinks/declass/assign.

- **Solver bridge**:
  - `/Users/sicarii/anubis-lang/compiler/src/middle/mod.rs:494` (SymbolicEngine impl).
  - `503` `check_obligations`, `520` `run_z3_obligation` (spawns `z3 -in -smt2`), `599` `obligation_to_smt`, `610` asserts, `620` `expr_to_smt`.
  - Z3 model/counterexample on "sat".

- **Hybrid emission**:
  - Dispatch: `native/mod.rs:149` (emit + build based on `_full_hybrid`).
  - `/Users/sicarii/anubis-lang/compiler/src/backends/native/hybrid/emit.rs:8` (`emit_hybrid_project`; chooses fast/full toml + main; copies vendor for full).
  - `/Users/sicarii/anubis-lang/compiler/src/backends/native/hybrid/build.rs:8` (`build_hybrid_host`; cargo build --release; `export_generated_methods` for full; extracts ANUBIS_ID/ELF).
  - Templates (included): `hybrid/templates/` (Cargo.fast/full.toml, host_main*.rs, methods_*, guest_*, etc.). Fast: metal dispatch + R0_DISABLE_METAL/ANUBIS_REQUIRE_METAL. Full: risc0 + vendored patch + receipt.verify shape.
  - Sidecar copy + hashing in evidence `copy_hybrid_sidecars:440`.

- **Other key**:
  - Frontend Stmt/Expr: `frontend/mod.rs:100` (Stmt), `140` (Expr).
  - TypedIR: `middle/mod.rs:79`.
  - Evidence build/validate: `evidence/mod.rs:79` / `316`.
  - CLI orchestration + first_mode: `tools/anubis/src/main.rs:70` / `223`.

## 7. Known Brittle Points and Dead/Stub Code

**Brittle / partial (from docs/history/ROADMAP_A_PLUS.md:20, docs/history/ANUBIS_REALITY_AUDIT.md, code comments):**
- Research assume-bound gate (native/mod.rs:89) is **scoped only to research lowering**; bare/safe tainted-to-sink or declassify-without-assume hit exact gate error ("research lowering requires assume..."). Roadmap Phase 3: "Brittle assume gate scoped/removed for safe taint paths."
- Taint-to-sink/declassify is **mostly reporting/traces** (populated on recognized patterns) + safe-mode diagnostic. Full policy enforcement incomplete for arbitrary flows (REALITY_AUDIT: "PARTIAL"; traces require the assume+research structure).
- Z3: optional (falls to FAIL if unavailable); solver checks only for obligations from Assert under assumptions.
- Lowering for research produces minimal observable Rust (env/arg driven write_idx); no full memory model yet.
- Hybrid: fast vs full; full requires vendored risc0 + cargo in lower path; Metal Tier2 probe; R0_DISABLE_METAL fallback. Receipt verification shape present in templates/tests but live fresh end-to-end sometimes timed out in audits.
- Evidence: strong on hashes + PASS-all + MANIFEST; but depends on successful lowering/artifact for full hashes.
- Parser: minimal (no full expr precedence edge cases beyond tests; recoverable but limited recovery).
- Git: noted UNBORN in some audit docs.
- Old audit weaknesses (sec5, program_b tainted unsafe, z3_bad, raw safe, bare declassify, tainted sink): now exercised in tests/fixtures but some still hit gates or require specific structure.

**Dead / stub / legacy (explicit comments):**
- `native/mod.rs:7`: "// Legacy emit_stmt/expr_to_str removed (were dead; research path uses collect/extract + inline)."
- Old hardcoded templates (256/300/100) explicitly rejected in tests (lib.rs:224 "must not be old hardcoded template 300").
- Comments in lib.rs:211,219,372,568,725,771 reference "no stubs", "not fallback", "real dispatch".
- Empty placeholder dirs at root (src/, frontend/, middle/ etc.).
- In hybrid templates/build: CPU fallback explicit; no silent shim (build.rs:2 comment).
- Some `out/` .anubis are empty or minimal (program_a/b/c.anubis in audit/).

**Other notes:**
- No constant-time evidence yet.
- Solver fidelity/replay, broad taint policy, full stdlib, LSP etc. in roadmap.
- Vendor is patched risc0 for Metal hybrid (referenced in hybrid-reference-patterns.md).

## 8. Current Test Surface (covers old audit weaknesses)

Primary tests: **62 `#[test]` in `compiler/src/lib.rs`**, **173 `#[test]` in `backends/run.rs`** (plus 1 in hybrid/build.rs). Total: **265 compiler + 56 tools = 321 tests.**

Key test names (with coverage):
- `parses_safe_program`, `parser_records_spans_params_and_precedence`, `parser_reports_spanned_diagnostics_and_recovers`, `parses_research_with_tainted_and_symbolic`, `parser_accepts_imports_and_modules_with_recovery`.
- `lowers_research_poc_to_source_driven_rust` (199) — uses `include_str!("../../examples/research_poc.anubis")`; asserts "x < 191", env/arg behavior, no old hardcoded.
- `research_lowering_requires_ast_assume_bound` (257) — exact gate test (missing assume → error containing "assume" && "bound").
- `research_constraints_include_nested_assume_and_assert`, `research_lowering_preserves_non_x_tainted_variable_name`.
- `parses_hybrid_and_spec_blocks`.
- `taint_propagates` (386), `taint_tracks_sink_and_declassify_traces` (459) — exact trace assertions for sink + declassify.
- `safe_mode_rejects_raw_pointer_without_research_boundary` (446) — covers rawptr audit case.
- `z3_solver_reports_counterexample_for_failed_assertion` (491) — uses z3_bad pattern; asserts FAIL + model with "x".
- Evidence / tamper:
  - `evidence_bundle_contains_reference_grade_metadata_and_reports` (523)
  - `validate_bundle_rejects_tampered_source_snapshot` (583)
  - `validate_bundle_rejects_tampered_artifact` (603)
  - `evidence_bundle_captures_and_hashes_hybrid_sidecars` (630)
  - `validate_bundle_rejects_manifest_rewrite_without_manifest_hash_update` (688)
- Hybrid:
  - `hybrid_host_compiles_and_dispatches` (723)
  - `hybrid_full_project_emits_methods_vendor_patch_and_receipt_contract` (794)
  - `hybrid_emission_snapshot`, `hybrid_fast_template_honors_lane_contract`, `hybrid_generated_cargo_projects_are_workspace_isolated`, `hybrid_full_template_uses_risc0_305_receipt_shape`.
- In `hybrid/build.rs:125`: `extracts_generated_anubis_id_after_type_annotation`.

**Old audit weaknesses covered** (sec5 / program_b / tainted sink / z3_bad / raw safe / research_poc / bare declassify / sec_research_*):
- Directly via fixtures in `out/audit/` + `out/sec*` + `examples/research_poc.anubis`.
- Tests: taint sink/declass traces, z3 counterexample, rawptr rejection, research_poc lowering + bound gate, safe tainted cases, evidence tamper (including sec patterns).
- CLI/CI: `anubis build examples/research_poc.anubis --bounty` + `verify`.
- Many `out/audit/sec5/`, `z3/`, `run1/2/` contain generated .rs / bounty-summary.json from these cases.
- Tamper evidence in `evidence-tampered/`.

**Additional coverage:**
- `cargo test` (all), clippy, build (CI).
- Repro: build + verify on research_poc.
- Doctor (z3/rustc presence).
- Manual audit packs in `out/audit/`.

**Test entrypoints:** `cargo test -p anubis-compiler` (lib tests), `cargo test --all`, direct `anubis` CLI on fixtures.

## 9. Summary Citations (key file:line)

- Gate: `compiler/src/backends/native/mod.rs:89-98` + test `research_lowering_requires_ast_assume_bound:257` (lib.rs).
- TaintPass: `middle/mod.rs:462`.
- Symbolic: `middle/mod.rs:494`, `520`.
- Evidence taint/solver: `evidence/mod.rs:132-137`, `239`.
- CLI flow: `tools/anubis/src/main.rs:89-102`.
- Fixtures: `examples/research_poc.anubis` (bound 191), `out/audit/*`, `out/sec*.anubis`.
- Tests: 62 in lib.rs + 173 in backends/run.rs + 1 in hybrid/build.rs = 265 compiler; 56 tools.
- Hybrid: `backends/native/hybrid/{emit,build}.rs` + templates/.

This map is derived exclusively from direct file reads, greps, and directory listings (no edits, no external assumptions beyond visible code/docs).

**Next steps per roadmap:** Phase 3 hardening of the assume gate + taint policy, expanded test surface for bare cases.
