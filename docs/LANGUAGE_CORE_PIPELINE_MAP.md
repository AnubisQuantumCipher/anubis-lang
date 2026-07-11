# Anubis Language Core Pipeline Map (Gate 2 / Gate 3 Slice)

**Date:** 2026-07-06 (A+ maturity branch a-plus-maturity/20260705-1649)
**Purpose:** Baseline snapshot before expanding the general-purpose language layer. Captures exact current implementation so Gate 2 (real language core) and Gate 3 (parser/AST/HIR/MIR maturity) work can be measured precisely without regressing sealed Gates 4/5/7/8/10/11.

## 1. Parser Entry Points

- Primary public: `compiler/src/frontend/mod.rs`
  - `pub fn parse_source(source: &str) -> Result<AST, String>`
  - `pub fn parse_source_detailed(source: &str) -> ParseOutput` (returns AST + `Vec<ParseDiagnostic>`)
  - `pub fn parse(tokens: Vec<Token>) -> Result<AST, String>`
  - `pub fn lex(source: &str) -> Vec<Token>`
  - `pub fn lex_spanned(source: &str) -> Vec<SpannedToken>`
- Internal: `Parser` struct with `parse_output`, `parse_fn`, `parse_stmt`, `parse_let`, `parse_expr` (precedence climbing), `parse_primary`, `parse_assume_or_assert`, `parse_hybrid`, `parse_spec`, recovery paths.
- Used by: `tools/anubis/src/main.rs` (check/build/prove), `compiler/src/lib.rs` (tests), `compiler/src/evidence/mod.rs`, `compiler/src/middle/mod.rs` (fallback parse in some paths).
- Never-panics goal: lexer skips gracefully; parser uses `Option` returns + diagnostics collection; `parse_source` returns Err on fatal but detailed path always produces output.

## 2. Token / Lexer Model

- `enum Token` (in frontend/mod.rs): Ident(String), Keyword(String), Number(String), StringLit(String), LParen/RParen/LBrace/RBrace/LBracket/RBracket, Colon, Semi, Comma, Dot, Star, Amp, Lt/Gt/Le/Ge, Eq/EqEq, Plus, Minus, Eof, Other(String).
- `struct SpannedToken { token: Token, span: Span }`
- `struct Span { start: usize, end: usize }` (byte offsets; `merge` helper).
- Lexer (`lex_spanned`): char_indices, skips whitespace, // line comments (to \n), recognizes keywords on the fly in parser, numbers, strings (basic), operators. No block comments yet (// only).
- `lex` wrapper strips spans.
- Current: line comments supported and stripped; no nested /* */ (PLANNED).

## 3. AST Node Definitions (frontend/mod.rs)

```rust
pub struct AST { pub items: Vec<Item> }

pub enum Item {
    Import { path: String, span: Span },
    Module { name: String, items: Vec<Item>, span: Span },
    Fn { name: String, params: Vec<(String, String)>, body: Vec<Stmt>, mode: Mode, intent: Option<String>, span: Span },
}

pub enum Stmt {
    Let { name: String, ty: Option<String>, init: Expr, span: Span },
    Assign { target: Expr, value: Expr },
    If { cond: Expr, then: Vec<Stmt>, else_: Option<Vec<Stmt>> },
    ResearchBlock { intent: Option<String>, body: Vec<Stmt> },
    ExploitBlock { intent: Option<String>, body: Vec<Stmt> },
    HybridBlock { gpu: Option<Vec<Stmt>>, cpu: Option<Vec<Stmt>>, prove: Option<Vec<Stmt>> },
    SpecBlock { forall: String },
    ExprStmt(Expr),
}

pub enum Expr {
    Var(String),
    Literal(String),
    Call { callee: String, args: Vec<Expr> },
    Binary { op: String, lhs: Box<Expr>, rhs: Box<Expr> },
    Cast { expr: Box<Expr>, ty: String },
    Tainted { ty: String, inner: Box<Expr> },
    Symbolic { ty: String },
    Assume(Box<Expr>),
    Assert(Box<Expr>),
    Declassify { inner: Box<Expr>, policy: Option<String>, reason: Option<String> },
    TaintSource { label: String },
    UnifiedBuffer { ty: String },
    RawPtr { mutable: bool },
    Other(String),
}

pub enum Mode { Safe, Research, Exploit }
```

