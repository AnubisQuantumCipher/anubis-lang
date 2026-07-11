# Anubis A+ Acceptance Criteria

Anubis is **A+ only** when **all mandatory gates** below pass with reproducible artifacts and A15 hostile audit produces no mandatory failures.

## GATE 1 — Clean build
```bash
cargo fmt --check
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo build --release
```
Evidence: command output + release binary.

## GATE 2 — Real language core (this slice: minimum viable per plan)
This slice (2026-07-06) completed the first serious pass of the *defined minimum*:
- comments (//)
- fn main + typed params + return (as call expr)
- let x = ; let x: u32 = ;
- primitives: bool, u8, u32 (u16/u64/string partial/where feasible)
- expr: literals, var, + - * , == != < <= > >= , & , parens
- control: if/else ; while (or PLANNED skip)
- structs: decl + lit + field access
- calls: user fn + builtins (symbolic, assume, assert, taint_source, declassify, sink)
- attrs parsed/preserved: @safe @research @proof @audit @effect(...)
- modules/imports, enums, full Result, large stdlib: PLANNED / out of slice

Evidence: 25 canonical fixtures (tests/fixtures/language_core/*.anb with EXPECT headers), runner, reports, A15 repro with consistent verdicts.

Do not claim "general-purpose language complete". See MATURITY_CLAIM_MATRIX + docs/language/UNSUPPORTED.md .

## GATE 3 — Parser/HIR/MIR
≥20 fixture programs:
- parse succeeds/fails correctly
- AST/HIR/MIR emitted with exact source spans
- diagnostics show exact source spans
- no panic on malformed source

## GATE 4 — Safe taint hard enforcement
This **must fail in safe mode** (exact diagnostic, SARIF finding, evidence rejected-flow trace, no native artifact unless metadata-only failure record):
```anubis
fn main() {
    let secret = taint_source("password");
    sink(secret);
}
```

## GATE 5 — Declassification policy
Passes **only** with explicit policy + reason. Trace, evidence, SARIF clean for approved path.
```anubis
fn main() {
    let secret = taint_source("token");
    let public = declassify(secret, policy: "hash-only", reason: "bug-bounty proof redaction");
    sink(public);
}
```

## GATE 6 — Research boundary
- Research mode requires explicit opt-in.
- Unsafe memory, raw pointers, exploit/PoC, shell, network, sensitive file reads rejected outside approved boundaries.
- Evidence records research mode + reason when used.

## GATE 7 — Solver correctness
Every symbolic assertion:
- SMT encodes actual program relationships
- counterexamples execute or are replayable against source semantics
- no free variables pretending to be program values
- bitvector widths and overflow explicit

## GATE 8 — Evidence schema
Every build (success or fail) produces bundle with:
- manifest.json
- MANIFEST.sha256
- source copy + hash
- command log + build log
- compiler version/commit
- environment JSON
- AST/HIR/MIR (when applicable)
- taint traces
- solver output
- SARIF
- artifact hash
- backend sidecars
- human report
- verification script

## GATE 9 — Tamper check
- Fresh bundle: verify true
- After modifying any hashed file: verify false

## GATE 10 — RISC0 receipt (end-to-end)
Hardened state (2026-07):
- real ImageID derived from actual guest ELF via risc0-build (no FRESH/NO_REAL placeholder)
- real RISC0 Receipt::verify(image_id) API call wired + documented + rejects bad IDs
- all required sidecars (guest.elf, image_id.txt, receipt.bin, metadata, logs, guest source) hashed and covered by strict tamper detection
- dev/mock/cache flags explicit and false for YES claims
- A15 reproduction with real ID + tamper results
- Verdict: PARTIAL (API + ID + tamper strong; full passing cryptographic receipt still limited in current hybrid emit/prove path)

## GATE 11 — Metal parity
≥3 deterministic kernels/workloads:
- CPU output == Metal output
- fallback works when Metal disabled
- evidence records backend used
- benchmarks disclose hardware + variance

## GATE 12 — Reproducibility
Two clean builds of same source produce identical **core** source/artifact hashes.
Nondeterminism (timestamps etc.) isolated and documented.

## GATE 13 — CI
GitHub Actions or local equivalent covering:
- fmt + clippy + tests
- evidence validation smoke
- taint rejection smoke
- SARIF JSON validation
- optional Metal/RISC0 jobs (guarded)

## GATE 14 — Docs honesty
Docs clearly separate: implemented / partial / experimental / planned / unsupported.
No unbacked hype.

## GATE 15 — Final hostile audit
A15 (red team) produces adversarial audit with final grade.
A+ requires **no mandatory gate failures**.

---

Run `bash scripts/audit_a_plus.sh` to execute the full sealed gate suite. It runs the repo safety check and delegates to the canonical runner `scripts/audit_unified.sh`, which executes every gate (G1–G15) and writes an honest PASS/FAIL/SKIP verdict plus `gate_report.json` to `out/unified_gate/<STAMP>/`, exiting non-zero if any gate fails.
