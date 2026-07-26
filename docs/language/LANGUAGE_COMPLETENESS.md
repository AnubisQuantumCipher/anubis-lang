# Anubis language completeness map (honest)

Anubis is already a **real programming language** on a frozen 1.0 Safe surface
([`SPEC_1_0_FREEZE.md`](SPEC_1_0_FREEZE.md)). This document maps “complete language”
expectations to **what is shipped** vs **what remains specialized residual**.

## Feature map

| Expectation | Status | Where |
|-------------|--------|--------|
| Bidirectional inference / structured `Ty` | **REAL** (enforcing assignability, generics conflict/arity, trait bounds) | `middle/ty.rs`, `middle/mod.rs` |
| Generics + traits/impl | **REAL** (parse + check); codegen **value-erased** | SPEC freeze §2 |
| **Static monomorphization inventory** | **REAL** (checker records concrete `T=…` at call sites on `TypedIR.mono_specializations`) | `typecheck` → `MonoSpecialization` |
| Native codegen monomorphized to unboxed Rust types | **PARTIAL** — runtime is `AnubisValue`; specialization inventory is static analysis | `backends/run.rs` |
| Effect system (transitive rows + linear caps) | **REAL** | `effects.rs`, `capability.rs`, `--verified` |
| Float comparison / QF_FP lane | **REAL** | contract solver float path |
| String equality / QF_S lane | **REAL** | contract solver string path |
| Multi-file imports + `import std.*` | **REAL** | `resolve`, `stdlib/` |
| Packages / lock / trust | **REAL** | `package/` |
| Stdlib modules | **REAL** (12 modules): collections, iter, option, result, str, math, testing, io, pwn, crypto, **time**, **net** | `compiler/stdlib/std/` |
| Crypto (RWC-aligned) | **REAL** host audited crates | `CRYPTO.md`, `RWC_LANGUAGE_MAP.md` |
| Research / VZ / evidence | **REAL** ops + IR; full research **grammar** residual | `SECURITY_RESEARCH_PROFILE.md` |
| Production 1.0 Safe surface | **FROZEN** | `SPEC_1_0_FREEZE.md` |

## What “complete” does **not** mean here

- Rust feature parity (async/await, const generics, borrow checker, …)
- Infinite stdlib (OS APIs, GUI, browser, …)
- DIY TLS/Noise/PQ as language core
- Claiming every residual closed forever

## How to inspect monomorphization

```anubis
fn id<T>(x: T) -> T { return x; }
fn main() {
    let a = id(1);
    let b = id("hi");
}
```

After `typecheck`, `TypedIR.mono_specializations` contains two instances of `id`
with concrete type arguments when the checker can pin them.

## Production 1.0

Use `anubis check` / `run` / `build --evidence` / `package` / `vz confine` on the
frozen surface. Expand via MINOR promotions, not silent “everything is done.”