- Spans attached on most nodes (some middle paths drop to Option<(usize,usize)>).
- No struct/enum item variants yet in AST (PLANNED for this slice or later).

## 4. HIR Definitions (middle/mod.rs)

Simplified evidence-oriented HIR (not full typed tree yet):

```rust
pub struct Hir {
    pub imports: Vec<String>,
    pub modules: Vec<String>,
    pub functions: Vec<HirFunction>,
}

pub struct HirFunction {
    pub name: String, pub module: Option<String>, pub mode: String,
    pub params: Vec<BindingInfo>, pub symbols: Vec<BindingInfo>,
    pub effects: Vec<String>, pub span: Option<(usize, usize)>,
}

pub struct BindingInfo { ... tainted, taint_source, declassified, ty, span ... }
```

Produced from AST during typecheck. Used for evidence manifests and sidecars.

## 5. MIR Definitions (middle + evidence)

- `MirBlock { function, mode, statement_count, effects }`
- `TypedIR` (carries the "MIR-like" after lowering):
  - mode, statements, constraints, taint_labels, taint_traces, symbolic_defs, symbolic_widths, solver_checks, diagnostics.
- Lowering happens in `typecheck` + `TaintPass::apply` + `SymbolicEngine`.
- Evidence bundles capture MIR-ish artifacts (solver.json, taint-traces.json, etc.).

## 6. Type Checker Files / Functions

- `compiler/src/middle/mod.rs`:
  - `pub fn typecheck(ast: AST, mode: Mode) -> Result<TypedIR, String>`
  - `analyze_function`, `analyze_stmts`
  - `expr_taint_source`, `declassify_source`, `is_tainted_type`
  - `TaintPass::apply(typed) -> TypedIR`
  - `SymbolicEngine::generate_constraints(source) -> Vec<SolverObligation>`
  - `check_obligations`, `replay_counterexample` (real model-substitution re-check)
- Produces `SemanticDiagnostic { message, span: Option<(usize,usize)> }`
- Enforces: taint in Safe mode requires declassify with policy+reason (Gate 4/5), bool conditions, numeric arithmetic, etc.
- No full Result/Option error handling in surface language yet.

## 7. Diagnostics / Span Handling

- Parse: `ParseDiagnostic { message, span: Span }` (byte offsets); collected in `ParseOutput`.
- Semantic: `SemanticDiagnostic { message, span: Option<(usize,usize)> }` attached to `TypedIR`.
- CLI/evidence: surfaces in bundles (checks.sarif, logs, manifest).
- Current: many paths have file/line from source, column not always computed (byte span only). Error codes (ANUBIS_*) not yet standardized in this slice start.
- Recovery: parser continues on many errors (detailed path); main parse_source can return Err.

## 8. Current Supported Syntax (real, exercised by examples + tests)

- Comments: `//` line comments (stripped in lexer).
- Functions: `fn name(params) { ... }`; typed params in some paths (`x: u32`); return types partial (inferred or declared in params).
- Bindings: `let x = expr;`, `let x: u32 = expr;`, `let x: tainted<u32> = ...`, `let secret: u32 = symbolic("secret");`.
- Primitives (seen): `u32`, `u8` (in symbolic overflow), bool via comparisons, strings (literals + taint labels).
- Literals: integer, string (`"..."`), bool implied.
- Expr: var refs, integer/string literals, `+ - *`, `== != < <= > >=`, `&` (bitwise), parenthesized (via precedence), calls.
- Control: `if cond { } else { }` (parsed).
- Special forms (first-class in AST/lower):
  - `symbolic("name")` → Symbolic expr
  - `assume(e)`, `assert(e)`
  - `taint_source("label")`
  - `declassify(v [, policy, reason])`
  - `sink(...)` (recognized in taint analysis)
