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
- **Lean is 199 theorems across 16 modules** (2026-08-18, after Phase-8 Slice-1 landed the
  37-theorem `Anubis.SecurityLabel` module). A naive `grep '^\s*theorem '` returns 200 — one sits
  inside a block comment in `NonInterference.lean`. Re-derive with `scripts/lib/docs_drift_derive.py`.
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

## Current state (2026-08-15 — Completion Phases 0–7 landed + VZ self-host seal PASSED + v0.1.2-preview released)

Latest `main` is `d8742aab`. The Completion Blueprint (`docs/COMPLETION_BLUEPRINT.md`) product arc —
Phases 0 through 7 — is landed, sealed, and released. Dated per-phase evidence lives under
`docs/evidence/`; this section is the living summary, not a second source of truth.

- **Phases 0–4 COMPLETE**, each with a signed `PHASE_<n>_COMPLETION_*.md` receipt. Phase 3 separated
  the security-label lattice from accept-biased type inference (PRs #28–#33); Phase 4 closed
  `docs/CLAIMS.md` item 21 **row 6** (place-assignment fn-identity write-carrier, PR #35) and
  explicitly published the rest of the residual surface (PR #36).
- **VZ self-host seal PASSED** — `docs/evidence/PHASE_3_VM_SEAL_2026-08-15.md`: throwaway-guest
  battery, **0 gate failures**, in-VM binary fixpoint
  `46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60` ==
  `scripts/vm/EXPECTED_FIXPOINT_VM`, disposable guest torn down, no host substitution. The golden
  `anubis-xcode` guest was re-provisioned (rustup `nightly-2026-05-10`, elan/Lean `v4.32.0`, z3
  4.15.4, cmake, coreutils) to earn it.
- **Phase 5** OPTIONAL-COMPLETE; **Phase 6** MET (30-gate `ci.yml` + `.gate_floors` + docs-drift,
  branch-protection-enforced); **Phase 7** evidence pack produced
  (`docs/evidence/RELEASE_EVIDENCE_PACK_2026-08-15/`) and **release `v0.1.2-preview` cut** (prerelease,
  commit `b5c24125`) under explicit operator authorization; **Phase 8** remains **unscheduled by
  blueprint design (§25–30)** — Slice 1 (`docs/evidence/PHASE_8_SLICE_1_2026-08-18.md`,
  `phase8/security-label-correspondence-v1`, draft PR pending architect sign-off) added the first
  bounded, production-linked mechanized-correspondence artifact for six `SecurityLabel` methods
  (37 new theorems in `Anubis.SecurityLabel`; byte-for-byte gate
  `scripts/run_security_label_correspondence_gate.sh`) and is a bounded first step, NOT a
  Phase-8 completion claim. See also
  `docs/evidence/COMPLETION_PHASES_5_8_STATUS_2026-08-15.md`.

Sealed-tree figures: security **337/337**, language **259/259**, stdlib fail-closed **104/104**,
cargo-test **1179/0**, native-authoritative **937 files / 0 mismatches**, formal at the release
seal **162 theorems / 15 modules**, docs-drift **53 stamps / 0 drift**. That seal is `v0.1.2-preview`
/ `b5c24125` (2026-08-15); the sealed-tree figures are the release-seal snapshot. Post-seal live
formal figures on this worktree are **199 theorems / 16 modules** — Phase-8 Slice-1 added the
`Anubis.SecurityLabel` module. Every PR this arc (#28–#41) was blocked until CI
`hosted-gate-witness` reported `HOSTED_PASS`.

The v0.1.2-preview release pin is `vm/pins/anubis-97cb47782d03-src-59dffa797c0f-release`, commit-bound
to `b5c24125` (built via `publish_pin.sh --release`; `--verify-release` PASS at that commit). Pin
binaries are gitignored; `.meta` provenance is tracked. `vm/pins/CURRENT` is a working pointer the
lead republishes per seal — a commit-bound pin does not match a moved-on HEAD — not a clone-portable
artifact. To re-seal a source-matching pin, run **`bash scripts/vm/run-slice.sh`** (throwaway guest,
never rebuilds on the host); for a host seal, **`bash scripts/run_seal_checklist.sh`**. Both score
only declared verdict lines and refuse missing/truncated preconditions.

**Green means no KNOWN defects, not no defects.** Known-open, with probes on disk:

- `docs/CLAIMS.md` item 21 remains load-bearing: rows 1/2 (contract `requires` carrier through
  guarded bodies / local-alias defeat), row 3 (`obj.f()` direct method-call-syntax stored-closure
  carrier), and rows 8/9/10 (unannotated array-literal / formal / return element-type precision) are
  OPEN. **Row 6** (place-assignment fn-identity write-carrier, `let`-bound read shape) was CLOSED in
  Completion Phase 4 (PR #35). Row 8 is sealed only for annotated `list<T>`/`map<K,V>` indexing; its
  unannotated array-literal twin is not closed.
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
