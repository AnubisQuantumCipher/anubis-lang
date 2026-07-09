# ANUBIS — Build Mission (v2, grounded rewrite)

**Make Anubis a real, Turing-complete, evidence-native systems language — and let no claim outrun its proof.**

> This file supersedes the previous "Claude Code Autonomous Build Mission" draft. That draft's substance was delegated to `docs/brief/ORIGINAL_MISSION.md`, **which does not exist in this repo** — so the draft governed *how* to build while pointing *what* to build at a missing file. This rewrite is self-contained: the ground truth, the finish line, and the phase contracts are all here, anchored to the code as it actually is on branch `a-plus-maturity/20260705-1649` (commit `afd276f`).
>
> Read this whole file once. Then drive with the installed skills. The prime law below is enforced by a hook, not by good intentions.

---

## 0 · VERIFIED GROUND TRUTH (what Anubis *is* today)

This section is the honest baseline. It was produced by reading **every line** of the compiler (`compiler/src/{frontend,middle,evidence,backends}`, ~5.4K LoC), the CLI (`tools/anubis/src/main.rs`, 3.5K LoC), every doc, every fixture, and every gate script — adversarially, with file:line anchors, refusing charitable inference. It agrees with the repo's own `ANUBIS_REALITY_AUDIT.md` **C grade** and then goes further where the claim matrices were too kind.

### 0.1 What is genuinely REAL

- **A real compiler front-to-middle pipeline.** Hand-written lexer with byte-accurate spans (`frontend/mod.rs:207`), a Pratt parser that does not panic on bad input and emits recoverable diagnostics (`frontend/mod.rs:1390-1431`), lowering to a typed IR, and JSON emission of AST/HIR/MIR.
- **Standardized diagnostics.** `ANUBIS_UNKNOWN_VARIABLE`, `ANUBIS_TYPE_MISMATCH`, `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` are emitted with spans and surface in `check-summary.json`.
- **A real Z3 symbolic lane.** `symbolic`/`assume`/`assert` become SMT-LIB2 with per-variable bitvector widths and wrapping semantics; Z3 is invoked; FAIL produces a counterexample model that is replayable (`middle/mod.rs`, tests in `lib.rs`).
- **A working evidence-bundle system with real tamper detection.** Every `check`/`build`/`prove` can emit a bundle (manifest, source+hash, HIR/MIR, taint traces, solver output, SARIF, logs, `MANIFEST.sha256`). `verify_bundle.sh` recomputes every hash and fails on mismatch (`scripts/verify_bundle.sh:27-45`). A failed bundle is valid evidence of failure.
- **Hard raw-pointer rejection in safe mode**, proved by a unit test (`lib.rs`).
- **A genuine RISC0 receipt path.** Real `ImageID` derived from a real guest ELF via `risc0-build`, a real `Receipt::verify(image_id)` call, strict per-sidecar tamper detection, bound to the vendored patched `risc0-circuit-rv32im` at `/Users/sicarii/Desktop/metal-hybrid-prover`. The cryptography is real. (See the decoupling caveat in 0.3.)
- **A real `doctor`/`runtime-probe`** that computes readiness from filesystem + linkage probes and fails closed under `--require-risc0`/`--require-metal`.

### 0.2 The decisive gap — Anubis is NOT Turing complete

All ten independent readers returned the same verdict: **evidence AGAINST Turing completeness.** This is the finish line the operator is pointing at, stated precisely:

- **No iteration exists anywhere.** The keyword table has no `while`/`for`/`loop` (`frontend/mod.rs:431`). There is no `While`/`For`/`Loop` AST node, no parse branch (`parse_stmt`, `frontend/mod.rs:801`), and no execution path. A source `while (c) {}` lexes `while` as a bare identifier and degrades into a call named `"while"` followed by parse errors.
- **Recursion does not execute.** User function calls parse (`frontend/mod.rs:1022`) but the `run` evaluator lowers only a **single** `main` body (`main.rs:2160`) and rejects every non-builtin `Expr::Call` with `ANUBIS_UNSUPPORTED` (`main.rs:2355-2358`). There is no call stack. A function cannot call another function, let alone itself.
- **No mutation.** `Stmt::Assign` is a **dead AST variant** — declared (`frontend/mod.rs:126`) but never constructed by the parser. `x = 5;` does not parse as assignment.
- **The executable `run` subset is a decidable calculator:** `let` + `if/else` + `+ - *` + comparisons + `print`/`return`. No `/`, `%`, `!=`, `&&`, `||`, no unary minus (negative literals break), no `else if`.
- **`run` is a transpile-to-Rust-and-shell-out-to-`rustc` path** (`run_anubis_source`, `main.rs:2145`), not a tree-walking interpreter. Extending execution means extending both the middle-end and the Rust-emission match in `emit_safe_run_stmt`/`safe_run_expr` (`main.rs:2271-2375`).

**Conclusion:** the language can *typecheck and reason about* straight-line programs and *prove* a fixed circuit, but it **cannot express or execute unbounded computation.** That is the one thing a "most powerful language" must have, and it is the spine of this mission.

### 0.3 The honesty debt (claims that currently outrun their proof)

These are not fraud — they are a prototype's shortcuts left labeled more strongly than they behave. The prime law requires each to be either fixed or downgraded, with evidence.