- Blocks/modes: `research { }`, `exploit { }`, `hybrid { gpu... cpu... prove... }`, `spec { forall ... }`.
- Attributes/modes: inferred from blocks or first fn; @safe/@research/@proof/@audit/@effect surface via Mode + effects vec (parse preserves intent strings). Full attribute syntax (`@safe fn ...`) may be keyword/block based currently.
- Modules/imports: parsed (Import/Module items) with recovery; limited resolution.
- Evidence lowering: taint traces, solver obligations, declass policy recorded in TypedIR and bundles.

## 9. Current Unsupported / Partial / PLANNED (as of map time)

- Full struct decl / struct literal / field access (not in AST/Item/Expr yet).
- Enums/tagged unions: not present (PLANNED).
- while / for / loops: If present in parser? Limited; many examples avoid; while/for classified PLANNED or partial.
- u16 / u64 / full integer width matrix: u32 dominant; u8 in tests; others partial.
- String / str type + len, hash_sha256 builtins: string literals exist for labels; general string ops + stdlib surface minimal.
- User-defined function calls with arity/return checking strong: basic calls work; full resolution + multiple fns limited.
- True attributes `@safe` etc as decorators (vs block keywords): parse may recognize in older .anubis but enforcement via Mode.
- Modules/imports: parsed but no real name resolution or stdlib import story (PLANNED for later).
- Error handling (Result<T,E>): none in language surface (PLANNED).
- Block comments `/* */`.
- Full column-accurate diagnostics (byte spans only in many places).
- Large stdlib, async, etc. (out of scope).

See `docs/language/UNSUPPORTED.md` (to be created/updated) and MATURITY_CLAIM_MATRIX.md.

## 10. Current Examples

- examples/hello.anb, safe_hello.anubis, taint_reject.anb, taint_declassify.anb, declassify_missing_policy.anb
- symbolic_assert_pass/fail.anb, symbolic_bitmask_*.anb, symbolic_overflow_u8.anb
- risc0_receipt.anb, metal_parity_*.anb
- research_poc.anubis, hybrid_stub.anubis

~15 real .anb exercising taint (G4/5), solver (G7), risc0 (G10), metal parity (G11).

## 11. Current Tests

- All in `compiler/src/lib.rs` (unit tests, no separate integration crate yet).
- Drive `parse_source`, `parse_source_detailed`, `typecheck`, `TaintPass`, `SymbolicEngine`, `lower_to_native`, `build_evidence_bundle`, `validate_bundle`, Gate11 verdict pure fn, RISC0 paths.
- ~35 tests total (post fmt); many language-core: parses_*, taint_*, solver_*, risc0_*, evidence_*, hybrid_*.
- No dedicated `tests/fixtures/language_core/` with 25 EXPECT: fixtures yet (this slice adds them).
- Direct source-driven (include_str! or literals) — good for purity.

## 12. How Special Forms Lower (safe, research, proof, taint, declassify, symbolic, assume, assert)

