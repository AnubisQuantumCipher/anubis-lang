# AGENTS.md — Anubis

Auto-loaded into every agent session in this repo. Keep it TRUE; a stale file here silently briefs
every worker with a dead plan and nothing warns you.

Last verified against the tree: 2026-07-27.

## What Anubis is

A proof-carrying systems language with a **Safe mode** and a **Research (offensive) mode**. Its
entire promise is one sentence:

> **`anubis check` passing means Anubis found no way for the program to violate its stated
> contracts, effects, capabilities, or information-flow policy at runtime — and everything it could
> not decide, it refused rather than assumed.**
>
> That is the goal AND the honest form of the claim. It is deliberately not "cannot violate":
> absolute totality is not established, and stating it that way was FALSE as recently as
> 2026-07-27, when `fn app(f){print(f());} … app(key)` passed `check` on a fully green board and
> printed the secret at runtime. **Green means no KNOWN defects, not no defects.** The residual is
> named, not implied — `docs/CLAIMS.md` § "Open — load-bearing" is the single living list, and a
> claim stronger than that list is a bug in the claim.

SMT-backed contracts, taint and information-flow enforcement, effect and capability tracking, a
from-scratch native SMT solver whose authority is bounded to Lean-proven bit-blasts, and a
self-hosted checker written in Anubis itself. It is designed to fail closed; any observed exception
belongs in `docs/CLAIMS.md`, not behind an absolute claim.

**Making the language "complete" means making that sentence TRUE**, not adding features.

## The binary

`./target/release/anubis` — **never** the bare `anubis`, a shell alias hijacks it.

**Do not run `cargo build`.** Many agents share one target dir; a build takes the lock from
everyone. The lead owns builds and will say when a fresh binary is published.

## The one defect class that explains this codebase

Nearly every false accept ever found here is the same disease:

> **A user writes something down — a return type, a field type, an attribute — or a producer
> computes a label, and a CONSUMER either ignores it or recomputes it independently.**

Two corollaries that have each cost real time:

- **Walker parity.** There are many independently maintained value-flow walkers over the same AST,
  plus an *enforcing* lane and a *summary* lane for taint and secret. Fixing one
  and not its sibling leaves a working launder path — that happened four separate times in one
  session. When you fix a construct, check every walker that handles it.
- **Under-approximating catch-alls.** A `_ => {}` arm that silently reports "clean" is a defect
  factory. Where a walker decides a security question it should be TOTAL over the AST enum, so a new
  variant fails to COMPILE. Precedents: `solver/src/fragment.rs::is_proven_authoritative`,
  `effects::walk_expr`, `collect_expr_vars`, `body_has_mode_elevator`.

**Never merge namespaces.** A map keyed by a bare method name that a free function can also claim is
a four-times-proven defect generator here (`method_contracts`, `fn_declared_effects`,
`fn_effect_rows`, the resolver). Keep free-fn and impl-method registries separate.

## Standing rules

- **No git. No commits, no pushes, ever.** The lead commits. This is absolute.
- **READ-ONLY on `compiler/src/**` and `solver/**`** unless your lane explicitly grants otherwise.
  Deliver unified diffs and fixtures; the lead applies them.
- **Zero fabrication.** Every number from a command you ran; every code claim with `file:line`. A
  step you did not run is **SKIPPED with a reason**, never PASS. Out of evidence is `[NEEDS-HUMAN]`.
  A plausible claim you did not verify is the worst thing you can hand the lead, because they will
  act on it.
- **The human presses send.** Draft disclosures; never transmit. No external filing, emailing or
  posting, and no public repos.

## Verification bar

A finding is real only if the **DIRECT form is REJECTED and the LAUNDERED form is ACCEPTED**, both
with real pasted output. If the negated variant is also accepted, that is a benign symmetric blind
spot — say so, do not pad the list. **A runtime proof beats a structural argument**: run it and show
the file appearing or the secret printing.

**Validate the instrument before the number.** This project has been misled repeatedly by harnesses
failing in ways indistinguishable from real findings: a gate exiting 127 reporting "44/44 FAIL";
"1764 corpus failures" that were timeouts; a lane that only fails under machine load; an unmatched
`grep` under `set -euo pipefail` truncating a corpus mid-run while still printing green. If a result
looks dramatic, check the instrument first.

## Gotchas that cost hours

- `run_security_fixtures.sh` / `run_language_fixtures.sh` **do not rebuild** — they grade whatever
  `./target/release/anubis` happens to be. Both accept **`ANUBIS_BIN`** to pin a snapshot; always
  pass a private `--out`, because the default output dir races between agents.
- A bare `{ … }` in **value position parses as a MAP**, not a block. Value-block probes need
  `if true { … }` or a match arm to produce a real `Expr::Block`.
