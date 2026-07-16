# Proof scaling — honest boundaries of proof-oriented Anubis

This note records, plainly, where proof-oriented Anubis is **young by design**, so the limits are
understood rather than discovered. It is grounded in this repository's actual proving path
(`tools/anubis/src/main.rs` `Commands::Prove` → `lower_program_to_guest` → a RISC0 zkVM guest), not in
any other project's numbers.

## The two architectural boundaries

These are properties of the approach, not bugs. They will not be "fixed" by a patch.

### 1. The dynamic runtime representation is expensive inside RISC0

Anubis values are a **dynamically typed, boxed** enum at runtime — `AnubisValue::{Int, Str(Rc<String>),
List, Struct, Bool, …}` (`compiler/src/backends/run.rs`). That representation is what makes the native
interpreter flexible and Turing-complete. When the SAME lowering is compiled into the zkVM guest, every
value carries dynamic dispatch and reference-counted heap traffic, and **every executed instruction is a
proved cycle**. A boxed dynamic value model inside a cycle-counted zkVM is inherently many-cycle: work
that is nearly free natively (an `enum` match, an `Rc` clone) becomes proving cost.

The real remedy is a **type-directed backend**: monomorphize a fast `int`/`bool` path out of the boxed
enum so proof-relevant integer arithmetic lowers to plain machine words. That is a full backend effort,
not a slice, and it is the honest long-horizon fix. It does not exist today.

### 2. A large program generates an enormous zkVM workload even when the proof branch is small

`lower_program_to_guest` lowers the **whole program** — every function reachable from `main()`, plus any
injected crypto / PoC-kit runtimes. There is **no proof-branch slicing and no dead-code elimination for
the guest** (grep of `compiler/` and `tools/` finds no `#[proof]` / entrypoint / DCE machinery). So
prover time and memory track **total program size**, not the size of the computation you actually want
to prove. A 2000-line program with a 10-line proof obligation still pays for all 2000 lines in-circuit.

The real remedy is a `#[proof]` / entrypoint **attribute** plus reachability slicing (prove only the
marked sub-branch, bind its inputs/journal), which needs new parser/AST syntax and a slicer — a
multi-week feature, not a slice. It does not exist today.

## What ships now (honest, incremental)

- **A lowering-size honesty warning.** When a program lowers to a large guest (> 256 KB of guest source
  or > 256 functions), `prove` prints a stderr warning that the whole program becomes proving work and
  points here. It does not reduce cost; it stops a large lowering from silently generating an enormous
  workload.
- **Shared-module composition for proofs.** `prove` now resolves `import`s through the same front-end as
  `run` (`load_program_items`), so proof programs can factor into shared modules — which also means a
  program should import only what it proves, keeping the guest smaller.
- **One input surface.** `run` accepts the same `--input-json` / `--input-file` as `prove`, so a program
  that both runs and proves has a single input format.

## Maturity gap (native vs proof-oriented)

Native Anubis is markedly more mature than proof-oriented application design, and that is expected: the
native path is a direct interpreter, while the proof path must additionally lower to a constrained zkVM
target under the two boundaries above. Closing the gap is a roadmap arc (a proof-aware value backend and
a proof-branch entrypoint), not a single change. Until then, proof-oriented programs should be written
**small and proof-focused** — the warning and the honest limits here are the guardrails.
