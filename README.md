# Anubis

**Anubis** is being built as an evidence-native dual-use systems language for professional bug bounty hunters (Sicarii) and builders of sovereign, high-assurance systems. The core bet: **a green `anubis check` must never certify a contract the runtime `anubis run` violates** — soundness is the product, and every claim below is checkable, not just asserted.

## The solver finds the bug you didn't know you had

Every language has contracts. What Anubis does that a type system cannot: it hands you the **exact program state** that breaks your invariant — before you ship.

A ring buffer's slots-in-use is `tail - head`. Correct in mathematics; a bug in fixed-width code — when the buffer has wrapped and `tail < head`, the count goes negative. So you say a count is never negative:

```rust
fn ring_used(head: u32, tail: u32) -> u32
    ensures(result >= 0)
{
    return tail - head;
}
```

`anubis check` doesn't shrug and say "unproven". It **disproves** the claim with a counterexample — the wraparound state your tests never hit:

```
$ anubis check examples/showcase/ring_buffer_underflow.anb
ANUBIS_ASSERTION_UNPROVEN: ensures:(bvsge (bvsub anb_tail anb_head) 0)
  counterexample:
    anb_head = 0x00000000c0000000     # 3_221_225_472
    anb_tail = 0x0000000000000000     # 0   →  tail - head is negative
```

Fix it — subtract only where it can't underflow — and the **same solver proves the fix correct** (`ring_used_fixed` in that file: `ensures(result >= 0)` discharged on both branches). That counterexample is worth more than a thousand type annotations. Run it yourself: [`examples/showcase/ring_buffer_underflow.anb`](examples/showcase/ring_buffer_underflow.anb).

> Honest boundary: Anubis's `u32` is a bounded 64-bit integer with **signed** arithmetic, so the failure it proves is "the count goes below zero", not "wraps to 4 billion" — the bug is the same, stated in the language's real semantics. Nothing here is dramatized past what the solver actually decides.

## Where Anubis is — honest phase status

Anubis follows an 11-phase maturity arc; the canonical source of truth is
[`docs/language/ROADMAP.md`](docs/language/ROADMAP.md). Status uses a graded vocabulary —
**REAL** (implemented **and** gated), **PARTIAL** (real slices landed, honest boundaries, fails
closed on the rest), **PLANNED**. Nothing here is marked done that isn't sealed.

| Phase | State | What that means today |
|---|---|---|
| 0 — Trust spine | ✅ REAL | reproducible build + self-host bootstrap + byte-identical fixpoint seal (`dc680001`) |
| 1 — Real type system | ✅ REAL | bidirectional inference, captured generics, traits + coherence — all enforcing |
| 2 — Capability & effect | ✅ REAL | transitive effect inference, linear capability tokens, and the **lethal trifecta as a compile error** |
| 3 — Broaden verified surface | 🟡 PARTIAL (converging) | Z3 contract lanes for int / float / string / bounded arrays / loop invariants / struct fields; **every case outside the modeled fragment fails closed** |
| 4 — Port checker into Anubis | ⬜ PLANNED | deliberately deferred until 1–3 settle (each port would reseal the fixpoint) |
| 5 — Mechanized soundness | 🟡 PARTIAL (live) | a **Lean 4 formal gate** — 72 machine-checked theorems: SMT-encoding soundness (the checker's bit-vector terms = the runtime's `i64` semantics), plus Safe-mode **non-interference** and **effect soundness** over a core calculus. Verify it yourself: `bash scripts/run_formal_gate.sh` |
| 6 — Proof-carrying packages | 🟡 PARTIAL | signed evidence bundles — source Merkle root, effect/taint summaries, receipts (`compiler/src/package/`) |
| 7 — Minimize TCB | ⬜ PLANNED | a second **independently-authored** parser + backend — closing author-diversity needs a second human |
| 8 — Developer experience | 🟡 PARTIAL | LSP, formatter, REPL, doc-gen, tree-sitter grammar, tutorial, spec |
| 9 — External reproduction | 🟡 PARTIAL | reproducibility + differential-compiler gates exist; independent-stranger reproduction is the pending step |
| 10 — Production 1.0 | ⬜ PLANNED | ship in ≥2 domains + a frozen, semver'd 1.0 spec |

