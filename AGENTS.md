# AGENTS.md — Anubis

Auto-loaded into every agent session in this repo. Keep it TRUE; a stale file here silently briefs
every worker with a dead plan and nothing warns you.

Last verified against the tree: 2026-07-31.

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

- **Workers do not use git.** No worker commits, pushes, stages, or rewrites history. The
  operator-appointed active lead is the sole committer/pusher and must stage only reviewed explicit
  paths from a green coherent slice. On 2026-07-29 the operator appointed this Hermes session as the
  replacement lead after the prior lead retired.
- **READ-ONLY on `compiler/src/**` and `solver/**`** unless your lane explicitly grants otherwise.
  Deliver unified diffs and fixtures; the lead applies them.
- **Code and docs never share a commit.** Fixtures stay with the code slice they verify; claims,
  phase reports, and roadmap changes land separately after the code receipt exists.
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

## Current state (2026-07-31 — Phase 1 bounded complete/activated; Phase 1.5 in progress)

Commit `03210603` has a source-matched immutable candidate
`vm/pins/anubis-58ba4abc0a63`, SHA-256
`58ba4abc0a636d909aa72e4f8df06d6e2adcad3ae378396a4c62a63f106a25bf`.
Historical bounded receipt against that pin: compiler library **766/766**, security **327/327**,
language **252 passed of 252**,
stdlib fail-closed **104/104**, current native-authoritative **923 files, 0 mismatches** (the bounded
W1 receipt itself graded 916 files), and the formal gate
PASS. The bounded W1 place-resolution slice is green.

The branch also contains the subsequent `889d9a7c` offensive isolation/evidence slice. The fresh
Phase-1 working-tree receipts below supersede the earlier pre-fix guest receipt and the later
2026-07-30 offensive bundle whose report identity did not match its export manifest. Neither a
superseding receipt nor a green gate turns the uncommitted tree into shipped work.

At the deciding technical epoch, the dirty tree was green at security **327/327**, language **255/255**, stdlib
fail-closed **104/104**, compiler library **771/771**, and formal. Native-authoritative and docs
drift now share one tracked inventory of **921 files**; native-authoritative reported **0 mismatches,
0 disagreements**, and the corpus/pin poison gate passed **27/27**. The two reduced block-label
walkers are replaced by one total `walk_block_labels`; walker completeness is green with **0
findings** on the registered security walkers.

The immutable compiler used for the current technical receipt is
`vm/pins/anubis-51f4a964347a`, SHA-256
`51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027`. Its source-bound technical
epoch was
`0281e8034022fc62f4f853906a33173bc0286e9ae9a0e07b26d761a495962b03`; pin/tree verification
passed at the opening and closing of the deciding runs. Disposable guest `anubis-run-23962`
completed all **22/22** named VM gates with zero failures, unchanged fixpoint
`46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60`, source identity unchanged
before/sync/after, strict validator PASS, and verified teardown at
`out/phase1_vm_51f4_postmetrics_final_20260731T182200Z`. Disposable guest
`anubis-offensive-gate-41607` completed the offensive gate **34/34**; its strict manifest validator
and independent revalidation both passed, and teardown was verified at
`out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z`. Both guests used 5,120 MiB under the
unchanged 8,192 MiB host-reserve guard; the full battery used three build jobs. The post-metrics
old/new check diff passed over **921 files with 0 flips and 0 timeouts** at
`out/phase1_verdict_diff_281e_to_51f4_postmetrics_20260731T185400Z.json`; the independent
falsification matrix passed **9/9**, PCA passed **19/19**, corpus/pin poison passed **27/27**, and
walker completeness remained green with **0 findings**. Promise coherence passed over **5 product
restatements**, each carrying scope plus a `docs/CLAIMS.md` pointer, with zero scan errors.

Documentation is part of the pin source manifest. The external finalization receipt at
`out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md` (SHA-256
`ff6dc5cad927f27b299657df32dcf978ae6bfc2e3be18cb1a0be3334765ac328`) proves the required
source-current VM **22/22**, offensive **34/34**, 921-row zero-flip diff, exact host seal **20/20**
with captured exit 0, and manifest revalidation. Its post-receipt
`independent_review.md` (SHA-256
`b0a55b624afad5c4a3f341acf5b2dc411359d3b3d7e20942d99b969850b5f69b`) records `APPROVE` with no
blocking finding and zero source writes. That satisfied the frozen report's external predicate and
activated **bounded Phase 1 COMPLETE** for source tree
`b3b5bfd8e472aec45856ff95a6d307670c20083c620f9971f90e5d4ce50be1a1`. It did not land or ship the
dirty epoch and is not a release artifact.

Any later source-current or release claim needs a fresh commit-bound build and immutable pin, then
the VM→offensive→921-row-diff refresh sequence in
`docs/evidence/PHASE_1_COMPLETION_2026-07-31.md` §§8/12 and a new
**`bash scripts/run_seal_checklist.sh`** receipt. The dirty-epoch `51f4...` pin must never be attached
to a tag or GitHub Release. The seal runner never rebuilds; it verifies and snapshots its selected
source-bound pin, scores only declared verdict lines, and refuses missing/truncated preconditions.

**Green means no KNOWN defects, not no defects.** Known-open, with probes on disk:

- `docs/CLAIMS.md` item 21 remains load-bearing: unannotated container/return/parameter place types,
  place-assignment parity in reduced walkers, conditional contract collection, function-value body
  blind spots, and builtin-result callable identity are open. Row 8 is sealed only for annotated
  `list<T>`/`map<K,V>` indexing; its unannotated array-literal twin is explicitly not closed.
- `anubis run` is **not** fail-closed *as a whole* — the ~213-builtin domain/arity/wrong-type/I/O
  surface is unenumerated. The instrumented surface is green; the whole claim is not available.
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

Pins are source-and-binary-addressed and read-only, so a rebuild or source-manifest change creates a
NEW file and cannot mutate the one you are holding. If the lead publishes a new pin mid-round,
finish on your old one — it still exists and still works — then re-measure deliberately instead of
discovering the change inside your results. Ordinary publication creates a bounded technical pin;
a tagged release requires a clean full commit, `scripts/publish_pin.sh --release`, and a closing
`scripts/publish_pin.sh --verify-release`. A dirty-tree technical pin is never a release artifact.

Report the pin PATH and its sha256, not just the sha256.

**Only the lead builds and publishes pins**, and the lead says so when a new one lands.

## Capture the exit code on the very next line

    "$ANUBIS_BIN" check "$f" >/dev/null 2>&1
    rc=$?                      # <- nothing may run before this

`printf 'x %s %s' "$(basename $f)" "$?"` prints `basename`'s status, not the check's — the command
substitution runs first. That misreporting made a working fix look broken twice in one session. In
this repo a broken harness has produced "44/44 gates FAIL" (exit 127) and "1764 corpus failures"
(timeouts). Validate the instrument before you believe the number.
