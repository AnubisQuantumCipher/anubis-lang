# ADR 0001: Bootstrap host in Rust

## Status
Accepted (v0.1)

## Context
Need rapid production-quality compiler with Z3, Metal, RISC0 interop, LLVM/Cranelift potential. User has deep expertise in Rust + RISC0 + Metal (risc0-metal-hybrid etc).

## Decision
Implement v0.1 compiler in Rust (workspace). Clear path to self-host later documented. Use direct entry points for testability (lex/parse/typecheck from &str).

## Consequences
- Fast iteration, great error reporting with ariadne.
- Native artifacts via rustc for MVP (real lowering later).
- Evidence system pure-ish functions + CLI write.
