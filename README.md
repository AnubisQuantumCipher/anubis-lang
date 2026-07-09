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
- `anubis capabilities --apple-native --json` exposes the Apple-native capability matrix, including RISC0/Metal, UMPG runtime-plan status, and advisory-only Neural Engine boundaries
- `anubis runtime-probe --json` emits host/toolchain/RISC0/Metal capability evidence without claiming proof execution
- `anubis runtime-plan --backend risc0 --lane metal-hybrid --apple-native` emits a source-derived UMPG-style operation DAG pointing proof/computation planning at `/Users/sicarii/Desktop/metal-hybrid-prover`
- `anubis run examples/hello_normal.anb` executes ordinary safe Anubis code through the current native safe subset
- **Bounty-grade PoC kit** (authorized local lab): packing (`p64`/`cyclic`), `target_run` process harness, mutation fuzz against local binaries, crash evidence — see `docs/language/POC_KIT.md`
- **Offensive Platform (AOP)**: engagement-scoped C2, agents, RBAC, hash-chained **action receipts** — see `docs/language/OFFENSIVE_PLATFORM.md`
- **Proof-native language**: program-bound RISC0 guests, parameterized inputs, named journals, `proof_assert` / `proof_commit_*` — private witnesses stay off the journal
- **Turing-complete executable core** (loops, mutation, recursion) with fixture gate
- Excellent direct library entrypoints for agents/tests
- Self-audit + reproducibility first-class (`bash scripts/run_power_gate.sh`)

See `docs/spec.md`, `docs/APPLE_NATIVE.md`, `docs/language/POC_KIT.md`, `docs/adr/`, `examples/`, and run `cargo test -p anubis-compiler`.

## Quick Start (on this machine)
```bash
cd anubis-lang
cargo build
cargo run -- --help
cargo run -- build examples/research_poc.anubis --bounty
cargo run -- verify <the-evidence-dir>
cargo run -- report <the-evidence-dir>
cargo run -- doctor --json
cargo run -- capabilities --apple-native --json
cargo run -- runtime-probe --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover --json
cargo run -- runtime-plan examples/risc0_receipt.anb --backend risc0 --lane metal-hybrid --apple-native --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover --json
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
./target/release/anubis prove examples/proof/proof_factorial_input.anb \
  --backend risc0 --lane cpu \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --input-json '{"n":5}' --evidence --out out/proof_factorial_5
# journal = 120; use '{"n":6}' → journal 720 (same ImageID)
bash scripts/run_parameterized_proof_gate.sh --out out/parameterized_proof
```

Current source gates: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`, and fresh CLI build/verify flows are the acceptance checks. PoC kit: `bash scripts/run_poc_kit_gate.sh`.

This is still an early language, but it now has both an ordinary safe execution path and a proof/evidence planning path. Remaining work is runtime-exec enforcement, language depth, richer semantics, broader taint modeling, package tooling, stress matrices, and release hygiene.

v0.1 MVP hardened: real AST lowering for POC, typed HIR/MIR records, semantic taint traces, real Z3 counterexamples, reference-grade evidence bundle files, IR-shaped safe native emission (no host crash), AST-driven mode, and tests that assert structure on shipped parser output.

Built with maximum rigor for the operator.
