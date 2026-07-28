# Anubis — Completion Blueprint

**Standing execution plan. Run phases in order. Do not skip. Do not regress.**

This file is the *execution* plan. It is not an authority on status. The authorities are:

- `docs/CLAIMS.md` § Known open issues — the single living list of open soundness defects
- `docs/language/ROADMAP.md` — the phase arc and living status layer

If this file and those disagree, **they win** and this file gets corrected. Never let this become a
second source of truth; that is the exact disease the project is fighting.

---

## The one sentence that decides "complete"

> `anubis check` PASS ⇒ the program cannot violate its stated contracts, effects, capabilities, or
> information-flow policy at runtime.

`docs/language/ROADMAP.md` states the consequence plainly:

> Phases 0–10 freestanding "DONE / At DoD / ROADMAP COMPLETE" are **FALSE** as a current *soundness*
> claim while CLAIMS open §1 stands. **Open false accepts break it.**

**Everything else can be green and it does not matter while a false accept stands.** That is the
sequencing rationale for the whole file.

---

## Measured baseline (2026-07-27, re-measure before quoting)

| Quantity | Value | How to re-derive |
|---|---:|---|
| Commits | 778 | `git rev-list --count HEAD` |
| Lean theorems | 162 | strip block comments first; naive grep gives 163 |
| Lean modules | 15 | `ls formal/Anubis/*.lean` |
| Security fixtures on disk | 280 | `ls examples/security/*.anb \| wc -l` |
| Language fixtures | 244 | `ls tests/fixtures/language_core/*.anb \| wc -l` |
| Security gate | 278/278 PASS | 2 red fixtures held out of tree pending their fix |
| Language gate | 244/244 PASS | pin `ANUBIS_BIN=./target/release/anubis` |

Re-derive by command. A number quoted from memory in this project has been wrong more than once —
including a "242/242 PASS" that was actually 242/244 FAIL.

### Corrections to the external assessment (2026-07-27)

An outside review rated the project 9.8/10 and is largely fair on architecture and ambition. Three of
its factual claims are **stale or false** and must not be carried forward:

1. **"Security tests 149/149"** — stale. The corpus is 280 fixtures; the gate reads 278/278 with two
   held out.
2. **"150+ theorems / 14 Lean modules"** — stale. Measured: **162 theorems across 15 modules**.
3. **"A green `anubis check` *never* certifies a contract that `anubis run` violates … This is not
   marketing."** — **This was false when written.** On 2026-07-27, with a fully green board, this
   program passed `check` and printed the secret at runtime:

   ```anubis
   fn key() -> secret<i64> { return 42; }
   fn app(f) { print(f()); }
   fn main() { app(key); }        // check exit 0; run printed 42
   ```

   Closed in `d5f0be8`. Four sibling carriers were still open at time of writing. **A green board is
   when a claim surface is most dangerous** — a reviewer sees 244/244 and concludes the promise is
   discharged, when the corpus merely stopped offering a counterexample.

The assessment's architecture read (self-hosting, native solver, dual-use, fail-closed intent) is
sound. Its *soundness* claim was not. Keep the first, discard the second.

---

## PROGRESS — 2026-07-28 (measured, re-derive before quoting)

| Gate | Now |
|---|---:|
| unified audit (`audit_unified.sh --profile full`) | **24/24 PASS**, 0 failed / 0 skipped / 0 external |
| language fixtures | **247/247** |
| security fixtures | **317/317** |
| stdlib fail-closed | **104/104** |
| stdlib integration gate | **10 pass / 1 fail** (was 7/4) |
| builtin surface (213) | **179 FAIL_CLOSED_OK · 11 RUN_REFUSES · 23 RUNS**, 0 crashes |
| native-authoritative corpus | **898 files, 0 mismatches** |
| Lean | **162 theorems / 15 modules**, machine-checked on host (G21) |

**Phase 1 — DONE.** 41/41 published carrier routes reject, element ladder 25/25, 25/25 pure guards
still accept, 0 over-rejection.

**Phase 2 — DONE.** `compiler/src/middle/carrier.rs` matches every `Expr` variant with no wildcard
arm; `run_carrier_totality_gate.sh` PLANTS a variant and proves rustc refuses it, then restores the
tree. Registered as **G23**. `compiler/src/middle/loopctl.rs` extends the same discipline to all 15
`Stmt` variants.

**Phase 3 — DONE.** `gate_common.sh`, content-addressed binary pins, coverage ratchets. Two ratchets
were found pointed at the wrong quantity during Phase 6 and corrected — see CLAIMS 2026-07-28.

**Phase 4 — DONE.** All five criteria; `t1_encrypted_c2 PASS (whoami over aop-2)`.

**Phase 5 — DONE, bounded residual published.** 213 builtins classified by the (`check`, `run`) PAIR.
The 11 `RUN_REFUSES` are proof-lane/native-lowering constructs with no run-lane implementation; they
refuse rather than assume, so the published promise holds and `check` is INCOMPLETE about runnability.
`break`/`continue` outside a loop now reject at check time.

**Phase 6 — DONE except the VM seal, which is deliberately NOT claimed.** `run_failclosed` reaches
`PASS_RUNTIME_FAILCLOSED_WHOLE`; **G24 promise-coherence** gate registered (it caught a real drift in
HANDOFF.md on first run); counts reconciled across README/AGENTS/CLAIMS.

**Closed since the 2026-07-27 list, each verified by command:**

- **crypto/hash/KDF/random builtin slice** — no longer unmeasured; every crypto name is classified in
  `docs/evidence/builtin_surface_matrix.tsv`.
- **research-mode receipt chain on the tart path** — a real crash op now seals:
  `sealed vz_exploit_run … (seq=2)` and `receipt chain ok=true count=4`.
- **`push` check/run divergence** — `let ys = push(xs,3); len(ys)` now runs and is CORRECT
  (`3`, `[1, 2, 3]`); check rc=0, run rc=0.
- **`vz-c2-cycle`** — closed under Phase 4.

**Still open, and named:**

- **Generics / HM / traits** — structurally absent from the language, and the reason the self-host
  grammar cannot express the type engine's full surface. [NEEDS-HUMAN] parser growth.
- **Trusting-trust closure** — DDC 34/34 and hermetic repro raise the bar; they do not close it.
- **`edges_all_modules` is NOT this residual any more.** It was published as needing generics on the
  assertion helpers; that diagnosis was wrong (the construct was, not the type system) and it is
  closed — see CLAIMS 2026-07-28.

**VM battery: 19/19, fixpoint sealed (2026-07-28).** `gate failures : 0`, `✓ fixpoint matches
baseline`, `PASS — all gates green, fixpoint unchanged`. `formal` runs in the guest for the first
time (elan + Lean v4.32.0 installed into `anubis-xcode`; it had been exit 127, toolchain absent).
`EXPECTED_FIXPOINT_VM` re-baselined `189ac496… -> 46ddce14…` only after the board went green, having
been deliberately held back at 17/19 and 18/19 — and the digest was reproduced across four
independent disposable guests before it was written.
