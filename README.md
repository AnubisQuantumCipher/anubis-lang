# Anubis

**Anubis** is being built as an evidence-native dual-use systems language for professional bug bounty hunters (Sicarii) and builders of sovereign, high-assurance systems.

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
- Excellent direct library entrypoints for agents/tests
- Self-audit + reproducibility first-class

See `docs/spec.md`, `docs/adr/`, `examples/`, and run `cargo test -p anubis-compiler`.

## Quick Start (on this machine)
```bash
cd anubis-lang
cargo build
cargo run -- --help
cargo run -- build examples/research_poc.anubis --bounty
cargo run -- verify <the-evidence-dir>
cargo run -- report <the-evidence-dir>
cargo run -- doctor --json
```

Current source gates: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`, and fresh CLI build/verify flows are the acceptance checks.

This is still an early language, but the hard Metal/RISC0 backend tranche is now wired end-to-end rather than represented by a host-only scaffold. Remaining work is language depth: richer grammar, fuller semantics, broader taint modeling, package tooling, stress matrices, and release hygiene.

v0.1 MVP hardened: real AST lowering for POC, typed HIR/MIR records, semantic taint traces, real Z3 counterexamples, reference-grade evidence bundle files, IR-shaped safe native emission (no host crash), AST-driven mode, and tests that assert structure on shipped parser output.

Built with maximum rigor for the operator.