**Watch it happen.** This repo is a live, minute-by-minute public record: every commit lands on the
`a-plus-maturity/20260705-1649` branch within seconds of being made. The discipline is auditable, not
advertised — `bash scripts/run_formal_gate.sh` machine-checks the Lean proofs, and every solver slice
is sealed against a byte-identical self-host fixpoint before it is allowed to commit.

## v0.2 Backend Status
- Dual safe (default) / research/exploit modes with intent annotations
- `tainted<T>` + automatic propagation + `symbolic`/`assume`/`assert` with SMT path constraints
- `hybrid { gpu(metal) {} cpu {} prove(...) {} }` + `spec { forall }` + `unified Buffer<T>`
- Full `--full-hybrid` lowering emits the reference RISC0 workspace shape with vendored patched `risc0-circuit-rv32im`, generated guest ELF/image ID, stock `receipt.verify(ANUBIS_ID)`, Metal lane reporting, CPU fallback, and `ANUBIS_REQUIRE_METAL=1` fail-closed mode
- `import` and `module` items with span-aware parsing and recoverable diagnostics
- Typed HIR/MIR records with symbol tables, mode/effect boundaries, raw-pointer checks, taint traces, and declassify-aware sink reporting
- Z3-backed assertion obligations with PASS/FAIL verdicts and counterexample models
- `anubis build --bounty` / `--evidence` : timestamped bundles with environment capture, source-tree manifest, HIR/MIR, taint traces, solver output, SARIF, Markdown report, hashes, logs, and artifacts
- Real native Apple Silicon executables emitted
- `anubis capabilities --apple-native --json` exposes the Apple-native capability matrix, including RISC0/Metal, UMPG runtime-plan status, and advisory-only Neural Engine boundaries
- `anubis runtime-probe --json` emits host/toolchain/RISC0/Metal capability evidence without claiming proof execution
- `anubis runtime-plan --backend risc0 --lane metal-hybrid --apple-native` emits a source-derived UMPG-style operation DAG for proof/computation planning (Metal reference resolved via `--metal-reference` / `ANUBIS_RISC0_METAL_REFERENCE` / `Anubis.toml`)
- `anubis run examples/hello_normal.anb` executes ordinary safe Anubis code through the current native safe subset
- **Bounty-grade PoC kit** (authorized local lab): packing (`p64`/`cyclic`), `target_run` process harness, mutation fuzz against local binaries, crash evidence — see `docs/language/POC_KIT.md`
- **Offensive Platform (AOP)**: engagement-scoped C2, agents, RBAC, hash-chained **action receipts** — see `docs/language/OFFENSIVE_PLATFORM.md`
- **Proof-native language**: program-bound RISC0 guests, parameterized inputs, named journals, `proof_assert` / `proof_commit_*` — private witnesses stay off the journal
- **Enums + `match`**: unit/tuple variants, pattern bindings, run + prove (`bash scripts/run_enum_match_gate.sh`)
- **`for x in list`** and `for i in a..b` — collection + range iteration (`bash scripts/run_for_in_gate.sh`)
- **Turing-complete executable core** (loops, mutation, recursion) with fixture gate
- Excellent direct library entrypoints for agents/tests
- Self-audit + reproducibility first-class (`bash scripts/run_power_gate.sh`)

See `docs/spec.md`, `docs/APPLE_NATIVE.md`, `docs/language/POC_KIT.md`, `docs/adr/`, `examples/`, and run `cargo test -p anubis-compiler`.

## Learn Anubis (developer experience — Phase 8)

Verification-first adoption path:

