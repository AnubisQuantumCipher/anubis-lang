# Anubis Language Specification (v0.1 authoritative for MVP)

## Philosophy
Dual Mastery: elite for offensive research/POCs + elite for verifiable sovereign systems.
Sovereign by default. Judgment native. Hybrid power (CPU + Metal + prove). Agent synergistic. Responsible & reproducible.

## Core Syntax (MVP)
- Modern, Rust/Zig inspired, indentation or braces ok but braces explicit for blocks.
- `fn main() { ... }`
- Parser foundation now records byte spans for functions and let bindings, parses function parameters, preserves expression precedence for simple binary expressions, and reports recoverable diagnostics through `parse_source_detailed`.
- Parser accepts `import path.to.thing;` and nested `module name { ... }` items. The AST preserves imports/modules/functions separately so later package layout and LSP work do not need to infer structure from comments.
- `research { ... }` or `exploit { ... }` : full low-level power. Requires intent annotation in comments or `intent: "buffer overflow trigger for CVE-XXXX"` .
- `hybrid { gpu(metal) { ... } cpu { ... } prove(risc0) { ... } }`
- `spec { forall x : T . P(x) }`
- `let x: tainted<u8> = ...;`
- `let s = symbolic::<u32>(); assume(s < 100); assert(s > 0);`
- `unified Buffer<T>` for zero-copy hybrid memory.
- `declassify(x)` marks an explicit trust-boundary transition. Tainted values sent to known sinks without declassification are reportable traces and are rejected in safe mode.

## Modes
- Safe (default): no UB without annotation.
- Research/Exploit: raw pointers allowed inside annotated blocks; compiler may insert logging/guards.
- Safe raw pointer bindings such as `let p: *mut u8 = ...;` are rejected unless moved behind `research`/`exploit`.
- The middle layer emits typed HIR/MIR summaries with symbols, modes, effects, taint sources, sink traces, and solver obligations.

## Novel Features (first-class in v0.1)
- tainted<T> with propagation rules, sink traces, and declassify().
- symbolic, assume, assert + path constraint export to SMT and Z3-backed obligation checks with counterexample models.
- Evidence bundles via `anubis build --bounty`.
- `anubis report <bundle>` prints the Markdown bounty report.
- `anubis validate <bundle>` and `anubis verify <bundle>` validate hashes and PASS verdicts.
- `anubis doctor --json` emits toolchain readiness for automation.

## Backends (v0.1)
- Native (aarch64-apple-darwin via rustc emission).
- Hybrid fast host: generated Cargo project with real Metal dispatch, Tier-2 runtime probe, `R0_DISABLE_METAL` CPU fallback, `ANUBIS_REQUIRE_METAL=1` fail-closed mode, shared `StorageModeShared` buffer, and base-allocation guard.
- Hybrid full host: generated RISC0 workspace with `host` + `methods` crates, vendored patched `risc0-circuit-rv32im`, `risc0-build` guest ELF/image ID generation, `get_prover_server`/`ProverOpts`, stock `receipt.verify(ANUBIS_ID)`, journal assertion, CPU fallback, and Metal-required fail-closed mode.
- Evidence bundles capture the host artifact plus `guest.elf`, `image_id.txt`, and `generated-methods.rs` with hashes in `evidence.json`, `source-tree.json`, and `MANIFEST.sha256`.
- Still incomplete: broad workload stress/parity matrices, richer generated Metal kernels from source blocks, and release-grade CI over all supported Apple Silicon lanes.

Full details in ADRs and future revisions.
