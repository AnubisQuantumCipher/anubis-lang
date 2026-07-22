# Contributing to Anubis

Anubis has one non-negotiable rule, and everything here follows from it:

> **A green `anubis check` must never certify a contract that `anubis run` violates.**
> Do not make a claim that is not backed by code, a test, command output, a
> generated artifact, or sealed evidence.

Anubis is an *evidence-native* language, and the project is built the same way it
asks you to build programs: every change carries its own proof, and nothing merges
until the gates that a stranger would run on a fresh clone stay green.

---

## The one front door

There is a single command that decides "is this green?" — the same one CI runs and
the same one a reviewer runs on a clean checkout:

```bash
bash scripts/audit_a_plus.sh --out out/gate
```

It runs the 15 fail-closed gates (**G1–G15**): `fmt`, `clippy`, the unit tests, a
release build, the language / turing-core / PCA / security / poc-kit / prove /
enum-match / for-in / language-trio / offensive fixtures, and a dogfood pass. Any
`FAIL` is a non-zero exit. If it is green here, it is green in CI.

Prerequisites (macOS / Apple Silicon): the pinned toolchain is selected
automatically by `rust-toolchain.toml` (`nightly-2026-05-10`); install `ripgrep`
and `z3` (`brew install ripgrep z3`). The heavy self-host seal runs in a throwaway
VM — see [`scripts/vm/README.md`](scripts/vm/README.md).

---

## What a good change looks like

1. **Scope it to one idea.** A checker/solver change and a formatting sweep do not
   belong in the same commit.
2. **Prefer declining to guessing.** If the solver cannot faithfully model a
   construct, it must **fail closed** (leave it to the runtime) — never fabricate a
   discharge. A new modeled lane is only worth adding if it can never turn a
   runtime-violating program into a green check.
3. **Land new checks shadow-first.** A check that adds rejection power lands in
   shadow mode and is promoted to enforcing only once the whole-corpus shadow diff
   shows **zero** unexpected rejections (`scripts/run_shadow_diff.sh`).
4. **Run the soundness hunt after any solver or checker change.** The deterministic
   gates pass while a false accept ships; an adversarial hunt is what catches it.
   See `.claude/skills/anubis-soundness-hunt`. A *false accept* (check accepts, run
   traps, and the opposite property is rejected) is a real bug — fix it and re-hunt
   until dry. A *fail-open deferral* (the checker models nothing there) is a
   documented completeness gap, not a false proof — record it honestly.
5. **Keep the self-host fixpoint honest.** Changes under `selfhost/` re-seal the
   byte-identical fixpoint; re-baseline `scripts/vm/EXPECTED_FIXPOINT_VM`
   **deliberately**, with a logged reason — never a silent drift.
6. **Document the boundary.** If a feature is partial, say exactly what it proves
   and what it does not, and mark unfinished third-party commitments `[NEEDS-HUMAN]`.

---

## Submitting

- Open an issue first for anything non-trivial (templates under
  [`.github/ISSUE_TEMPLATE`](.github/ISSUE_TEMPLATE)); a **false accept** has its
  own template — please use it.
- Attach the passing gate output (or the exact commands + artifacts) to your PR;
  the PR template asks for it.
- By contributing you agree your contribution is licensed under the repository's
  [Business Source License 1.1](LICENSE) (which converts to Apache-2.0 on the
  Change Date).

Small, evidence-carrying changes get reviewed fastest. Thank you for helping keep
the check honest.
