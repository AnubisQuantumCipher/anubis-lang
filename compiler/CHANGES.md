# Anubis Compiler CHANGES (this run canonical edits)

This file records canonical edits performed in /Users/sicarii/anubis-lang/compiler/src during the current goal session for the Anubis v0.1 MVP.

## Edits this session (canonical compiler/src only)
- compiler/src/frontend/mod.rs : generalized assume/assert top-level + research block parsing to build Binary { op, lhs: Var, rhs: Literal } for bounds (e.g. x < 191). Removed reliance on string heuristics for top-level.
- compiler/src/middle/mod.rs : TypedIR retains body: Vec<Stmt> from AST; typecheck populates body + taint_labels + constraints from Let/Assume/ResearchBlock; TaintPass records derived_from; expr_to_smt and SymbolicEngine for real constraints.
- compiler/src/backends/native/mod.rs : lower_to_native + emit_stmt is pure AST walk over ir.body (no appended POC sim template, no footer, no hardcoded 256/300/100). emit_stmt produces executable: tainted Let reads from ANUBIS_TEST_X/env/arg, Assume Binary -> real if !(var < bound) guard, Assert checks. extract_assume_bound + collect_research_driver drive write_idx = if x < BOUND from source.
- compiler/src/evidence/mod.rs : build_evidence_bundle computes + includes artifact_hash (from .rs or binary after copy); pushes strict "artifact_hash" Check (PASS/FAIL only, no INFO leaks); manifest has artifact_hash: Option; validate_bundle requires all PASS + hash matches.
- compiler/src/frontend/mod.rs : added span-aware lexing (`lex_spanned`), `Span`, `ParseDiagnostic`, `ParseOutput`, and `parse_source_detailed`; the public `parse_source` path now uses the new parser foundation with function parameters, let-init expressions, binary precedence, and recoverable diagnostics.
- compiler/src/lib.rs : tests (parses_*, parser foundation tests, lowers_research_poc_to_source_driven_rust, hybrid contract tests, etc.) now drive parse_source / typecheck / lower_to_native directly on include_str! or literal examples/research_poc.anubis (191 bound); assert emitted .rs contains "x < 191", "if x < 191", POC observable; behavior runs with arg/env; 18 tests.

## Other this-run hygiene (outside src but required for gates)
- .github/workflows/ci.yml : fixed malformed YAML (reproducibility job was nested under build.steps; now sibling top-level job under jobs).
- compiler/CHANGES.md : added to satisfy "CHANGES_FILE must reflect canonical .../compiler/src edits this run".
- compiler/src/backends/native/hybrid/templates/* : tightened the emitted hybrid host lane contract to match risc0-metal-hybrid patterns: `R0_DISABLE_METAL` forces CPU, Metal requires `MTLArgumentBuffersTier::Tier2`, lane labels are `metal-hybrid` / `cpu`, shared buffers go through `checked_base_ptr`, and the full template uses `get_prover_server(&ProverOpts::default())` with an actual optional `receipt.verify(image_id)` path.

All changes in this tranche were made on canonical /Users/sicarii/anubis-lang only. No copy/ directories introduced. Current gates include cargo fmt, cargo check, cargo clippy -D warnings, cargo test, release build, actionlint, fresh build/verify bundles, fast hybrid runtime, and full hybrid host build/runtime checks.

Target: 191 bound (not 256) for skeptic contract.
