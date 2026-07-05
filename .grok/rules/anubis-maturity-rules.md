# Anubis Maturity Rules (A+ Contract)

**Enforced for all work on a-plus-maturity/***

## 1. Claim Classification (mandatory everywhere)
Every technical claim in code comments, docs, tests, reports, CLI output, or chat must be labeled:

- **REAL** — backed by passing test + artifact or command output in this tree or sealed evidence.
- **PARTIAL** — implemented for some paths; documented limits + counter-examples exist.
- **EXPERIMENTAL** — works in lab but not under all gates.
- **PLANNED** — in roadmap, no code yet.
- **UNSUPPORTED** — explicitly not supported.

Forbidden words without label + evidence: "production-grade", "complete", "formally verified", "secure by default", "tamper-proof", "A+", "general-purpose", "proof-backed".

## 2. Evidence Before Merge
- No PR / commit that touches behavior merges unless:
  - New or updated test in `tests/a_plus/` or equivalent proves it.
  - Evidence bundle or SARIF or solver output cited in commit message or linked issue.
  - A15 (or equivalent) has reproduced the improvement.

## 3. Git & History
- Work only on `a-plus-maturity/*` branches or isolated worktrees.
- Never rewrite shared history.
- Baseline tag `pre-a-plus-capture-*` must remain.
- Large build artifacts never committed (enforced by .gitignore + safety script).

## 4. Destructive / Dangerous Commands
- `rm -rf` outside `target/`, `out/`, `tmp/`, worktree dirs requires explicit human confirmation in this session.
- No network calls or secret reads except those the project already declares (z3, rustc, cargo, risc0 when present).
- Safety script (`tools/grok-safety-check.sh`) must be run or sourced before risky operations.

## 5. Test & Repro Discipline
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all` must be green before claiming "build clean".
- Repro: two clean builds of same source produce identical core hashes (source, deterministic emitted .rs, simple native artifact).
- Every failing gate or hostile finding must be recorded with exact command + output path.

## 6. Arena / Worktree
- Competing implementations live in `../anubis-worktrees/NAME`.
- Winner selected only by A15 running the acceptance tests + evidence + hostile audit on the candidate.

## 7. Docs
- `docs/CLAIMS.md` and `MATURITY_CLAIM_MATRIX.md` are the source of truth.
- All other docs must cross-reference with status + evidence path.

Violations of these rules are treated as defects. Fix the process, not the symptom.
