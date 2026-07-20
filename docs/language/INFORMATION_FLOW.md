# Information flow in Anubis — one system, two source kinds

A recurring confusion is that Anubis has "two taint systems" with different guarantees. It does not.
There is **one** information-flow analysis, enforced by `anubis check` in Safe mode. It tracks two
*kinds* of source with two different questions, but the machinery, the sinks, and the release valve are
shared.

## The two source kinds

| Kind | Question it answers | Sources | Violation |
|---|---|---|---|
| **Integrity** (taint) | did *untrusted input* reach somewhere dangerous? | `input()`, `read_file`, `open`, `read_line`, `recv`, `net_recv`, `env`, `getenv`; a `tainted<T>` parameter | `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` / `ANUBIS_INTERPROC_SINK` |
| **Confidentiality** (secret) | did a *private value* leave the program? | `secret_source(...)`; a `secret<T>` parameter or return | `ANUBIS_SECRET_EXFILTRATION` / `ANUBIS_INTERPROC_EXFILTRATION` |

These are genuinely different properties — "attacker data must not reach a command shell" is not the
same as "my key must not leave the box" — so they have different *sources*. That difference is the
point, not a gap.

## Sinks are unified (integrity ⊇ confidentiality)

As of 2026-07-20 the sink sets satisfy a deliberate subset relationship: **every egress point is also a
taint sink.** Untrusted data is dangerous both when it *leaves* the program (network / shell) and when
it hits a *local* interpreter or buffer (SQL, `memcpy`, file write); a secret is dangerous only when it
*leaves*. So:

- integrity sinks = every egress (`send`, `connect`, `http_get`, `http_post`, `shell`, `system`, `exec`,
  `target_run`, …) **plus** local-injection sinks (`sql`, `write`, `write_file`, `append_file`,
  `memcpy`, `sink`);
- confidentiality sinks = the egress set only (a secret into a local write stays inside the trust
  boundary).

Before this change, `input() → shell(cmd)` — command injection — slipped through the taint lane because
`shell` was an egress sink but not a taint sink. It is now caught. There is no sink a user could reach in
one lane and miss in the other for the same threat.

## `--verified` is a different axis, not a third taint system

`--verified` (and the default contract checking in `anubis build` / `anubis run`) is about **contracts**
— `requires` / `ensures` / `assert`, discharged by the SMT solver. That is orthogonal to information
flow. You do not choose "taint tracking *or* `--verified`"; Safe-mode flow analysis and contract
verification run together and check different things.

## Higher-order flow is analyzed, and fails closed when it can't be

The historically leaky case is a secret/tainted value that flows through a *closure or function value*.
Anubis resolves these through their storage and application shape — a closure in a struct field, a list,
a map, an enum payload, one returned from a function, one aliased to a name, one passed as a parameter,
one applied through an intermediate binding. When the target genuinely cannot be resolved — e.g. a
closure selected from a container by a **symbolic index** — the analysis **fails closed**: if the
container holds any secret/tainted-capturing closure, the indirect application is treated as the leak it
may be, rather than silently accepted.

A **capturing closure** — a lambda whose body reads a secret/tainted value from the enclosing scope — is
itself treated as a labelled value, so the ordinary value-flow tracks it through struct fields, lists,
maps, function parameters and returns, aliases and (beta-substituted) forwarding: `let s = Box{cb: |x|
k}; run(s.cb)`, `let g = fwd(|x| k); g(0)`, `run(mk(k))` and the like are all caught.

**Honest boundary (known residuals under active work).** A handful of deep higher-order shapes are not
yet tracked and can currently compile a leak: a closure applied through *another* closure nested inside
a function body (`|y| { let inner = |x| k; inner(0) }`); a closure returned *conditionally* from a
helper (`fn mk(c){ if c>0 { return |x| k; } … }`); and a closure captured through a symbolic map key.
These are enumerated with reproducers in the soundness-hunt notes and are being closed. Until then, for
a program that dispatches secret/tainted-capturing closures through those specific shapes, prefer the
`--verified` effect-row analysis, which covers them. This section states exactly what is and is not
covered rather than overclaiming.

## The one release valve

Both kinds are released the same way — `declassify(value, "policy", "reason")` — an explicit, auditable
statement that a specific value is allowed past a sink. A malformed declassify does not release (the
release is keyed on the AST shape, not a substring), so you cannot accidentally weaken it.
