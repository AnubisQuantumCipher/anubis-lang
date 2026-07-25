# SPARK vs Anubis — evidence, not theater

This note replaces a floating “SPARK vs Anubis” marketing comparison that mixed
**true** product differences with **false Anubis outputs**. Everything below is
grounded in:

- AdaCore SPARK 2014 User’s Guide (GNATprove messages, AoRTE, counterexamples, “possible fix”)
- Live `gnatprove` on a minimal overflow unit (2026-07-25)
- Live `anubis check` / `anubis run` on this tree (same day)

**Rule:** if a claim has no command + observed result, it is not a claim.

---

## What SPARK actually does (from docs + live tool)

| Mechanism | Behavior |
|-----------|----------|
| **AoRTE (absence of run-time errors)** | GNATprove generates VCs for overflow, division by zero, index bounds, etc. — **even without a user `Post`**. |
| **Overflow check** | On bare `X := X + 1`, high/medium: *overflow check might fail*, often with `e.g. when X = Integer'Last`, and *possible fix: mention X in a precondition*. |
| **Contracts** | `Pre` / `Post` / `Depends` / `Global` — functional properties are **extra** on top of AoRTE. |
| **Loop invariants** | Manual `pragma Loop_Invariant`; without them, postconditions and overflow VCs around loops often stay unproved. |
| **Counterexamples** | Optional/level-dependent (`--counterexamples=on`); path + values; not always feasible. |
| **Maturity** | Decades of industrial use (avionics, rail, space). That is real. |

**Live SPARK (local gnatprove, 2026-07-25):**

```text
increment.adb:4:14: high: overflow check might fail, cannot prove upper bound for X + 1
  e.g. when X = Integer'Last
  possible fix: subprogram … should mention X in a precondition

increment.adb:9:14: info: overflow check proved   # after Pre => X < Integer'Last
```

---

## What Anubis actually does (code + live tool)

| Mechanism | Behavior |
|-----------|----------|
| **Integer semantics** | **Wrapping i64** (two’s complement). `MAX + 1` runs as `MIN` — observed with `anubis run`. |
| **Contracts** | `requires` / `ensures` / `assert` / loop `invariant` — discharged by SMT (native QF_BV + z3 cross-check). |
| **No free AoRTE historically** | A bare `return x + 1` with **no** contracts **passes** check (wrap is intentional). |
| **False posts** | `ensures(result > x)` on `x + 1` is **DISPROVED** with CEX `x = i64::MAX` — **not** “passes”. |
| **Unmodeled / loop-mutated** | Often `ANUBIS_CONTRACT_UNPROVABLE` (fail-closed), **not** a fake counterexample. |
| **Tuples** | `result.0` / `result.1` in contracts **do not parse**. Use struct fields. |
| **Security extras** | Taint, secrets, capabilities, evidence bundles — SPARK does not ship these as core. |

### Anubis-native upgrades (this branch)

Two SPARK-grade UX pieces, done the **Anubis** way:

#### 1. Wrap-safety (AoRTE-lite) — `ANUBIS_WRAP_RISK`

When integer parameters are solver-modelable (function has contracts / int asserts),
Anubis emits **automatic wrap-safety obligations** on modelable `+` / `-` / `*`:

```text
$ anubis check  # ensures(result == x + 1) only, no requires
ANUBIS_WRAP_RISK: 1 integer operation(s) can wrap under free inputs …
  wrap-safety:(+ anb_x (_ bv1 64))
  counterexample:
    x = 0x7fffffffffffffff  (9223372036854775807)
  possible fix (from counterexample; edit + paste onto the fn signature):
    requires(x < 9223372036854775807)  // excludes counterexample x=i64::MAX
```

- **Opt out:** `ANUBIS_WRAP_SAFETY=0` (restore “wrap only, no automatic VC”).
- **Bare** functions with **no** modeled params still **pass** (language wrap unchanged).
- **Bounded** `requires(x < MAX)` + ensures → **check passed**.

This is the honest dual of SPARK’s overflow check:

