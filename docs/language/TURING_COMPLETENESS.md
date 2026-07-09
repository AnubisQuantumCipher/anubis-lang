# Anubis Turing Completeness

**Status: REAL (executed, with a runnable universality witness).**
Verified 2026-07-09 on branch `a-plus-maturity/20260705-1649`.

Anubis is Turing-complete: its executable subset can express and run any computation a
Turing machine can, bounded only by physical memory (as every real machine is). This
document states the claim precisely, shows why it holds, and points at the runnable
evidence.

## What makes a language Turing-complete

A language is Turing-complete if it has, together:

1. **Conditional branching** — `if / else if / else`.
2. **Unbounded iteration or general recursion** — a way to repeat work an unbounded number
   of times, not fixed at compile time.
3. **Mutable, unbounded state** — storage that can grow with the computation.

Anubis now has all three, and — critically — they **execute**, not merely parse.

| Ingredient | Mechanism in Anubis | Evidence |
|---|---|---|
| Conditionals | `if` / `else if` / `else` | `tests/fixtures/turing_core/collatz.anb` |
| Unbounded iteration | `while`, `loop` + `break` / `continue` | `tests/fixtures/turing_core/while_counter.anb`, `loop_break.anb` |
| General recursion | user functions lowered to real call stack | `tests/fixtures/turing_core/recursive_factorial.anb`, `recursive_fibonacci.anb`, `mutual_recursion.anb` |
| Mutable state | `let` bindings + `x = expr;` assignment | `tests/fixtures/turing_core/while_counter.anb` |
| Unbounded state growth | recursion depth + arbitrary-precision-shaped integer stacks | `tests/fixtures/turing_core/turing_machine.anb` |

## How execution works

`anubis run` lowers the **whole program** to a self-contained Rust program (one Rust
function per Anubis function) and executes it. This means:

- User-defined functions can call one another and themselves, so **recursion runs on the
  Rust call stack** (`tools/anubis/src/main.rs`: `lower_program_to_rust` / `emit_fn`).
- `let` bindings are emitted as mutable, so **assignment mutates real state**.
- `while` / `loop` map to native loops, so **iteration is unbounded** (runs until the
  condition is false or `break`).

Safe-mode enforcement (taint, effects, raw-pointer rejection) runs first via `typecheck`;
only then is the program lowered and executed.

## The universality witness

Claiming Turing-completeness is cheap; demonstrating it is not. Anubis ships a **Turing
machine simulator written in Anubis** as an executable witness:
[`tests/fixtures/turing_core/turing_machine.anb`](../../tests/fixtures/turing_core/turing_machine.anb).

- The **tape** is encoded as two integers used as bit-stacks (`left`, `right`), with the
  head symbol in `cur`. Push/pop is pure arithmetic (`*2`, `/2`, `%2`) — no arrays needed.
- The **machine** is the 3-state, 2-symbol busy beaver (BB-3), a nontrivial Turing machine
  with known halting behaviour.
- It runs entirely in Anubis using loops, a recursive-style helper (`popcount`), mutation,
  and arithmetic — the exact ingredients above.

It halts after **14 transitions** leaving **6 ones** on the tape, matching:

1. the known busy-beaver constants **S(3) = 14, Σ(3) = 6**, and
2. an **independent reference simulator** of the identical machine.

Because Anubis can simulate an arbitrary Turing machine (the transition table is just data
in the program), it can compute anything a Turing machine can. That is Turing completeness,
demonstrated rather than asserted.

## Reproduce it

```bash
cargo build --release -p anubis
bash scripts/run_turing_core_fixtures.sh --out out/turing_core
jq -e '.overall_verdict=="PASS"' out/turing_core/report.json     # -> true

# The witness on its own:
./target/release/anubis run tests/fixtures/turing_core/turing_machine.anb   # prints 14 then 6
```

## Honest boundary

- "Unbounded" means **bounded only by available memory**, exactly like every physical
  computer. The witness proves the *language* is universal; no single run is literally
  infinite.
- `anubis run` executes via transpile-to-Rust + `rustc`. It is an execution backend, not a
  bytecode VM; the semantics are the Anubis semantics, but the host is the Rust toolchain.
- The **safe run subset** deliberately excludes research/exploit/proof constructs
  (`symbolic`, `assume`, `sink`, raw pointers, …). Those live in the `check` / `prove`
  paths, not in ordinary execution. Turing-completeness is a property of the ordinary,
  safe, executable language — which is the right place for it.

## What this does NOT claim

It does not claim a large standard library, generics, a module/name-resolution system, or
`Result`/error-handling in the surface language. Those remain PLANNED (see
[`UNSUPPORTED.md`](UNSUPPORTED.md)). Turing completeness is about computational power, and
that is now REAL and executed.