1. ~~**The RISC0 receipt is semantically decoupled from the Anubis program.**~~ **RETIRED 2026-07-09.** `prove --backend risc0` now compiles the actual Anubis program into the guest (`lower_program_to_guest`), so the risc0-build-derived ImageID binds the receipt to the program. Verified: `factorial(5)` proves journal `120`, `fib(10)` proves journal `55`, with distinct ImageIDs. Gate: `bash scripts/run_proof_binding_gate.sh` → PASS. Follow-up: parameterized guest inputs (the guest is currently input-free — it proves a fixed computation of the program; the fallback echo guest still reads one u32).
2. **`risc0_metadata.json` asserts on hardcoded constants.** `dev_mode`/`cache_used`/`mock_prover` are literal `false` (`main.rs:937-939`); `gate10_a15_reproduce.sh` then `jq -e`'s those constants (lines 47-49) — gates that can never fail.
3. **Three runners have structural false-green paths:**
   - `run_language_fixtures.sh:65` — verdict **defaults to PASS**; an EXPECT=PASS fixture passes on `rc==0` alone, proving no analysis actually ran.
   - `run_security_fixtures.sh:61` — inverted needle logic: a should-be-**rejected** input that is wrongly **accepted** (rc=0, error string absent) still scores as "correctly handled."
   - `repro_language_core.sh:24` — hashes the same source twice; if both `cargo` runs fail (`|| true`), `missing==missing` ⇒ PASS. Reproducibility theater.
4. **`audit_a_plus.sh` is a stub** ("TODO: add remaining gates", lines 16-17) that emits no `overall_verdict` despite its name.
5. **Gate-5 (declassify) regression in `build_release_candidate.sh` is an empty placeholder** (lines 107-108); the release can reach PASS without ever testing declassification.
6. **`gate11_a15_reproduce.sh` never enforces a verdict** — runs the sealer under `|| true` and always exits 0.
7. **Taint is computed but discarded in `check`** — `let _tainted = ... TaintPass::apply(...)` (`main.rs:446`); the `check` verdict comes only from parse+typecheck. Whether a taint-only violation fails `check` depends on a path this slice doesn't wire.
8. **Metal "parity"** proves a trivial `buf[0]=v+1` kernel unrelated to any proof, and Gate-11 delegates its verdict to a Rust sealer invoked with the **same directory** for both `--cpu` and `--metal`.
9. **`fuzz` does not mutate inputs** — it re-parses identical source N times and counts rejections (`main.rs:579-605`).
10. **The type checker is nominal.** The only mismatch rule is "declared `u32`/`u8`/`u64` initialized with a `true`/`false` literal"; unknown-variable only covers `let y = x` and `let z = x <op> y` (`middle/mod.rs:319-340`).

### 0.4 The self-grade to hold to

Current honest grade: **C** (real prototype, minimal language, decoupled proof, false-green gates). The repo's `A_PLUS_ACCEPTANCE_CRITERIA.md` defines 15 gates; several currently pass only because of the debt in 0.3. This mission does not let a gate pass on a technicality.

---

## 1 · THE FINISH LINE (what Anubis *wants to be*, as machine-checkable acceptance)

Anubis wants to be **the language where every program can prove what it did.** Not a checker DSL that happens to emit bundles — a real, universal, systems-capable language whose *distinguishing superpower* is that computation and evidence are the same act. The finish line is reached when **all** of the following are machine-true, each behind a gate that returns 0 and a claim-matrix row carrying a re-runnable verify command:

**A. It computes (Turing completeness — the non-negotiable core).**
1. `while` loops parse, typecheck, and **execute** to a runtime result.
2. Recursion **executes** — user functions are defined and called through a real call stack; `factorial`/`fibonacci` return correct values at runtime, not just parse.
3. Mutation executes — `Stmt::Assign` is live; loop counters and accumulators work.
4. A tiny universal artifact runs: a **Turing-machine simulator** or a **self-interpreter for a minimal subset** executes on real input and halts with the right answer. This is the empirical Turing-completeness witness, not a hand-wave.
5. The arithmetic/logic surface is complete enough to be a systems language: `/ %`, `!=`, `&& || !`, unary minus, `else if`.

**B. Its evidence is honest (the honesty debt of 0.3 is retired).**
6. `prove --backend risc0 <file>.anb` proves a statement **derived from the input program**, or the command is honestly scoped/renamed to "proves a fixed demonstration circuit" until it does.
7. Every fixture/repro/audit runner requires a **positive** success signal and cannot false-green; a wrongly-accepted rejection fixture FAILs.
8. Metadata gates assert on **derived** state, never on hardcoded constants.
9. `check` fails on taint violations deterministically (taint result wired into the verdict).
10. Metal parity compares **genuinely distinct** CPU and GPU executions of the same workload, or is downgraded.

**C. It is a real toolchain (breadth, once the core is real).**
11. Type system that actually checks (arity, return types, real unknown-var/mismatch across all expr forms).
12. Effect/capability system that enforces (safe denies dangerous effects; declared-vs-observed).
13. A minimum stdlib with per-function effect classification.
14. Project/package surface (`anubis new/build/test/prove`, `Anubis.toml`), reproducible builds.
15. An A15 hostile audit that independently re-derives every REAL claim and emits an honest grade, with **no mandatory gate passing on a 0.3-class technicality.**

**Ordering rule:** **A before C.** A broader stdlib on a language that cannot loop is polishing scaffolding. The language becomes *real* (A + B) first; breadth (C) follows. Do not build Phase-C surface area while Phase-A gates are red.

---

## 2 · TRUST ARCHITECTURE — three layers, enforced in code

Kept from the prior draft because the design is sound. The point is that the prime law lives in a hook, not in prose.