| | SPARK | Anubis |
|--|-------|--------|
| Overflow is | language error to prove absent | wrap risk, with **concrete sat model** |
| Fix prompt | “mention X in a precondition” | **paste-ready** `requires(x < MAX)` from the witness |
| Semantics | no wrap if proved | wrap is real at runtime unless you bound |

#### 2. CEX-guided `possible fix` on every DISPROVED / WRAP_RISK

Disproof diagnostics now append **editable** `requires(...)` candidates derived from the
model (MAX / MIN / −1 heuristics). Never auto-applied.

#### 3. Pretty counterexamples

`ANUBIS_ASSERTION_DISPROVED` (not conflated `UNPROVEN`) + source names + hex + signed decimal.

---

## False claims that must not reappear

| Claim | Reality |
|-------|---------|
| `ensures(result > x)` on bare `x+1` **passes** | **DISPROVED** at `x = MAX` |
| Anubis swap with `result.0` / `result.1` | **Parse error** |
| Loop without inv fails with a concrete CEX | Often **`UNPROVABLE`**, no model |
| Printed `saturating_add` “just proves” | **DISPROVED** as written |
| “Anubis automatic, SPARK needs a PhD” | **Oversell** — both are SMT-backed; SPARK’s default **AoRTE** is the real difference |
| “Anubis fail-closed on everything” | Contract-free code can pass check and wrap at run |

---

## Side-by-side that *is* true (re-runnable)

### Increment

```bash
# SPARK-shaped Anubis: contract without bound → wrap risk + fix
cat > /tmp/inc.anb << 'EOF'
fn increment(x: i64) -> i64
    ensures(result == x + 1)
{ return x + 1; }
fn main() {}
EOF
anubis check /tmp/inc.anb
# → ANUBIS_WRAP_RISK … x = MAX … possible fix: requires(x < 9223372036854775807)

# Full prove
cat > /tmp/inc_ok.anb << 'EOF'
fn increment(x: i64) -> i64
    requires(x < 9223372036854775807)
    ensures(result == x + 1)
    ensures(result > x)
{ return x + 1; }
fn main() {}
EOF
anubis check /tmp/inc_ok.anb
# → check passed
```

### Runtime wrap (Anubis honesty)

```bash
# MAX + 1 prints MIN under wrap
anubis run …  # observed: -9223372036854775808
```

---

## Where Anubis is *actually* ahead (when evidence holds)

1. **Counterexample as product** — pretty model + replay attestation + **possible fix** on the same diagnostic.
2. **Fail-closed unmodeled fragment** — will not invent a BV witness for string/float/untracked loop state.
3. **Security lattice in the language** — taint / secret / capabilities / evidence bundles (not SPARK’s job).
4. **Native authoritative solver path** — RUP Unsat cert + model replay on the proven fragment (see README boundary).

## Where SPARK is still ahead (honest)

1. **Default AoRTE breadth** — indexes, discriminants, initialization, flow/`Depends`, concurrency profiles.
2. **Industrial certification trail** and multi-decade toolchain.
3. **`Integer'Last` as a first-class language notion** vs typing `9223372036854775807` (Anubis can improve with `i64::MAX` sugar later).
4. **Loop / modular reasoning maturity** for large Ada codebases.

---

## Design stance (Anubis way)

- **Do not** pretend wrapping is SPARK-style “no overflow” by default.
- **Do** make wrap risk **visible, concrete, and repairable** when the checker is already reasoning about the function.
- **Do** keep security and evidence as first-class surfaces SPARK never owned.
- **Never** ship fake PASS/FAIL strings in comparisons.

---

## Verification commands (this note)

```bash
cargo test -p anubis-compiler --lib wrap_safety
cargo test -p anubis-compiler --lib counterexample
./target/release/anubis check examples/showcase/ring_buffer_underflow.anb
# expect ANUBIS_ASSERTION_DISPROVED + pretty head/tail
```

Opt out of wrap-safety:

```bash
ANUBIS_WRAP_SAFETY=0 anubis check …
```