- **Mode / @safe/@research/@proof/@audit/@effect**: `Mode` enum on Fn/Item; `infer_mode` from blocks; effects collected into HirFunction + TypedIR. Research/Exploit open more (raw ptrs, tainted flows). Safe enforces taint/declass (Gate 4/5).
- **Taint**: `TaintPass::apply` walks let/init, marks `tainted`, builds `TaintTrace` (source -> sink steps, declassified flag). Safe mode rejects tainted->sink without declass.
- **Declassify**: special Expr; requires policy + reason strings in safe paths (checked in typecheck + taint analysis). Traces record "declassify ->".
- **Symbolic / assume / assert**: `Symbolic` exprs, `Assume`/`Assert` stmts → `symbolic_defs`, `constraints`, `SolverObligation`. `SymbolicEngine::generate_constraints`. Solver checks (Z3 in some paths) produce status/model in TypedIR.solver_checks. assert fail → counterexample in evidence.
- **RISC0 / prove**: via `Prove` CLI + backends; produces receipt + image_id + journal.bin (verified outside r0vm in Gate 10/11 paths).
- **Metal parity**: separate lane outputs (cpu vs metal-hybrid) + journals extracted via verify-receipt; Gate11 sealer + pure `gate11_fixture_verdict`.
- Evidence: `build_evidence_bundle` + sidecars (solver.json, taint-traces.json, manifest with verdict, hashes) + tamper checks.

All lowering is source-driven from real AST; no hard-coded bypasses for sealed gates.

## 13. Known Brittle Areas

- Span fidelity drops (Option in middle; column not computed).
- Parser recovery is ad-hoc; some malformed still produce partial AST that typecheck may accept or crash downstream.
- No standardized ANUBIS_* error codes yet (messages only).
- Structs / richer types / modules not wired into typecheck or taint.
- "run" / interpreter path thin or missing; build always emits native in current CLI.
- Reproducibility: bundles contain timestamps; AST/HIR JSON emission not yet guaranteed deterministic in all paths.
- u16/u64/string support spotty outside specific tests.
- CLI "anubis check" exists and supports --evidence; "prove --backend risc0" exists; "doctor" exists; full ordinary workflow polish needed.
- Some tests use temp dirs / real toolchains (RISC0/Metal) — env sensitive (but A15/Gate11 handled with require-metal + real journals).

## 14. What Must Be Added for Gate 2 / Gate 3 (this slice)

- 25 canonical fixtures in `tests/fixtures/language_core/` (20 PASS + 5 FAIL) with `// EXPECT: PASS/FAIL` + `// FEATURE:` / `// ERROR_CONTAINS:` headers. Exercise every required syntax + sealed paths.
- Parser: never panics on bad input for fixtures; always emit spans + clean diag (file/line/column); unsupported syntax → diagnostic not crash.
- AST/HIR/MIR JSON emission on `check --emit ast,hir,mir` or `--evidence`.
- Type checker: full coverage of listed errors with codes (ANUBIS_UNKNOWN_VARIABLE, ANUBIS_TYPE_MISMATCH, ANUBIS_WRONG_ARITY, ANUBIS_INVALID_CONDITION_TYPE, ANUBIS_UNKNOWN_FIELD, ANUBIS_MISSING_RETURN); duplicate let defined; arithmetic numeric only; comparison → bool; struct field checks (once structs added).
- Structs: decl, literal, field access (minimal).
- Control: ensure while/for or document PLANNED + provide if/else + return.
- CLI: ordinary `check`/`build`/`run` (or documented), `prove --backend risc0`, `verify-*`, `doctor` (Rust + RISC0 + metal + evidence + git); docs/CLI.md.
- scripts/run_language_fixtures.sh + repro_language_core.sh + golden Rust tests under `cargo test -p anubis-compiler (language|parser|type)`.
- docs/language/{SPEC,GRAMMAR,CORE_FEATURES,UNSUPPORTED, STDLIB_CORE}.md .
- Update all claim matrices honestly (language core likely PARTIAL; sealed gates YES).
- Full A15 reproduction block passing with verdicts.
- Zero regression on sealed commands (taint_reject, declassify with policy, symbolic assert pass/fail, risc0 prove+verify, metal parity jq + verify_bundle).
- `cargo fmt --check`, all tests, clippy -D warnings remain green.
- No LSP, no large stdlib, no async, no publishing.

**Sealed gates must stay green throughout.** Any change to taint/declass/solver/RISC0/Metal/evidence paths must be additive only and re-verified with the exact regression commands.

This map is the source of truth for "what existed at start of slice." Update only with new measured reality after changes.