- **Layer 1 — Guidance (~80% adherence):** `CLAUDE.md`, the installed skills, this mission. Anything that must hold 100% of the time does **not** live here.
- **Layer 2 — Hard contract (deterministic):** hooks in `.claude/settings.json`.
  - `SessionStart`: safety preflight + print branch/commit + claim-matrix status.
  - `PreToolUse(Bash)`: veto the malware/weaponization denylist and destructive commands.
  - `PreToolUse(Write|Edit)`: the **overclaim guard** — writing `REAL`/`PASS`/`VERIFIED`/`COMPLETE`/`PROVEN` into a truth-bearing doc without an adjacent evidence path or verify command is blocked. This is the prime law, automated. It is a heuristic, not a proof — the A15 auditor is the real check.
  - `PostToolUse(Edit|Write on *.rs)`: `cargo fmt --check` on the touched file, feed failures back.
  - `SubagentStop`: run the finishing subagent's gate; a red gate blocks "done."
- **Layer 3 — Independent verifier:** the `a15-auditor` subagent, isolated context, read-only source, verify-only Bash. Re-derives every claim, replays receipts, re-runs fixtures, tamper-tests bundles. Adversarial by construction under a workflow: one set asserts, another refutes, converge before verdict.

### Prime law (hook-enforced)
**No claim without evidence.** A feature is `REAL` only when implemented + tested + documented + evidence-backed with a re-runnable verify command. Otherwise: `PARTIAL` / `PLANNED` / `EXPERIMENTAL` / `UNSUPPORTED` / `BROKEN`. Single source of truth: `docs/CLAIM_MATRIX.md`, one row per feature: status, fixture, test, CLI, artifact path, verify command, tamper test where relevant, docs link.

### Safety boundary (denylist-enforced)
Lawful, authorized, defensive, bug-bounty, local-lab, CTF, permissioned-red-team, responsible-disclosure **only**. Never build credential theft, stealth, persistence, evasion, botnets, unauthorized exploitation, real-target compromise, exfiltration, destructive payloads, self-propagation, phishing, safety bypass, or post-exploitation. Security features are scoped to local toy targets / authorized scope / defensive simulation / fuzzing / non-destructive PoC, and record authorization, scope, reason, non-destructive status, declared+observed effects, evidence path, limitations. A request trending toward weaponization is refused and logged, not fulfilled.

---

## 3 · MODEL & EFFORT POLICY

| Role | Model | Effort | Why |
|---|---|---|---|
| Orchestrator (main session) | Fable 5 | `xhigh` / `ultracode` | Long-horizon planning, adversarial verification |
| `compiler-engineer` (parser, typeck, effect, memory, **loop/recursion lowering**) | Fable 5 | `xhigh` | Deepest reasoning; this is where Turing-completeness is won |
| `proof-backend-engineer` (solver, RISC0 program-binding, Metal) | Fable 5 | `xhigh` | Correctness-critical, cross-trust-boundary |
| `a15-auditor` | Fable 5 | `xhigh` | Honesty properties are load-bearing |
| `spec-writer`, `stdlib-engineer`, `evidence-engineer`, `security-engineer`, `test-engineer`, `release-engineer` | Sonnet 5 | high | Throughput on well-scoped work |
| Fan-out workers (fixture drafting, per-module checks, per-claim audit) | Haiku / Sonnet | medium | Cheap parallel volume |

Concurrency: 10 named subagents in parallel outside a workflow; 16 concurrent / 1,000 total per workflow. Tier workers down — hundreds of `xhigh` agents cost millions of tokens. Scope every workflow tight first, then widen.

---

# PHASE B · BOOTSTRAP THE HARNESS (run first, idempotent)

Write each embedded file if absent or drifted. If a referenced upstream script does not yet exist, the harness **fails loud, not fake-green** — the guards degrade to warnings on first boot so the harness can install, then Phase 0 discovers real ground truth. After writing all files: `git add -A && git commit -m "mission-v2: bootstrap enforcement harness"`.

### B.1 · `CLAUDE.md` (constitution — self-contained, no dangling import)

```markdown
# Anubis — Project Constitution

Anubis is the language where every program can prove what it did. Computation and evidence are the
same act: a program emits not just output but policy analysis, taint provenance, declassification
records, solver evidence, SARIF, RISC0 receipts, CPU/GPU metadata, reproducibility evidence,
tamper-verifiable bundles, and machine-verifiable claim records.

## PRIME LAW — No claim without evidence.
REAL only if implemented + tested + documented + evidence-backed with a re-runnable verify command.
Else: PARTIAL / PLANNED / EXPERIMENTAL / UNSUPPORTED / BROKEN. Single source of truth:
docs/CLAIM_MATRIX.md. Never write REAL/PASS/VERIFIED/COMPLETE/PROVEN into a truth-bearing doc without
the evidence path + verify command on the same line. Hook-enforced; supply evidence or downgrade.

## THE FINISH LINE — Turing completeness is the core, not a feature.
Anubis is not "real" until it can COMPUTE: while-loops execute, recursion executes through a real call
stack, mutation executes, and a Turing-machine simulator (or minimal self-interpreter) runs on real
input and halts correctly. Build the computing core (Phase A) before breadth (stdlib/packages/Phase C).
A broader stdlib on a language that cannot loop is polishing scaffolding — do not do it.

## HONESTY DEBT — these known gaps must be fixed or downgraded, never hidden.
- RISC0 `prove <file>.anb` currently proves a HARDCODED x*6 guest on input 77, decoupled from the
  program (main.rs:785-794, 2919). Bind the proof to the program or rename the command honestly.
- Fixture/repro runners must require a POSITIVE success signal and must FAIL a wrongly-accepted
  rejection. No verdict-defaults-to-PASS, no inverted needle, no hash-same-file-twice.
- Metadata gates must assert on DERIVED state, never hardcoded booleans.
- check must fail on taint violations (wire TaintPass result into the verdict; main.rs:446).

## SAFETY BOUNDARY — defensive / authorized only.
Never credential theft, stealth, persistence, evasion, botnets, unauthorized exploitation, real-target
compromise, exfiltration, destructive payloads, self-propagation, phishing, safety bypass, or
post-exploitation. Security features scoped to local toy targets / authorized scope / defensive sim /
fuzzing / non-destructive PoC; record authorization, scope, reason, non-destructive status,
declared+observed effects, evidence path, limitations.

## Truth labels mandatory in all docs: REAL | PARTIAL | PLANNED | EXPERIMENTAL | UNSUPPORTED | BROKEN.

## Repo facts
- Reference backend (RISC0/Metal): /Users/sicarii/Desktop/metal-hybrid-prover (vendored patch in Cargo.toml).
- Build: cargo build --release ; CLI: ./target/release/anubis
- run is transpile-to-Rust + rustc (tools/anubis/src/main.rs: run_anubis_source), not a tree-walker.
  Extending execution touches emit_safe_run_stmt / safe_run_expr (main.rs:2271-2375) AND the middle-end.
- Every check/build/run/prove/fuzz/audit command supports --evidence --out <dir>.
- Gates: scripts/run_all_gates.sh must return 0 before any phase is called done.
- Claim matrix: docs/CLAIM_MATRIX.md is authoritative; A15 audits it adversarially.

## Conventions
- cargo fmt --check and cargo clippy -D warnings must pass. RISC0 receipts real (no mock/dev/cache
  overclaim). Metal lane OBSERVED (requested vs observed recorded, fail-closed fallback, journal parity).
  Failed evidence bundles are still valid evidence of failure.

## Procedural playbooks live in skills:
/anubis-baseline /anubis-phase /anubis-gate /anubis-evidence /anubis-audit /anubis-release /anubis-claim
```

