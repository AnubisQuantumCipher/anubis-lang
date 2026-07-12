# anubis-in-anubis Dogfood — Execution Roadmap

**Goal:** rewrite `selfhost/src/anubis_sh.anb` to *use* enums, `match`, and if-expressions
in **load-bearing** positions, verified by the byte-identical `stage2.rs == stage3.rs`
source fixpoint + same-toolchain binary fixpoint + a new fail-closed **dogfood/ablation
gate** — more rigorous than Zig's prose-asserted `zig3≡zig4` and Dafny's never-closed
self-host loop.

Grounded against how Rust / Go / Zig / OCaml / Dafny actually bootstrapped (research
2026-07-12). Key external lessons applied:
- **Rust**: stage2==stage3 is the right idea but shipped as an *optional, soft* sanity
  check. → We make it a **mandatory fail-closed gate**.
- **Zig**: `zig3≡zig4` is prose, never diffed in-build; trust root is a committed
  `zig1.wasm` blob. → We **diff it in-build**; our seed is source (Rust host + rustc).
- **Dafny**: substitutes semantic verification but **never closes** the self-host loop.
  → We close the loop *and* run a differential oracle.
- **Thompson "trusting trust" / Wheeler DDC**: a self-host fixpoint does NOT prove seed
  integrity. → We label this an explicit `NEEDS-HUMAN` residual, never claim
  "backdoor-free". (DDC = future work.)

## Strategy A (frozen wire format)
Keep the emitted JSON contract (`{"kind":"Let",…}`) **unchanged**; represent the
compiler's *internal* values as enums and `match` on them. Golden AST/token fixtures are
the byte-level net. No feature addition needed — enums/match/if-expr already execute in
both the host and the emitted interpreter.

## Tiers (each: FAST loop → SEAL only at tier end)
- **Tier 0 — runtime hardening + performance (DONE 2026-07-12).** `anubis_sh_interp_rt.rs` only.
  - H1: fail-closed no-match `panic!("ANUBIS_SH_MATCH_UNMATCHED")` (was silent `V::Null`),
    aligning the interpreter with the host's `ANUBIS_MATCH_UNMATCHED`.
  - H2: run entry on a large-stack thread (256 MiB) — enum recursion + match→helper depth.
  - **Performance (the real blocker): the interpreter was O(n²) and self-compile took
    ~14 min, ballooning to >45 min under the dogfood.** Root cause: values passed by deep
    clone (the 236 KB source string + 30k-token list cloned on every call). Fix: `Rc`-wrap
    `Str`/`List`/`Map` (O(1) clone, copy-on-write via `Rc::make_mut`), byte-index string
    builtins (ASCII), O(1) in-place `push` and `x = x + s` append, and an `Rc` function
    table. **Result: lex 85s→0.67s, check >180s→0.52s, compile >600s→1.15s** — ~100–500×,
    with all 7 enum/match oracle examples still byte-identical to the host. This makes the
    ~1s dev loop (recompile the evolving `anubis_sh.anb` with the fast stage1) and a
    practical seal possible.
  - Re-establish fixpoint once (runtime bytes changed); record new `binary_fixpoint.sha256`.
- **Tier 1 — if/else → if-expressions (DONE).** `jbool`, `json_escape`, `prec_of`, lexer
  char→kind table.
- **Tier 2 — SKIPPED (redundant with Tier 3).** `jstmt`/`check_stmts` become `match` in
  Tier 3, so an intermediate if-expr pass would be wasted work.
- **Tier 3 — `enum Stmt` + match (DONE, sealed 9/9, fixpoint `949bd378…`).** 7 struct
  variants; constructed in `parse_stmt`, read via `match` in `jstmt` (codegen) +
  `check_stmt` (checker). Gotcha found & fixed: helpers get `env` by value (Anubis passes
  maps by value), so binds must be threaded back — `check_stmt` returns `[env, diags]`.
- **Tier 4 — `enum Expr` + match (DONE, fastloop-verified).** 15 variants: tuple shapes
  for atoms (`Int`/`Str`/`Bool`/`Var`), struct for compound; `parse_prefix`/`parse_expr`/
  `parse_enum_init`/`parse_if_expr`/`parse_match_expr` construct, `jexpr`/`check_expr`
  read via `match`. `check_expr` reads env only (no threading). The whole AST is now an ADT.

**Deferred:** token-kind→enum (high churn, low value), parse-result→enum (blocked: no
`?`, no tuple-let, single-expr arms), literal/string match patterns (nicety, not needed
for enum dogfood; host already supports them so parity stays checkable if added later).

## Determinism invariants the rewrite must not break
- **INV-1** no map-key iteration on any output path (host insertion-order vs interp
  `BTreeMap`); serialize by literal key, iterate Lists by index only.
- **INV-2** hardcoded emission field order (never from runtime map layout).
- **INV-7** self-clean: `check anubis_sh.anb` prints "check passed", zero diagnostics.
- **INV-15** every dogfood `match` is exhaustive or ends in `_`; use pattern binds **only
  inside** the arm body (host scopes them; interp leaks → reading after = divergence).
- **INV-16** once a node is an enum, ALL readers `match` it, never index it — same commit.
- **INV-17** never `==`/`!=` payloaded enum values (interp equality is display-based); compare by `match`.
- **INV-18** no material recursion deepening without H2.

## Dogfood gate (`scripts/run_selfhost_dogfood_gate.sh`, fail-closed)
- **G1 structural (AST-based, not grep):** parse `anubis_sh.anb` with the compiler itself;
  assert ≥1 top-level `Enum`, a load-bearing `Match` (enum arms w/ non-empty binds used in
  the body) inside named codegen functions, ≥K `IfExpr` in codegen-path functions.
- **G2 semantic:** `run_selfhost_gate.sh` (seal + binary fixpoint) and
  `run_selfhost_fulllang_gate.sh` green with the dogfooded source.
- **G3 ablation (the differentiator):** mechanically neuter each flagged construct on a
  throwaway copy, rebuild, assert the fixpoint OR oracle now FAILS. Load-bearing ⟺ removal
  breaks the output. No incumbent ships this.

## Honest claims earned / residuals never claimed
Earned: authored-in-Anubis parse/check/codegen using enums+match+if-expr load-bearingly;
byte-identical source+binary fixpoint as a **mandatory** gate; mechanically-proven
load-bearing (ablation); differential oracle; source-auditable trust root.
Never claim: trusting-trust-closed (no DDC — `NEEDS-HUMAN`); compiler correctness
(fixpoint = determinism, not correctness); "whole compiler in Anubis" (the tree-walking
runtime is a fixed hand-written Rust seed); cross-rustc-version binary identity; general
host/interp semantic identity (only over the map-iteration-free corpus).

Status: plan approved 2026-07-12; executing Tier 0 first.
