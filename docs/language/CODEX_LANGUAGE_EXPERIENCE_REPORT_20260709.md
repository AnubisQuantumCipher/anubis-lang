# Field Report: Learning and Using Anubis

**Date:** 2026-07-09
**Scope:** current checkout of `a-plus-maturity/20260705-1649`
**Truth rule:** claims below distinguish fresh commands from project aspirations. This report did not run the RISC0 prove or Metal lanes.

## Executive verdict

I find Anubis genuinely interesting. It feels less like a conventional new language and more like an **evidence-native systems-language workbench**: a compact Rust-flavored language surrounded by taint analysis, policy boundaries, generated artifacts, and a separate proof path.

The ordinary executable core is already pleasant for small algorithms and policy-shaped programs. It is not yet a mature general-purpose language ecosystem: the native runner transpiles through Rust, the runtime uses dynamic `AnubisValue` values, and the project still explicitly lists modules, packages, generics, async, LSP, and a broad standard library as partial or planned.

I would happily use it for a compact, auditable kernel—an engagement ledger, a validation rule, a bounded algorithm, or code intended to later enter a proof workflow. I would not yet choose it as the only language for a large application.

## The program I wrote

[`examples/codex_20260709_language_tour.anb`](../../examples/codex_20260709_language_tour.anb) is a safe data-review program. It:

- sums a list using a helper function and `for … in`;
- records expected and observed values in a map;
- selects a struct-like enum variant with an if-expression;
- extracts its public result using an exhaustive `match`;
- prints the computed total and public code.

Its final output was:

```text
42
46
```

The first run caught my own comment arithmetic error: I initially documented `45`, but a sum of `42` plus four samples is `46`. I corrected the source comment and reran both analysis and execution. That is a small but real example of why observed output matters more than a prose expectation.

## Fresh evidence from this session

| Action | Result |
|---|---|
| `bash tools/grok-safety-check.sh` | `safety-check: OK` |
| `cargo test --all` | PASS: 47 CLI tests and 94 compiler tests, no failures |
| `anubis check … --emit ast,hir,mir --evidence` | PASS, with AST/HIR/MIR and a hashed evidence bundle |
| `anubis verify <bundle>` | `bundle valid: true` |
| `anubis run … --evidence --json` | PASS, safe mode, exit code 0, output `42` and `46` |
| `anubis check examples/taint_reject.anb` | expected FAIL: `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` |

Final check evidence:

```text
out/codex_language_tour_20260709/final_check/evidence-20260709-181048-safe
```

Final run evidence:

```text
out/codex_language_tour_20260709/final_run
```

The run summary truth block says `ordinary_execution: true`, `proof_execution_claimed: false`, and `receipt_verified: false`. That distinction is exactly right: this was a real native execution, not a claim of ZK proof generation.

## My working mental model

The familiar surface is intentional: `fn`, `let`, braces, semicolons, `if`, `for`, maps, lists, enums, and `match` make the language quick to read. For the normal runner, Anubis parses and type-checks the whole program, lowers it to a self-contained Rust source file, invokes `rustc`, then runs the resulting executable. The emitted runtime is an `AnubisValue` model rather than a direct mapping of every Anubis annotation into a Rust static type.

That makes Anubis feel like a practical compiler experiment, not a toy parser. My program used enough real structure—function calls, iteration, a dictionary, ADTs, pattern matching, and a branch—to make the experience meaningful rather than a hello-world demo.

## What felt strong

- **Readable core syntax.** I could write the program in a direct, Rust/Zig-adjacent style without fighting the grammar.
- **Real safe execution.** The program was checked, transpiled, compiled, and executed; the generated Rust source and native executable are retained as artifacts.
- **Evidence as a first-class output.** Check output included AST/HIR/MIR, SARIF, source snapshots, hashes, and a verifier. The run output explicitly limits its own claim rather than implying that every successful execution is a proof.
- **Security boundary behavior.** The independent taint fixture failed closed in safe mode with an actionable diagnostic instead of silently accepting a tainted sink flow.

## What felt rough or unfinished

- **Iteration cost.** A normal run generates Rust and invokes `rustc`, which is transparent and trustworthy for a prototype but heavier than an interpreter or persistent compiler daemon.
- **Multiple semantic lanes.** `check`, ordinary `run`, RISC0 proof generation, and Metal/hybrid work have to remain behaviorally aligned. That is the project’s main engineering challenge, not adding another keyword.
- **Tooling and ecosystem are early.** The project itself calls multi-file resolution, packages, generics, async, LSP, and a large standard library partial or planned.
- **Documentation needs consolidation.** `README.md` describes a v0.2 backend while the CLI reports version 0.1.0; `docs/CLI.md` describes a much narrower `run` subset than this session and the current backend test suite demonstrated. Fresh commands should remain the source of truth until those documents converge.
- **Maturity claims need care.** The acceptance criteria still mark Gate 10 partial, and `scripts/audit_a_plus.sh` says the remaining gates are TODO. I therefore would not call the whole project A+ or production-ready from this study.

## How it feels

Anubis feels like a **sharp research instrument with an unusually good chain of custody**. The syntax is approachable; the distinctive personality comes from its insistence that security policy, taint flow, proof boundaries, and evidence should be visible in the programming workflow.

That is much more compelling to me than a language that merely imitates Rust syntax. It has a credible niche: small, mission-shaped systems programs where the reader should be able to inspect both the source and the evidence around it. The next leap is not more ambition—it is making the semantics, docs, and reproducibility story equally crisp across every backend.

## Reproduce my result

```bash
cargo test --all
cargo run -p anubis -- check examples/codex_20260709_language_tour.anb \
  --emit ast,hir,mir --evidence \
  --out out/codex_language_tour_20260709/final_check
target/debug/anubis verify \
  out/codex_language_tour_20260709/final_check/evidence-20260709-181048-safe
target/debug/anubis run examples/codex_20260709_language_tour.anb \
  --evidence --json --out out/codex_language_tour_20260709/final_run
cat out/codex_language_tour_20260709/final_run/stdout.txt
```