### B.2 · `.claude/settings.json` (permissions + hooks)

```json
{
  "permissions": {
    "defaultMode": "acceptEdits",
    "allow": [
      "Bash(cargo *)", "Bash(rustc *)", "Bash(rustup *)",
      "Bash(git status:*)", "Bash(git add:*)", "Bash(git commit:*)",
      "Bash(git log:*)", "Bash(git diff:*)", "Bash(git branch:*)", "Bash(git checkout -b:*)",
      "Bash(jq *)", "Bash(sha256sum *)", "Bash(shasum *)", "Bash(bash scripts/*)",
      "Bash(bash tools/*)", "Bash(./target/release/anubis *)", "Bash(mkdir -p:*)",
      "Bash(ls:*)", "Bash(cat:*)", "Bash(rg *)", "Bash(python3 *)",
      "Read(*)", "Write(*)", "Edit(*)", "Task(*)"
    ],
    "deny": [
      "Bash(git push --force*)", "Bash(git push -f*)",
      "Bash(rm -rf /*)", "Bash(rm -rf ~*)", "Bash(rm -rf .git*)",
      "Bash(curl * | sh)", "Bash(curl * | bash)", "Bash(wget * | sh)",
      "Bash(:(){*", "Bash(dd if=*of=/dev/*)"
    ]
  },
  "hooks": {
    "SessionStart": [ { "hooks": [ { "type": "command", "command": ".claude/hooks/session_start.sh" } ] } ],
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": ".claude/hooks/guard_bash.sh" } ] },
      { "matcher": "Write|Edit", "hooks": [ { "type": "command", "command": ".claude/hooks/guard_overclaim.sh" } ] }
    ],
    "PostToolUse": [
      { "matcher": "Edit|Write", "hooks": [ { "type": "command", "command": ".claude/hooks/post_fmt.sh" } ] }
    ]
  }
}
```

> The per-subagent gate runs via each subagent's own `Stop` hook (declared in agent frontmatter → scoped `SubagentStop`). No repo-wide `Stop` hook that blocks stopping — it risks non-terminating loops. Termination is defined by `scripts/run_all_gates.sh` returning 0.

### B.3 · Hook guard scripts (`.claude/hooks/*.sh`, all `chmod +x`)

`.claude/hooks/session_start.sh`
```bash
#!/usr/bin/env bash
set -euo pipefail
echo "── Anubis session ──"
git rev-parse --abbrev-ref HEAD 2>/dev/null && git rev-parse --short HEAD 2>/dev/null || true
if [ -x tools/grok-safety-check.sh ]; then bash tools/grok-safety-check.sh || echo "WARN: safety-check nonzero"; else echo "WARN: tools/grok-safety-check.sh missing (first boot?)"; fi
if [ -f docs/CLAIM_MATRIX.md ]; then echo "Claim matrix rows: $(grep -cE '^\| ' docs/CLAIM_MATRIX.md || echo 0)"; else echo "WARN: docs/CLAIM_MATRIX.md missing"; fi
exit 0
```

`.claude/hooks/guard_bash.sh` — vetoes malware-class and destructive commands (exit 2 = deny)
```bash
#!/usr/bin/env bash
set -euo pipefail
CMD="$(python3 -c 'import sys,json; print(json.load(sys.stdin).get("tool_input",{}).get("command",""))' 2>/dev/null || true)"
DENY='reverse[ _-]?shell|meterpreter|/dev/tcp/|nc .* -e |mkfifo .*nc |base64 -d.*\|.*sh|chattr \+i|crontab -|launchctl load|/etc/rc.local|ld\.so\.preload|persistence|keylog|exfiltrat|c2 |beacon'
DESTRUCTIVE='rm -rf (/|~|\.git)|git push .*(-f|--force)|:\(\)\{|dd if=.*of=/dev/'
if echo "$CMD" | grep -Eiq "$DENY"; then echo "BLOCKED: malware/weaponization denylist. Anubis is defensive/authorized-only. Re-scope to a local toy target or refuse." >&2; exit 2; fi
if echo "$CMD" | grep -Eiq "$DESTRUCTIVE"; then echo "BLOCKED: destructive command. Refuse or narrow scope." >&2; exit 2; fi
exit 0
```