- **Builtins are 213**, by union of five functions in `compiler/src/backends/run.rs`
  (`emit_builtin_call` + its inline `matches!` + `is_proof_input_builtin` + `is_poc_kit_builtin` +
  `is_non_run_builtin`, deduplicated). README understates at "~150". Cite the counting method.
- **Lean is 162 theorems across 15 modules.** A naive `grep '^\s*theorem '` returns 163 — one sits
  inside a block comment in `NonInterference.lean`.
- `docs/language/ROADMAP.md` has single lines up to 77,561 characters; `MATURITY_CLAIM_MATRIX.md` is
  83 KB. Work them with targeted `grep`/`sed` and verify every quote against the live file.

## Documented leniency — do NOT report these as defects

`int`/`float`/`parse_*` per LANGUAGE.md:518 · IEEE NaN/inf from `sqrt`/`ln`/`log`/`pow` · float
division by zero yielding `inf` · `position` returning `-1` · string auto-stringify. Read
`tests/fixtures/stdlib/NON_COLLECTION_SURFACE.md` before calling anything a defect.

## Offensive / research work — VZ isolation is mandatory

Anything research-gated, crash-capable, or exploit/fuzz/agent/C2-class runs **inside a disposable
tart guest** cloned from `anubis-xcode` (`./target/release/anubis vz status` first; SSH `admin` +
`~/.ssh/tart_anubis`). Host `anubis fuzz`, host `anubis run --allow-research` crash PoC, and host
`exploit-run` are **FORBIDDEN as primary evidence**, and calling a host run "isolated" is
**fabrication**. Tear down with `tart stop X; sleep 2; tart delete X`, then confirm with `tart list`
— `delete` silently no-ops on a RUNNING guest. Report `isolation: tart-disposable-guest` plus the
guest name. Crash isolation is not air-gap; no zero-NIC claim without native-preflight. **If tart is
red: STOP and say so. Do not fall through to the host.**

## Current state (2026-07-27)

Green under a pinned binary, 12-gate `SEAL_PASS`, `known_fail=0`:
security **317/317** · language **247/247** · stdlib fail-closed **104/104** · runtime 4/4 ·
selfhost 9/9 · taint/type/effect/capset self-host **0 disagreements** · formal gate PASS with every
theorem machine-checked and no `sorry`/`admit`/`axiom` · native-authoritative **901 files, 0
mismatches**.

Reproduce all of it with one command: **`bash scripts/run_seal_checklist.sh`**. It rebuilds once,
pins that binary for every gate, scores only each gate's declared verdict line (never the log body —
fixture rows contain `exp=FAIL` and a naive grep false-alarms), checks corpus completeness so a
truncated run cannot pass, and REFUSES to report PASS if any precondition is unmet.

**Green means no KNOWN defects, not no defects.** Known-open, with probes on disk:

- `anubis run` is **not** fail-closed *as a whole* — the ~213-builtin domain/arity/wrong-type/I/O
  surface is unenumerated. The instrumented surface is green; the whole claim is not available.
- The research half's **receipt chain** is unimplemented: `vz exploit`/`vz fuzz` produce no
  engagement receipt and no host evidence, so the proof-carrying thesis fails for exactly the
  operations that lane exists to prove.
- The bare `anubis` shell alias is documented, not fixed.

## The binary moves. Use a pin, not `target/release/anubis`.

`cargo build` rewrites `target/release/anubis` IN PLACE. When the lead rebuilds while you are
mid-round, your measurements straddle two different compilers — and recording a sha256 at the start
does not save you, because the PATH you recorded has already changed underneath. This has cost real
work: an adversary round legitimately recorded three different pins and re-ran everything twice, and
the lead misread a working fix as broken.

Resolve the pin ONCE at the start of a round and use it throughout:

    ANUBIS_BIN="$(scripts/publish_pin.sh --current)"
    "$ANUBIS_BIN" check foo.anb

Pins are content-addressed and read-only, so a rebuild creates a NEW file and cannot mutate the one
you are holding. If the lead publishes a new pin mid-round, finish on your old one — it still exists
and still works — then re-measure deliberately instead of discovering the change inside your results.

Report the pin PATH and its sha256, not just the sha256.

**Only the lead builds and publishes pins**, and the lead says so when a new one lands.

## Capture the exit code on the very next line

    "$ANUBIS_BIN" check "$f" >/dev/null 2>&1
    rc=$?                      # <- nothing may run before this

`printf 'x %s %s' "$(basename $f)" "$?"` prints `basename`'s status, not the check's — the command
substitution runs first. That misreporting made a working fix look broken twice in one session. In
this repo a broken harness has produced "44/44 gates FAIL" (exit 127) and "1764 corpus failures"
(timeouts). Validate the instrument before you believe the number.
