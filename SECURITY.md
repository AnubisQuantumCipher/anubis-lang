# Security Policy

## Reporting a vulnerability

Report privately to **sic.tau@pm.me**. Do **not** open a public issue for a
security problem. You will get an acknowledgement, and — if the report is valid —
credit in the fix, unless you prefer to stay anonymous.

Please include:

- a **minimal `.anb` program** (or command sequence) that reproduces it,
- the `anubis` version / commit (`anubis --version`, `git rev-parse HEAD`),
- what you expected versus what happened.

## The bug class that matters most: a *false accept*

Anubis stakes everything on one invariant:

> a green `anubis check` must never certify a contract that `anubis run` violates.

If you find a program where **`anubis check` accepts** but **`anubis run` traps** a
`requires` / `ensures` / `assert` / `invariant` — or a secret reaches a sink with
no diagnostic, or a declared-effect boundary is bypassed — that is a **soundness
break**, and it is the highest-severity report this project can receive. It is
worth more than a crash.

To tell a genuine false accept from a *completeness gap*: negate the property (flip
the predicate / constant) and re-check.

- The opposite is **rejected** → genuine **false accept** (a real, directional
  bug). Please report it.
- The opposite is **also accepted** → a **fail-open deferral** (the checker models
  nothing there and leaves it to the runtime). That is a documented, safe
  completeness limit, not a soundness break — an issue, not a security report.

## In scope

- The checker, the native SMT solver, the information-flow / effect / capability
  lanes, the self-host toolchain, evidence-bundle tamper-evidence and signatures,
  and the package trust store.
- Memory-unsafety or a crash in the compiler/runtime on untrusted input.

## Explicitly out of scope

Anubis ships an **engagement-scoped offensive toolchain** on purpose (`fuzz`,
`vz-exploit`, the AOP C2/PoC surface). Its *ability* to run authorized offensive
actions inside isolated VZ guests is a feature, not a vulnerability — the riskiest
primitives are `PLAN_ONLY` by design and every action is receipted. A report that
"Anubis can perform offensive operations" is out of scope; a report that one of
those primitives **escapes its VZ isolation or its receipt/authorization gate** is
very much in scope.

## Supported versions

Anubis is **pre-1.0**; security fixes land on the active development branch. There
is no long-term-support release yet.