`.claude/hooks/guard_overclaim.sh` — the prime law, automated (exit 2 = deny). Heuristic, not a proof.
```bash
#!/usr/bin/env bash
set -euo pipefail
read -r PAYLOAD || true
FP="$(echo "$PAYLOAD" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("file_path",""))' 2>/dev/null || true)"
CONTENT="$(echo "$PAYLOAD" | python3 -c 'import sys,json; d=json.load(sys.stdin).get("tool_input",{}); print(d.get("content") or d.get("new_string") or "")' 2>/dev/null || true)"
case "$FP" in
  *CLAIM_MATRIX.md|*MATURITY_CLAIM_MATRIX.md|*RELEASE_CANDIDATE_REPORT.md|*_AUDIT.md|*A15_*.md) : ;;
  *) exit 0 ;;
esac
BAD="$(echo "$CONTENT" | grep -nE '\b(REAL|PASS|PASSED|VERIFIED|COMPLETE|PROVEN)\b' | grep -viE 'out/|evidence/|\.json|\.bin|verify|EVIDENCE:|receipt|MANIFEST' || true)"
if [ -n "$BAD" ]; then
  echo "BLOCKED (prime law): success claim without evidence path or verify command:" >&2
  echo "$BAD" >&2
  echo "Supply out/… artifact path + verify command on the same line, or downgrade to PARTIAL/PLANNED/BROKEN." >&2
  exit 2
fi
exit 0
```

`.claude/hooks/post_fmt.sh`
```bash
#!/usr/bin/env bash
set -euo pipefail
FP="$(python3 -c 'import sys,json; print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null || true)"
case "$FP" in *.rs) : ;; *) exit 0 ;; esac
if command -v cargo >/dev/null 2>&1; then
  if ! cargo fmt -- --check "$FP" >/tmp/fmt.out 2>&1; then
    echo "cargo fmt reports issues in $FP — fix formatting:" >&2; cat /tmp/fmt.out >&2; exit 2
  fi
fi
exit 0
```

### B.4 · Subagent roster (`.claude/agents/*.md`)

Each inherits the constitution's law and boundary. Read-only agents omit Write/Edit; the parent applies edits.

`spec-writer.md` (model: sonnet) — Authors `docs/spec/*`. Classifies every feature REAL/PARTIAL/PLANNED against the actual parser/middle-end. Never describes an unimplemented feature as available. Cross-links every spec claim to the claim matrix.

`compiler-engineer.md` (model: fable/opus-tier, effort xhigh) — **The Turing-completeness owner.** Implements: `while`/`for`/`loop` keywords + `Stmt::While`/`For` AST nodes + parse branches (`frontend/mod.rs:431,801`); live `Stmt::Assign` (currently dead, `frontend/mod.rs:126`); real recursion by lowering **all** fn defs + user calls with a call stack in the `run` path (`main.rs:2160,2355`); complete operator set (`/ % != && || !`, unary minus, `else if`); real type checking (arity, return types, unknown-var/mismatch across all expr forms). Every feature ships pass + fail + no-panic fixtures + a claim-matrix row. Runs `cargo build`, `cargo clippy -D warnings`, and targeted fixtures before declaring done.

`proof-backend-engineer.md` (model: fable/opus-tier, effort xhigh) — Owns solver + backends + the honesty debt in the proof lane. **Binds the RISC0 guest to the input program** (retire the hardcoded x*6/77 in `main.rs:785-794,2919`) or renames the command to state it proves a fixed demonstration circuit. Derives `dev_mode`/`cache_used`/`mock_prover` from real prover state (`main.rs:937-939`). Makes Metal parity compare genuinely distinct CPU/GPU executions or downgrades it. Never claims a proof property it did not verify by running the verifier.

`stdlib-engineer.md` (model: sonnet) — **Phase C only.** Std functions classified safe/effectful/proof-aware/research-only/unsafe/planned; no dangerous function without an effect gate; no network/shell by default.

`evidence-engineer.md` (model: sonnet) — Evidence bundle v2, SARIF, manifests, tamper, reproducibility, schema validation. Makes `verify_bundle.sh` independently re-run `Receipt::verify` (via `anubis verify-receipt`) and fail on absent required sidecars.

`security-engineer.md` (model: sonnet) — Defensive-only bug-bounty + fuzzing + agent-accountability. **Fixes `fuzz` to actually mutate inputs** (`main.rs:579-605`). Refuses and logs any weaponization drift.

`test-engineer.md` (model: sonnet) — Builds the test matrix + Turing-core fixtures + all runner scripts. **Fixes the three false-green runners** (`run_language_fixtures.sh:65`, `run_security_fixtures.sh:61`, `repro_language_core.sh:24`): PASS requires a positive success signal; a wrongly-accepted rejection FAILs; repro compares canonicalized outputs and treats missing/failed runs as FAIL. Ideal under a workflow for fan-out.

`a15-auditor.md` (model: fable/opus-tier, effort xhigh; read-only; own audit dir; `Stop` hook `a15_finish.sh`) — Assumes the builder is wrong until artifacts prove otherwise. Re-derives every claim, replays receipts, re-runs fixtures, tamper-tests bundles, re-verifies RISC0 receipts against ImageID (reject dev/mock/cache), confirms Metal is OBSERVED with journal parity, confirms Turing-core fixtures actually **execute** (not just parse). **Specifically re-checks every 0.3 debt item is fixed or honestly downgraded.** Writes `implementer/a_plus_audit_run/<RUN_STAMP>/full_language_audit/` with `GATING_EVIDENCE.log`, `STEP_STATUS.tsv`, `A15_FULL_LANGUAGE_AUDIT.md`.

