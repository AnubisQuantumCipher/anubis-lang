# NEXUS — a secure AI agent, proved by the compiler

NEXUS is the flagship Anubis showcase: an **autonomous AI agent whose safety
properties are machine-checked**, not left to a system prompt. It forms private
beliefs, reasons over untrusted sensor input, deliberates, and acts — and
`anubis check` proves, *before it ever runs*, that:

- its private beliefs (`secret<T>`) never reach a public sink — **a leak is a compile error**;
- every untrusted `taint_source` (a sensor, a command) is validated and `declassify`-ed
  with an auditable **policy + reason** before the agent may act on it — prompt-injection
  and poisoned input become a compile error;
- outbound action is a **linear, use-once capability** (`cap_acquire("net.send")` →
  `cap_use`): it earns the right to broadcast once and spends it once;
- the **lethal trifecta** — reads-private **+** untrusted-input **+** can-exfiltrate —
  can never fire (`ANUBIS_LETHAL_TRIFECTA`);
- and it emits a **hash-committed record of its own cognitive integrity** — evidence it
  reasoned within its safety envelope — while revealing nothing about what it deliberated on.

It is also a real, **472-line** program that `check`s clean **and** `run`s, exercising
essentially the entire language: 9 Z3-verified contracts, traits, three `enum` kinds,
generics, the higher-order builtins, and a proved loop-invariant integrity chain.

```bash
# 1. Prove it — types, taint, effects, and every requires/ensures discharged by SMT
anubis check examples/showcase/nexus/nexus_cognitive_kernel.anb
# → check passed

# 2. Run it — the same program lowers to a native Apple-Silicon binary and executes
anubis run   examples/showcase/nexus/nexus_cognitive_kernel.anb
```

## What one program touches

| Feature | Where |
|---|---|
| `requires` / `ensures` contracts, discharged by **Z3** | 9 functions (`clamp`, `weighted_blend`, `margin_of_safety`, `fuse_private`, …) |
| `secret<i64>` — private beliefs, checker-enforced no-leak | `fuse_private`, `belief_surprise`, `disclosure_gate` |
| `trait` + default method, `impl … for` dispatch | `Describable` on `SensorFrame` |
| `struct` + `impl` methods / computed properties | `SensorFrame::snr`, `Certificate` |
| `enum` — unit / tuple / struct variants + `match` destructuring | `Belief`, `Verdict`, `CogAction` |
| `Result<T,E>` + `Ok`/`Err`, `Option<T>` + `if let Some`, or-patterns | `safe_divide`, Phase 7/8 |
| generics (`fn identity<T>(x: T) -> T`) | Phase 9 |
| higher-order builtins — `map` `filter` `each` `find` `all` `any` `sort_by` | Phase 7 |
| `while` + `invariant(...)`, `for i in 0..N`, mutable state | Phase 10 deliberation + integrity chain |
| `assert`, string interpolation, hash-committed convergence | throughout |

## Verified runtime output (excerpt)

Reproduced from `anubis run` on the prebuilt binary — the deliberation commitment,
sort, and integrity chain are deterministic:

```
[Phase 5] Deliberation: hash-committed, convergence-tracked
  cycles:     20
  commitment: 253638
  converged:  true
...
[Phase 10] Integrity proof: hash chain + loop invariant
  integrity chain: 117133
  batch verified: 10 items
=================================================================
The kernel proved its own cognitive integrity.
It revealed NOTHING about what was deliberated.
```

## The checker-only security companion

[`nexus_checker_security.anb`](nexus_checker_security.anb) is the information-flow
half. It uses `taint_source`, `declassify(value, policy, reason)`, `@verified`,
`uses(net.send)`, and linear `cap_acquire`/`cap_use` — primitives that live **only
in the checker lane**:

```bash
anubis check --verified examples/showcase/nexus/nexus_checker_security.anb
# → check passed  (the information-flow discipline is proved sound)

anubis run examples/showcase/nexus/nexus_checker_security.anb
# → ANUBIS_UNSUPPORTED_NATIVE_LOWERING — by design: these are proof artifacts, not runtime code
```

`anubis check` proves that every untrusted `taint_source` is validated before it is
`declassify`-ed with an auditable policy + reason, and that the network broadcast
acquires and consumes its capability exactly once — the lethal trifecta never fires.

## Honest boundary

`secret<T>` is a **compile-time** confidentiality qualifier: the checker proves no
secret reaches a public sink, and at runtime the value is an ordinary `i64` (there is
no runtime encryption — the guarantee is the information-flow proof, not ciphertext).
The checker-only companion does not `run` on purpose; its value is the *proof* that
its flows are safe.