| Tool | Purpose |
|------|---------|
| `anubis doc` | API docs with a **Contracts** section from source `requires`/`ensures` |
| `anubis repl` | Check-first REPL (fast AST default; `--exact` = native `run` path) |
| `anubis lsp` | Diagnostics from typecheck + obligations; contract hovers |
| Editors | `editors/vscode-anubis` (TextMate + LSP), `editors/tree-sitter-anubis` |
| Tutorial | [`docs/language/TUTORIAL.md`](docs/language/TUTORIAL.md) |
| Reference | [`LANGUAGE.md`](LANGUAGE.md) · [`docs/language/SPEC.md`](docs/language/SPEC.md) |

```bash
cargo build --release -p anubis
./target/release/anubis doc tests/fixtures/dx/contracts_doc.anb   # look for ### Contracts
./target/release/anubis repl --eval '2 + 3'
bash scripts/run_dx_gate.sh out/dx_gate   # DX_GATE: PASS
```

### Self-hosting bootstrap (Anubis-SH — trust spine; porting the *checker* into it is Phase 4)

The compiler subset that compiles itself lives in `selfhost/`. The gate runs a **real bootstrap** (stage0 host → stage1 → stage2 → stage3, `cmp` stage2/stage3), not host×2:

```bash
bash scripts/run_selfhost_gate.sh out/selfhost_gate   # SELFHOST_GATE: PASS (N/N)
./target/release/anubis run selfhost/src/anubis_sh.anb --allow-research -- parse selfhost/corpus/ok_hello.anb
```

See `docs/language/SELFHOST.md` and `selfhost/SUBSET.md`.

## Quick Start
```bash
cd anubis-lang
cargo build
cargo run -- --help

# `check` is the primary verb: it runs the Z3 contract solver over every
# requires/ensures/assert and prints a real counterexample for any it cannot prove.
cargo run -- check examples/hello_normal.anb
# `build` is fail-closed by default — it runs the SAME verification before emitting
# an artifact and refuses on an unproven contract (use --no-verify to skip).
cargo run -- build examples/research_poc.anubis --bounty
cargo run -- verify <the-evidence-dir>
cargo run -- report <the-evidence-dir>
cargo run -- doctor --json
cargo run -- capabilities --apple-native --json
cargo run -- run examples/hello_normal.anb

# Bounty-grade local PoC kit
bash poc_kit/build_vuln.sh
./target/release/anubis run examples/security/poc_local_overflow.anb --allow-research
./target/release/anubis fuzz --target poc_kit/bin/vuln_local --runs 200 --out out/fuzz_vuln
bash scripts/run_poc_kit_gate.sh --out out/poc_kit

# Offensive platform (engagement-scoped lab C2)
./target/release/anubis engage-init --dir out/engagements/lab --authorization local-lab-charter
./target/release/anubis listen --engage out/engagements/lab   # terminal A
./target/release/anubis agent-generate --engage out/engagements/lab --name agent0
./target/release/anubis task-queue --engage out/engagements/lab --module whoami
bash scripts/run_offensive_platform_gate.sh --out out/offensive_gate

# Parameterized RISC0 proof: prove f(input)=output (journal depends on input; ImageID on program)
# Parameterized RISC0 proof (requires ANUBIS_RISC0_METAL_REFERENCE or --metal-reference)
./target/release/anubis prove examples/proof/proof_factorial_input.anb \
  --backend risc0 --lane cpu \
  --input-json '{"n":5}' --evidence --out out/proof_factorial_5
# journal = 120; use '{"n":6}' → journal 720 (same ImageID)
bash scripts/run_parameterized_proof_gate.sh --out out/parameterized_proof
```

Current source gates: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`, and fresh CLI build/verify flows are the acceptance checks. PoC kit: `bash scripts/run_poc_kit_gate.sh`.

This is still an early language, but it now has both an ordinary safe execution path and a proof/evidence planning path. Remaining work is runtime-exec enforcement, language depth, richer semantics, broader taint modeling, package tooling, stress matrices, and release hygiene.

v0.1 MVP hardened: real AST lowering for POC, typed HIR/MIR records, semantic taint traces, real Z3 counterexamples, reference-grade evidence bundle files, IR-shaped safe native emission (no host crash), AST-driven mode, and tests that assert structure on shipped parser output.

Built with maximum rigor for the operator.