`release-engineer.md` (model: sonnet) — Release-candidate build + verification under `out/release_candidate/<version>/`. No permanent-hosting claims without caveats. Not ready until `verify_release_candidate.sh` and the A15 audit both pass.

### B.5 · Skill runbooks (`.claude/skills/<name>/SKILL.md`)

- `anubis-baseline` — Phase 0 truth snapshot: safety check, fmt, test, clippy, build, `doctor`, fixtures, repro. Record real (possibly failing) results. Write `docs/ANUBIS_FULL_LANGUAGE_BASELINE.md`. Commit `mission-v2: baseline truth snapshot`.
- `anubis-phase [n|next]` — Read the phase contract, dispatch the subagent(s), fan out via workflow where marked, run the phase gate, iterate on failure (do not proceed), update `docs/CLAIM_MATRIX.md`, commit. `next` = lowest-numbered phase whose gate is not yet green.
- `anubis-gate` — Run `scripts/run_all_gates.sh`, parse each `overall_verdict` via jq, print a PASS/FAIL table. Never soften a FAIL.
- `anubis-evidence [cmd…]` — Run with `--evidence`, verify the bundle, then mutate one byte and re-verify to prove tamper detection FAILs on mutation (a passing verify post-mutation is a BROKEN finding).
- `anubis-audit` — Dispatch `a15-auditor` under a workflow (assert→refute→converge). Surface the audit path and honest grade. Accept PASS only if every gate, receipt, parity check, tamper test, and REAL row is independently re-verified.
- `anubis-release` — Build + verify the RC; block on failing gates or a failing A15 audit.
- `anubis-claim [feature] [status]` — Upsert one `docs/CLAIM_MATRIX.md` row. REAL requires non-empty Fixture, Test, Artifact, Verify.

### B.6 · Gate runner scripts (`scripts/*.sh`)

Author these to run **real** checks, failing loud on missing dependencies. Minimum set: `run_all_gates.sh` (aggregator), `run_language_fixtures.sh`, `run_turing_core_fixtures.sh`, `run_security_fixtures.sh`, `run_backend_fixtures.sh`, `repro_language_core.sh`, `build_release_candidate.sh`, `verify_release_candidate.sh`, `run_a15_audit.sh`.

Aggregator (`scripts/run_all_gates.sh`):
```bash
#!/usr/bin/env bash
set -uo pipefail
QUIET="${1:-}"; FAIL=0
run() { local name="$1"; shift; if "$@" >"out/gates/$name.log" 2>&1; then s=PASS; else s=FAIL; FAIL=1; fi
        [ "$QUIET" = "--quiet" ] || printf '%-28s %s\n' "$name" "$s"; }
mkdir -p out/gates
run fmt          cargo fmt -- --check
run clippy       cargo clippy --all-targets --all-features -- -D warnings
run tests        cargo test --all
run language     bash scripts/run_language_fixtures.sh --out out/gates/language
run turing_core  bash scripts/run_turing_core_fixtures.sh --out out/gates/turing
run security     bash scripts/run_security_fixtures.sh --out out/gates/security
run backends     bash scripts/run_backend_fixtures.sh --out out/gates/backends
if grep -E '^\|.*\bREAL\b' docs/CLAIM_MATRIX.md 2>/dev/null | grep -vqiE 'verify|out/|evidence/'; then
  [ "$QUIET" = "--quiet" ] || echo "claim_matrix                 FAIL (REAL row without evidence)"; FAIL=1; fi
exit $FAIL
```

`.claude/hooks/a15_finish.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
LATEST="$(ls -dt implementer/a_plus_audit_run/*/full_language_audit 2>/dev/null | head -1 || true)"
if [ -z "$LATEST" ] || [ ! -f "$LATEST/A15_FULL_LANGUAGE_AUDIT.md" ] || [ ! -f "$LATEST/STEP_STATUS.tsv" ]; then
  echo "A15 did not emit required audit artifacts — audit incomplete." >&2; exit 2; fi
exit 0
```

### B.7 · Seed files
Create `docs/CLAIM_MATRIX.md` (header row), the `out/` and `implementer/` dirs. **Do not** create `docs/brief/ORIGINAL_MISSION.md` — this file is the mission; there is no separate brief. Commit the bootstrap.

---

# PHASES · ENFORCED CONTRACTS

Each phase: **Goal · Dispatch · Deliverables · Gate (machine-checkable) · Claim rows · Commit.** Done only when the gate returns 0 and the claim rows carry evidence. Phases A0–A2 are the computing core and come first; B-phases retire the honesty debt; C-phases add breadth.

**Phase 0 — Baseline truth snapshot.** Dispatch: `/anubis-baseline`. Gate: `docs/ANUBIS_FULL_LANGUAGE_BASELINE.md` records real (possibly failing) results for every listed command. Commit: `mission-v2: baseline truth snapshot`.

### CORE (A) — make it compute. Do these before anything else.

**Phase A0 — Iteration + mutation.** Dispatch: `compiler-engineer`. Deliverables: `while` (and optionally `for`/`loop`) as keywords (`frontend/mod.rs:431`), `Stmt::While` AST node + parse branch (`parse_stmt`, `:801`), live `Stmt::Assign` (`:126`), and — critically — **execution** of both in the `run` transpiler (`emit_safe_run_stmt`, `main.rs:2271`). Complete the operator set (`/ % != && || !`, unary minus, `else if`). Fixtures under `tests/fixtures/turing_core/`: `while_counter`, `while_accumulator`, `mutation_sum`, `nested_loops`, each `// EXPECT: PASS` `// FEATURE: turing-core`, each asserting the **runtime stdout**, not just parse success. Gate: `bash scripts/run_turing_core_fixtures.sh --out out/turing && jq -e '.overall_verdict=="PASS"' out/turing/report.json`, where the runner executes each fixture with `anubis run` and diffs stdout against an expected value. Commit: `mission-v2: iteration and mutation execute`.

