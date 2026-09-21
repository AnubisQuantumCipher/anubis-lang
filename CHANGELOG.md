# Changelog

All notable changes to Anubis are recorded here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Honesty rule.** This file follows `docs/CLAIMS.md`. A claim here stronger than the one there is a
> bug in the claim. Soundness closures are listed under **Fixed** only when they are backed by a
> 0-flip verdict-diff, an over-rejection guard fixture, a VM seal, and an empty audit re-run on that
> surface — not when a fixture goes green. Six items once passed the first three and were still open.

## Unreleased

`anubis check` passing means Anubis found no way for the program to violate its stated policy, and
refused what it could not decide. It does **not** yet mean the program cannot violate that policy —
see `docs/CLAIMS.md` item 21 and the phased blueprint.

### Added
- `anubis check --message-format=json` emits the `anubis-diagnostics/1` stream on stdout: one JSON
  object per refusal, then a summary, and nothing else on that channel. The epistemic class is a
  field rather than a severity, so `disproved` and `undecided` never collapse into one value. A
  refusal raised before the solver ran still reports `fail`. `suggestions` is always empty, because
  the existing "possible fix" re-fails when applied verbatim and is never re-proved. Schema and its
  deliberate omissions: `docs/language/DIAGNOSTICS_JSON.md`. An unrecognised `--message-format`
  value is refused rather than treated as `human`. Optional flag, so MINOR under
  `docs/language/SEMVER_1_0_POLICY.md`; the schema is NOT part of the 1.0 frozen surface.
- Certificate coverage in the ordinary `check` verdict, and as a `coverage` object on the JSON
  summary. A pass now states how many discharged obligations carry a re-checkable witness and names
  those resting on the solver's word. No new analysis: the native lane's decision was already being
  made per obligation and discarded. Coverage may understate itself and may never overstate it — an
  obligation whose provenance is unrecognised counts as neither.
- `[package] edition` in the manifest, with an edition this compiler does not recognise refused
  (`ANUBIS_EDITION_UNKNOWN`) rather than compiled under today's rules. `docs/language/EDITIONS.md`.
- `anubis evidence-verify` re-derives every published `rup_refutation` by reverse unit propagation,
  reading the published DIMACS and DRAT text rather than any solver struct. Measured 2026-09-21: the
  documented forgery — replace a `.drat` with a two-line stub and recompute `MANIFEST.sha256` — went
  from `overall: PASS` to FAIL exit 1; an honest bundle still passes, replayed in-process and
  independently by upstream `drat-trim`. Listed here as an added check, NOT under **Fixed**: this
  file's honesty rule requires a 0-flip verdict-diff, a VM seal and an empty audit re-run before a
  soundness closure may be claimed, and none of those has been run for it.
- `.gitattributes`, `.github/CODEOWNERS`, `.github/dependabot.yml`, and this changelog — repository
  operating surface brought in line with how the project is actually developed.
- `tools/host_exec_guard.py`: the destructive rule is now pinned by the self-test in both
  directions (15 must-block, 8 must-allow cases).

### Fixed
- Proof search is bounded by work rather than by wall clock. z3 runs under a deterministic
  `rlimit` and the native solver's clock is off by default, so the same query yields the same
  verdict under any machine load. Measured 2026-09-21: a QF_FP obligation cost 91,502,028 resource
  units and 8.18 s of CPU against the previous 10 s timeout, a margin of 1.2x, which is why provable
  contracts came back UNDECIDED under parallel load. The corpus verdict-diff under saturation that
  would *demonstrate* load-independence has NOT been run; what changed is the mechanism and the
  measurement.
- A compile-and-run could fail with `Text file busy` under parallel load. This was the POSIX
  fork/exec race, not a defect in the program being launched: `fork` copies the file-descriptor
  table, so a child momentarily holds another thread's write handle to a different executable while
  close-on-exec has not yet fired. Bounded retry on `ETXTBSY` only. Measured 2026-09-21 at four test
  threads: two failures before, none after.
- A contract refusal described its bound as a "time budget" after that bound had become a
  deterministic resource counter, which tells a reader to retry on a quieter machine when the
  verdict will not change. It now names the work budget, and the string is a named constant so its
  regression test cannot drift from production.
- A counterexample's trust was decided by searching the refusal text for the word "replayed", so the
  native lane's models — independently re-evaluated, but described in different words — were
  reported as untrustworthy.
- Extracting the bundle validator into a constant had placed it between
  `#[allow(clippy::too_many_arguments)]` and the function that attribute guarded, orphaning it onto
  the constant.
- `tools/host_exec_guard.py` decided on raw command **text** instead of resolved **targets**, which
  failed in both directions: it missed `rm -rf "$VAR"`, `$HOME/…`, `${HOME}/…`, `-fr`, `-r -f`,
  `--recursive --force`, and `$(…)` substitution, while wrongly blocking every absolute path
  including `/tmp/…`. Now parses the invocation, resolves the target, and refuses when the target is
  unknowable — fail-closed-on-unknown applied to the project's own tooling.
- Three gate scripts (`run_promise_coherence_gate.sh`, `run_proof_correspondence_gate.sh`,
  `run_native_shadow_gate.sh`) were mode `0644`, so direct invocation returned exit 126 and read as
  a gate failure.

### Known open
- **Certificate coverage is not correspondence.** A witnessed obligation means a stranger can
  re-check that *that CNF* is unsatisfiable. Nothing binds the CNF to its `.smt2`, or the `.smt2` to
  the source. `docs/PROOF_CORRESPONDENCE.md`.
- **Out-of-fragment obligations carry no witness at all.** `bvsdiv`, `bvurem`, `bvsrem`, `bvudiv`,
  `bvashr` and `sign_extend` have no machine-checked bit-blast, so the native lane declines and z3
  decides alone. This is REG-002, and it is now counted and named in the verdict rather than silent.
- **Certificates are DRAT, not LRAT with hints**, so a *verified* checker such as `cake_lpr` cannot
  yet accept Anubis output.
- **Semantic refusals carry no source location.** Their span is still start-of-file, so the JSON
  `location` field is omitted rather than reported as `1:1`. `docs/CLAIMS.md`.
- **The enforcement lanes are not total.** Confidentiality and integrity cover a subset of the AST
  while the effect and capability lanes cover all of it; carrier-routed programs can pass `check` and
  violate at runtime. Tracked as `docs/CLAIMS.md` item 21.
- `compiler/src/evidence/mod.rs` reports a taint field that is an alias of the typecheck result
  rather than an independently computed verdict.
- Hosted CI is only a `HOSTED_PASS` witness. The sealed Tart/VZ battery and require-Metal lane are
  explicitly out of CI until a dedicated hardened runner exists.

---

## Historical tags

These predate this changelog and are recorded for provenance rather than as releases. **No GitHub
Release has been published yet**; see the blueprint's Phase 1.5.5.

- `selfhost-fixpoint-v1` — 2026-07-12 — the compiler dogfooded in Anubis; self-host reaches a
  byte-identical fixpoint.
- `pca-v0.1` — 2026-07-10 — proof-carrying artifact: Ed25519 software signing plus attributable
  verify.
- `pre-a-plus-capture-20260705-1649` — 2026-07-05 — baseline captured before the A-plus maturity arc.