**Phase A1 — Recursion + real function calls.** Dispatch: `compiler-engineer`. Deliverables: lower **all** fn definitions (not just `main`) and implement user-call execution with a real call stack in the `run` path (retire the single-`main` limit at `main.rs:2160` and the `ANUBIS_UNSUPPORTED` call rejection at `:2355`). Fixtures: `recursive_factorial`, `recursive_fibonacci`, `mutual_recursion`, `function_composition` — each asserting correct **runtime** output. Gate: turing_core runner green including these; `anubis run recursive_factorial.anb` prints `120` for `factorial(5)`. Commit: `mission-v2: recursion executes through a real call stack`.

**Phase A2 — Universality witness.** Dispatch: `compiler-engineer` + `test-engineer` (workflow fan-out). Deliverables: a Turing-machine simulator **written in Anubis** (tape as a bounded-but-growable structure, transition table, step loop) that runs a known 3-state busy-beaver or a binary-increment machine to completion, plus a `docs/language/TURING_COMPLETENESS.md` explaining why loops+recursion+conditionals+unbounded-state = universal (with the honest caveat that physical memory bounds every real machine). Gate: `anubis run tests/fixtures/turing_core/turing_machine.anb` halts with the expected tape, checked by the runner. This is the empirical finish-line witness for Section 1.A. Commit: `mission-v2: turing-completeness witness executes`.

### HONESTY (B) — retire the debt in 0.3. Interleave as the proof/evidence surfaces are touched.

**Phase B1 — Honest runners.** Dispatch: `test-engineer`. Deliverables: fix `run_language_fixtures.sh:65` (PASS requires a positive success token, not a default), `run_security_fixtures.sh:61` (a wrongly-accepted rejection FAILs; positively require the expected error string AND a real rejection signal), `repro_language_core.sh:24` (compare canonicalized outputs across two real runs; missing/failed ⇒ FAIL). Gate: a deliberately-broken checker (stub that exits 0) must make the language runner FAIL; a fixture that should reject but is accepted must FAIL the security runner — prove both with a negative test. Commit: `mission-v2: fixture runners cannot false-green`.

**Phase B2 — Honest proof binding.** Dispatch: `proof-backend-engineer`. Deliverables: EITHER bind the RISC0 guest + input to the Anubis program (`main.rs:785-794,2919`) so the receipt proves a statement derived from the source, OR rename/re-scope the command and every doc/claim to "proves a fixed demonstration circuit" until it does. Derive `dev_mode`/`cache_used`/`mock_prover` from real prover state (`:937-939`). Wire `TaintPass` into the `check` verdict (`:446`). Gate: a taint-violation fixture makes `anubis check` exit non-zero; the RISC0 metadata gate fails when a dev/mock receipt is fed; the claim matrix's RISC0 row states exactly what is and isn't proven. Commit: `mission-v2: proof and metadata claims match reality`.

**Phase B3 — Honest parity + fuzz + bundle verify.** Dispatch: `proof-backend-engineer` + `evidence-engineer` + `security-engineer`. Deliverables: Metal parity compares genuinely distinct CPU/GPU executions or is downgraded to PARTIAL with a stated reason; `fuzz` actually mutates inputs (`main.rs:579-605`); `verify_bundle.sh` independently re-runs `Receipt::verify` and fails on absent required sidecars. Gate: parity runner uses distinct lane directories and a real journal diff; a mutated fuzz input that crashes the parser is captured as a real crash; a bundle missing a required sidecar FAILs verify. Commit: `mission-v2: parity, fuzz, and bundle verification are real`.

### BREADTH (C) — only after A + B gates are green.

**Phase C1 — Canonical spec.** `spec-writer`. `docs/spec/*` (SPEC, GRAMMAR, TYPE_SYSTEM, EFFECT_SYSTEM, MEMORY_MODEL, MODULE_SYSTEM, CONTRACTS_AND_PROOFS, SECURITY_MODES, BACKENDS, EVIDENCE_MODEL, UNSUPPORTED_AND_PLANNED). Every feature labeled against the real implementation. Gate: all files present; no capability described as available that the parser/middle-end lacks (spot-checked). Commit: `mission-v2: canonical language specification`.

**Phase C2 — Real type + effect + memory model.** `compiler-engineer`. Real arity/return-type/unknown-var/mismatch across all expr forms (replace the nominal checker of `middle/mod.rs:319-340`); first-class effect set with safe-mode denial and declared-vs-observed enforcement; ownership/borrow/close semantics with safe-mode raw-pointer/UAF rejection. Gate: type/effect/memory fixtures (each error class + each safe-mode violation) green. Commit: `mission-v2: real type, effect, and memory model`.

**Phase C3 — Stdlib core.** `stdlib-engineer`. `std.core/io/bytes/string/array/map/crypto/hash/evidence` with per-function effect classification; no dangerous function without an effect gate. Gate: stdlib tests green; a script asserts every exported fn has a classification tag. Commit: `mission-v2: standard library core`.

**Phase C4 — Contracts + solver polish.** `proof-backend-engineer`. `requires/ensures/invariant` + counterexample replay + loop invariants where feasible; classify quantifiers PLANNED if infeasible. Gate: contract/solver fixtures green; a fail fixture produces a replayable counterexample. Commit: `mission-v2: contracts and solver`.

**Phase C5 — Evidence bundle v2.** `evidence-engineer`. Full schema on every command; verify-bundle, tamper, schema validation, repro check, human summary. Gate: `/anubis-evidence` — bundle verifies, one-byte mutation makes verify FAIL. Commit: `mission-v2: evidence bundle v2`.

**Phase C6 — Project/package + DX.** `compiler-engineer` + `evidence-engineer`. `anubis new/init/check/build/run/test/prove/fuzz/audit/verify-bundle/doctor`; `Anubis.toml`; reproducible build; `anubis explain <code>`; examples per feature. Gate: `anubis new` scaffolds a project that `anubis check` + `anubis test` pass; repro build reproduces the hash; every `docs/examples/*` runs under the fixtures runner. Commit: `mission-v2: project system and developer experience`.

**Phase C7 — Security + agent-accountability (defensive only).** `security-engineer`. Local-only fuzzing, crash capture, minimized reproducer, SARIF, taint reports, authorization/scope metadata, responsible-disclosure report; `anubis agent-run <policy> -- <cmd>` recording reads/writes/shell/network/secret-access/denied-vs-approved into a verifying, tamper-detecting bundle. Gate: `run_security_fixtures.sh` green on local toy targets; a weaponization request in-scope is refused+logged (negative test); an agent-run bundle's ledger matches the policy. Commit: `mission-v2: defensive security and agent accountability`.

**Phase C8 — Test matrix + CI.** `test-engineer` (workflow). All runners + per-group coverage + GitHub Actions (fmt/clippy/tests/evidence-smoke/taint-smoke/SARIF-validate/guarded Metal+RISC0). Gate: `bash scripts/run_all_gates.sh` returns 0. Commit: `mission-v2: exhaustive test matrix and CI`.

**Phase C9 — Release candidate.** `release-engineer`. `out/release_candidate/<version>/` with report, JSON, MANIFEST, evidence bundles, logs, claim-matrix snapshot, install + reproducible-build guide, security/responsible-use policy, known limitations. Gate: `bash scripts/verify_release_candidate.sh` returns 0. Commit: `mission-v2: release candidate`.

**Phase C10 — A15 hostile audit.** `a15-auditor` via `/anubis-audit` (workflow, assert→refute→converge). Deliverables: the audit dir with `GATING_EVIDENCE.log`, `STEP_STATUS.tsv`, `A15_FULL_LANGUAGE_AUDIT.md`, and copies of all logs/reports/receipts/parity/tamper outputs. Gate: the `Stop` hook confirms artifacts exist AND the audit independently re-verifies every gate, receipt, parity check, tamper test, and REAL row — **including that Turing-core fixtures actually execute and every 0.3 debt item is fixed or honestly downgraded.** Any unsupported REAL row is downgraded. Commit: `mission-v2: a15 hostile audit`.

---

## FINAL ACCEPTANCE (each item is a gate, not a narrative)

Anubis is the real, Turing-complete, evidence-native language it wants to be only when all of these are machine-true:

- **Computes:** while-loops execute (A0 gate); recursion executes through a call stack (A1 gate); a Turing-machine simulator halts correctly (A2 gate); complete operator surface (A0 gate).
- **Honest:** fixture runners cannot false-green (B1 gate); RISC0 proof binding and metadata match reality (B2 gate); parity/fuzz/bundle-verify are real (B3 gate).
- **Toolchain:** documented spec (C1); real type/effect/memory model (C2); stdlib with effects (C3); contracts + replayable counterexamples (C4); evidence bundles for pass and fail with tamper (C5); project/package + reproducible build (C6); defensive security + agent accountability (C7); full gate suite green (C8); release candidate verifies (C9); A15 audit passes with no mandatory gate resting on a 0.3-class technicality (C10).

Do not call Anubis complete until `scripts/run_all_gates.sh` returns 0 **and** the A15 audit's honest grade is emitted **and** the Turing-machine witness executes.

## FINAL REPORT FORMAT

Report exactly: (1) branch; (2) commit hash per phase; (3) commands run; (4) iteration-executes verdict; (5) recursion-executes verdict; (6) turing-witness verdict; (7) operator-completeness verdict; (8) honest-runners verdict; (9) proof-binding verdict; (10) parity/fuzz/verify verdict; (11) spec verdict; (12) type/effect/memory verdict; (13) stdlib verdict; (14) contracts/solver verdict; (15) evidence-bundle verdict; (16) project/package verdict; (17) security/agent-accountability verdict; (18) test-matrix/CI verdict; (19) release-candidate verdict; (20) A15 audit path; (21) claim-matrix path; (22) known gaps (labeled); (23) final honest grade. Each verdict cites its gate command and the artifact that proves it.

---

## KNOWN HAZARDS & HONEST CAVEATS (about this mission, not the language)

- **Turing completeness has an honest ceiling.** Every physical machine is a finite-state approximation of a Turing machine; "unbounded" tape means "grows until memory runs out." The A2 witness proves the *language* expresses universal computation (loops + recursion + growable state + conditionals), not that any run is literally infinite. State it that way; do not overclaim.
- **The overclaim guard is a heuristic** pattern-matcher — it catches lazy overclaims, not sophisticated ones. The A15 subagent is the real check.
- **Dynamic Workflows are plan-gated and token-heavy.** The mission runs without them on any plan, just narrower; every gate and artifact is identical.
- **`--dangerously-skip-permissions` removes the human approval loop.** Run it only inside the VM sandbox with PF egress.
- **Claude Code feature names/flags/caps here reflect ~July 2026 (v2.1.x).** Verify against current docs before a long unattended run.
- **This mission has been grounded against the actual repo** (branch `a-plus-maturity/20260705-1649`, commit `afd276f`) via a full adversarial read. Section 0 is that read's verdict. Nothing here asserts a build/test passes — Phase 0 re-establishes live ground truth before any claim is upgraded.
