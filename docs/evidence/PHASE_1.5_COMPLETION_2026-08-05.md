# Phase 1.5 completion report — 2026-08-05

GitHub as the system of record.

Supersedes `docs/evidence/PHASE_1.5_COMPLETION_2026-07-31.md`, which recorded the same phase while
`main` did not exist. That report's verdict was `INCOMPLETE` with one criterion met; it remains a
truthful record of that day and is not edited.

---

## 1. Header

| field | value |
|---|---|
| absolute tree | `/Users/sicarii/anubis-worktrees/phase2-unified-walker` |
| branch | `phase2/unified-value-flow` |
| repository | `https://github.com/AnubisQuantumCipher/anubis-lang` (PUBLIC) |
| default branch | `main` |
| `main` HEAD at writing | `5ff1e87f95eed8b402729efdb131e848797e8f48` |
| immutable pin (slice evidence) | `vm/pins/anubis-bad018faeba7-src-032d9de847ea` |
| baseline pin (verdict-diff old side) | `vm/pins/anubis-149035b30c11-src-a88353570172-release`, SHA-256 `149035b30c114a4bd84235341e89e65da0bcb12760a6015487dac731f475da02` |
| rustc | `1.97.0-nightly (82bee9650 2026-05-09)` (hosted attestation) |
| cargo | `1.97.0-nightly (a343accce 2026-05-08)` (hosted attestation) |
| z3 | `4.15.4` (hosted attestation) |
| elan / Lean | `elan 4.2.3 (b6cec7e10 2026-06-08)` / `Lake 5.0.0-src+8c9756b (Lean 4.32.0)` |

The pins named above are technical pins for slice evidence. The release pin required by criterion 5
is a separate, clean, commit-bound artifact and is discussed in §2 and §9.

---

## 2. Exit criteria

The literal criteria are those in the controlling handoff. One row per criterion, with the exact
command, the observed line, and the immediate exit code.

### 1. CI green on the default branch; all failing gates fixed — **MET**

```
$ gh run view 30986778239 --json status,conclusion --jq '"\(.status)/\(.conclusion)"'
completed/success
```

Artifact `hosted-gate-report`, downloaded and read directly:

```
$ jq -r '{verdict,total,pass,fail,skip,external,tree_state}' gate_report.json
{ "verdict": "HOSTED_PASS", "total": 29, "pass": 28, "fail": 0,
  "skip": 0, "external": 1, "tree_state": "clean" }

$ grep -E '^(ref|github_sha|git_tree)=' attestation_identity.txt
ref=refs/heads/main
github_sha=3ccb735c0e04f3d8356925e2618fb894d79d295e
git_tree=e760bb215b24e94a218d0ed1dae3c2579d322790
```

`ref=refs/heads/main` is the load-bearing field: this is a run on the default branch, not a branch
that happened to share a commit. The single non-PASS row is `G9_poc_kit`, which is `EXTERNAL` by
design (§6 of the trust boundary) and is never counted as a pass.

`main` has since advanced to `5ff1e87f` (PR #3 merged), and its own push run `30992886117`
completed `success`. The criterion is therefore evidenced twice on the default branch, at two
different commits.

### 2. `main` exists, is default, is protected, requires a green hosted gate to merge — **MET**

```
$ gh api repos/AnubisQuantumCipher/anubis-lang --jq '"default=\(.default_branch) visibility=\(.visibility)"'
default=main visibility=public

$ gh api repos/AnubisQuantumCipher/anubis-lang/branches/main --jq '"sha=\(.commit.sha) protected=\(.protected)"'
sha=5ff1e87f95eed8b402729efdb131e848797e8f48 protected=true

$ gh api repos/AnubisQuantumCipher/anubis-lang/rules/branches/main --jq '[.[].type]|join(",")'
deletion,non_fast_forward,pull_request,required_status_checks
```

Ruleset id `20440878`, `enforcement=active`, **`bypass_actors` empty**. The required check is
`hosted-gate-witness` bound to `integration_id=15368` (the GitHub Actions app), with
`strict_required_status_checks_policy=true` so a branch must be current with `main` before merging.

`required_approving_review_count` is **0**. That is a deliberate, disclosed choice: this is a
single-maintainer repository, and requiring an approval nobody else can give would deadlock the
branch rather than protect it. The protection that is real here is the green-gate requirement and
the no-force-push / no-delete rules.

Observed working: pull requests #2, #3 and #4 all reported `BLOCKED` until their required check
completed.

### 3. `CHANGELOG.md`, `CODEOWNERS`, `.gitattributes`, `dependabot.yml` on the operative default — **MET**

```
$ gh api "repos/AnubisQuantumCipher/anubis-lang/contents/<path>?ref=main" --jq '.sha, .size'
CHANGELOG.md                       PRESENT sha=37cccfdf size=3025
.github/CODEOWNERS                 PRESENT sha=84d31dac size=2392
.gitattributes                     PRESENT sha=5eb99963 size=985
.github/dependabot.yml             PRESENT sha=45dcd9cd size=2231
.github/PULL_REQUEST_TEMPLATE.md   PRESENT sha=895b2dfc size=2717
```

Queried against `?ref=main` on the remote, not the local worktree — the criterion is about the
operative default branch, and a local file proves nothing about it.

### 4. Every Phase 2 slice lands as its own PR with pasted gate evidence — **MET for slices 1–2; the criterion is standing**

- PR #2 — <https://github.com/AnubisQuantumCipher/anubis-lang/pull/2> — Phase 2 slices 1, 2 and 3,
  with the full evidence table pasted in the description (verdict diff, fixture suites, library
  tests, fmt/clippy).
- PR #3 — <https://github.com/AnubisQuantumCipher/anubis-lang/pull/3> — merged as `5ff1e87f`.
- PR #4 — <https://github.com/AnubisQuantumCipher/anubis-lang/pull/4> — release packaging lane.

This criterion cannot be "finished", only kept. It is met in the only way it can be: the workflow
exists, is enforced by the ruleset rather than by intention, and the first two Phase 2 slices went
through it. A future slice that bypassed it would violate the criterion again.

### 5. At least one GitHub Release with binary and evidence bundle attached — **NOT MET**

```
$ gh api repos/AnubisQuantumCipher/anubis-lang/releases --jq 'length'
0
$ gh api repos/AnubisQuantumCipher/anubis-lang/tags --jq 'length'
0
```

This is the one open criterion and the reason the sign-off below is `INCOMPLETE`.

What exists: the packaging lane (`scripts/build_public_release.sh` +
`scripts/verify_public_release.py`, PR #4) and a verified clean commit-bound release pin mechanism
(`publish_pin.sh --release`, confirmed with `--verify-release` rc 0). A release pin was built from
`3ccb735c` and its leak scan is in §5.

What is missing: the release must be built from the **final** `main`, and `main` is still moving
(PR #2 and #4 in flight). Publishing from a superseded commit would attach a binary that is not the
code at the tag. `docs/evidence/PHASE_1_COMPLETION_2026-07-31.md` and the release contract both
forbid attaching the older `vm/pins/anubis-51f4a964347a`, which is dirty-tree Phase 1 evidence.

### 6. Self-hosted runner registered, or the sealed job explicitly documented as out-of-CI — **MET (the second branch)**

```
$ gh api repos/AnubisQuantumCipher/anubis-lang/actions/runners --jq '.total_count'
0
```

Zero runners is the *chosen* design, not a gap. `docs/CI_TRUST_BOUNDARY.md` documents it explicitly:

> No persistent self-hosted runner is part of the Phase-1.5 design. The daily signed-in Mac must not
> execute public-repository branch code as a GitHub runner. The former active Metal workflow and the
> queued sealed-VZ job were removed from `.github/workflows/`; a queued, skipped, or absent runner is
> never counted as PASS.

It also states the minimum bar a future runner must clear, and the exact operator-run entry points
for the sealed VZ and Metal lanes. That satisfies the criterion's second branch honestly: the job
is documented as out-of-CI and is never presented as passing.

### 7. `.git/auto-push.log` reviewed and commit-equals-publish understood — **MET**

The hook at `.git/hooks/post-commit` background-pushes commits on `a-plus-maturity/*`. Every branch
created in this phase (`phase2/*`, `fix/*`, `release/*`) is outside that glob, so no commit in this
phase could auto-publish. Publication was performed deliberately and explicitly through the GitHub
Git Data API, one named ref at a time.

---

## 3. RED before GREEN

Every change in this phase has a recorded failure before its fix.

| # | RED | GREEN |
|---|---|---|
| 1 | Default branch `a-plus-maturity/20260705-1649`, CI run `30177034579` = `11/15 passed, 4 failed` | `main` created at the green commit; run `30986778239` = `HOSTED_PASS 28/29` on `refs/heads/main` |
| 2 | `gh api .../rulesets --jq 'length'` → `0`; branch protection endpoint `404 Branch not protected` | ruleset `20440878` active, four rules, zero bypass actors |
| 3 | `w1_unannotated_param.anb` / `w2_unannotated_return.anb`: `check_rc=0`, `run` printed `42` | both `check_rc=1` with `ANUBIS_SECRET_EXFILTRATION` |
| 4 | `in_if.anb` / `in_for.anb`: `check_rc=0`, `run` completed calling `callee(-1)` | both `check_rc=1` with `ANUBIS_ASSERTION_DISPROVED` |
| 5 | slice-1 CI run `30989379521` = `Overall: FAIL (27/29 passed, 1 failed)`, `G16_docs_drift` | docs reconciled; `run_docs_drift_gate.sh` rc 0, `PASS (36 stamps checked, 0 drift)` |
| 6 | release binary carried `/Users/sicarii/anubis-lang` (1 `strings` hit) | constant removed (PR #3, merged `5ff1e87f`) |
| 7 | `d_place_assign.anb`: `b.f = key; print(b.f())` `check_rc=0`, `run` printed `42` | `check_rc=1` with `ANUBIS_SECRET_EXFILTRATION` |
| 8 | slice 1 wrongly REJECTED a method printing a public field (`rc=1 ANUBIS_SECRET_EXFILTRATION`) — a false rejection found by review, not by the corpus | `rc=0`; guard fixture committed |

Row 5 is the most useful one: the local suites were all green and CI still failed. The gate was
right and the local battery was incomplete.

---

## 4. Over-rejection guards

Every enforcing change ships an accept-side fixture, and the whole corpus was diffed.

```
$ /usr/bin/python3 -I -B scripts/phase1_verdict_diff.py \
    --old vm/pins/anubis-149035b30c11-src-a88353570172-release \
    --new vm/pins/anubis-81f11e11b770-src-1b8864f8ff5b --root . --out out/p2_final_diff.json
VERDICT_DIFF_V2 verdict=FAIL total=939 flips=6 timeouts=0 rc_changes=6
```

`verdict=FAIL` is the tool refusing to bless flips automatically. All four were inspected:

```
$ jq -r '.acceptance_flips[]|"\(.old.class) -> \(.new.class)   \(.fixture)"' out/p2_final_diff.json
ACCEPT -> REJECT   examples/security/contract_carried_guard_always_true_rejects.anb
ACCEPT -> REJECT   examples/security/contract_carried_guard_for_range_rejects.anb
ACCEPT -> REJECT   examples/security/place_assigned_fn_identity_secret_rejects.anb
ACCEPT -> REJECT   examples/security/place_assigned_fn_identity_taint_rejects.anb
ACCEPT -> REJECT   examples/security/unannotated_param_declared_secret_field_rejects.anb
ACCEPT -> REJECT   examples/security/unannotated_return_declared_secret_field_rejects.anb
```

All six are this branch's own `EXPECT: FAIL` fixtures — the intended closures. **Zero unintended
flips across 939 files.** (The final run measures 939 files against pin
`anubis-81f11e11b770-src-1b8864f8ff5b`; earlier runs in this phase measured 933 and 938 before
later fixtures were added.)

Accept-side guards, all `EXPECT: PASS` and all passing:
`unannotated_param_plain_field_accepts`, `unannotated_return_plain_field_accepts`,
`unannotated_param_declassify_accepts`, `contract_carried_dead_branch_accepts`,
`contract_carried_empty_range_accepts`, `contract_carried_guard_valid_arg_accepts`,
`place_assigned_fn_identity_plain_accepts`, `place_assigned_fn_identity_declassify_accepts`,
`unannotated_param_method_namespace_accepts`.

The `declassify` and dead-branch guards are the load-bearing ones. The dead-branch guard is what
caught the rejected solver-assumption design during slice 2 (§10).

---

## 5. Falsification

**Slice 1 — unannotated place types.** Direct form (`print(b.k)` on a local of known type) REJECTED;
laundered forms through an unannotated parameter and an unannotated return ACCEPTED and printed the
secret at runtime. Both closed. Alternate carriers attempted and still open: a formal reached only
through a function value, and several plain candidate types reaching one formal.

**Slice 2 — carried contract guards.** Direct form REJECTED; `if 1 == 1`, `for i in 0..1` ACCEPTED
and executed the violating call. Both closed. Dead-branch twins (`if 1 == 2`, `for i in 0..0`)
confirmed still ACCEPTED — the fix does not over-reject. A non-constant guard (`if n > 0`) remains
ACCEPTED; that is unchanged prior behavior, disclosed, not closed.

**Slice 3 — place-assigned callable identity.** Direct call, local alias, higher-order application,
and struct-literal construction all REJECTED; `b.f = key; print(b.f())` ACCEPTED and printed the
secret at runtime. Closed. The integrity twin (`b.f = dirty; shell(b.f())`) shows the same
structural asymmetry but is **not** runtime-provable — `shell` has no run-lane lowering, so the
runtime refuses with a structured `ANUBIS_UNSUPPORTED_NATIVE_LOWERING` rather than leaking. The
fixture says exactly that rather than implying a leak it cannot demonstrate.

**Builtin laundering — a NEGATIVE result, recorded because it refutes a claim.** A read-only scout
reported that 157 stdlib builtins are "implicitly clean in middle.rs analysis … all classification
sites use fail-open fallback (unknown = clean)", implying a broad laundering surface. That was
tested rather than believed. Every builtin name in `run.rs` was swept in both directions,
arities 1–3, plus arities 4–6 for the high-arity crypto entries:

```
integrity:      shell(NAME(x))  with x = input()
  172 names — 153 ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY, 15 arity, 4 unknown, 0 ACCEPTED
confidentiality: print(NAME(k)) with k a secret<i64>
  172 names — 150 ANUBIS_SECRET_EXFILTRATION, 3 tainted-sink, 15 arity, 4 unknown, 0 ACCEPTED
high-arity crypto (chacha20_poly1305_open/seal, hkdf_sha256, pbkdf2_hmac_sha256), arity 4
  4/4 ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY
```

**Zero laundering routes.** The remaining untested names are zero-argument builtins, which cannot
launder an argument by construction. The scout's claim is refuted for single-hop application; no
slice was written for a defect that does not exist.

Break attempts made in this phase: 2 place-type shapes, 4 contract-guard shapes, 2 dead-branch
controls, 6 callable-identity shapes, 516 builtin sweep programs, 4 high-arity crypto programs. Survivors: the non-constant
guard case, the polymorphic parameter case, and the function-value reachability case — all named in
§9.

---

## 6. Audit rerun

**SKIPPED — the instrument is inadmissible and Phase 2.0 has not repaired it.** The
completeness-audit harness binds mutable `./target/release/anubis`, invites host runtime-effect
witnesses contrary to VZ isolation, and uses stale CLAIMS numbering. Running it would produce a
number this report is not entitled to cite. No substitute is claimed.

---

## 7. Convergence metrics

Phase start, clean tree at `3ccb735c`:

```
middle/mod.rs lines                           28604   strictly decreasing (Phase 2+)
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  1934   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
_ => in label-lane walkers                        7   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
walker families                                   4   non-increasing, → 1
Expr variants: 29   Stmt variants: 15
PHASE_METRICS: OK
```

Phase end is recorded in the Phase 2 slice reports rather than here; this phase's changes were to
GitHub state, not to the walker structure. **`middle/mod.rs` grew** (28,604 → ~29,600) because
slices 1–3 ADD analysis rather than removing duplication. That moves the headline metric the
wrong way and is stated plainly rather than omitted: the "strictly decreasing" target belongs to the
unification slices (3–5), which delete duplicated lanes. Slices 1 and 2 were chosen first because
they close runtime-proven false accepts, and a smaller file that still leaks is not the goal.

---

## 8. Seal and CI

| lane | result |
|---|---|
| hosted CI, `main` @ `3ccb735c`, run `30986778239` | `HOSTED_PASS`, 28 PASS / 0 FAIL / 0 SKIP / 1 EXTERNAL, `tree_state=clean`, `ref=refs/heads/main` |
| hosted CI, `main` @ `5ff1e87f`, run `30992886117` | `completed/success` |
| PR #2 @ `b1e743e0`, run `30992814305` | `HOSTED_PASS`, 28 PASS / 0 FAIL / 0 SKIP / 1 EXTERNAL, `tree_state=clean`, `github_sha=b1e743e0383c2404a9b9f3a6de949cc7f8d7b163`, `git_tree=5cc1247437485f336abacdca9f1f12e504f3742d` |
| PR #4 @ `b90c3bd3` (updated onto `main`) | **PENDING at the time of writing** — not counted |
| this report's own commit | **not self-citable.** Adding this file changes the tree, so the run that grades the commit containing this sentence cannot be named inside it. That run is the external binding for this report. |
| VM seal (Tart/VZ) | **SKIPPED — not run in this phase.** No VM receipt is claimed. The last VM seal on record belongs to Phase 1 and is bound to a different pin and a different source epoch; citing it here would attribute Phase 1 evidence to Phase 1.5 work. |
| Offensive/VZ gate | **SKIPPED — not run in this phase.** Same reason. |
| Metal | **SKIPPED — out of CI by design** (`docs/CI_TRUST_BOUNDARY.md`). |

`HOSTED_PASS` is never a VZ proof. A stock GitHub macOS runner cannot run Tart.

---

## 9. What was not verified

- **The Release (criterion 5) does not exist.** No tag, no release, no published asset, no
  independent asset review. The packaging lane is written and syntax-checked but has never
  completed a full run, because by construction it refuses to run against an uncommitted tree and
  the final `main` is not yet settled.
- **No VM seal and no offensive/VZ gate were run in this phase.** The self-host fixpoint was not
  re-measured.
- **The completeness-audit harness was not run** (§6).
- **`docs/CLAIMS.md` item 21 is not closed.** Still open: unannotated polymorphic parameters
  (several plain candidate types reaching one formal), formals reached only through a function
  value, non-constant branch guards in carried contract discharge, the impl-method arm of
  `place_struct_type`, place-assignment parity in the reduced walkers, and builtin-result callable
  identity. Slice 3 closes the *named-field* case of place-assignment identity; a callable reached
  through a **container element** rather than a named field path is unchanged, and an `Unknown`
  identity set is still not charged.
- **`anubis run` is not fail-closed as a whole.** The ~213-builtin domain/arity/wrong-type/I/O
  surface remains unenumerated. This phase's sweep tested *laundering*, not runnability.
- **The bare `anubis` alias hazard** is documented, not fixed.
- **PR #1** (`a-plus-maturity/safe-mode-trust-spine-20260725` → the old default) is still open and
  was not triaged. It targets a branch that is no longer the default.
- **Duplicate CI runs.** `on: push:` is unfiltered, so every PR branch push produces two runs that
  both report `hosted-gate-witness`, doubling cost and required checks. Identified, not fixed.

---

## 10. What was gotten wrong

- **I read `tail`'s exit code as cargo's, twice.** `cargo build … | tail -40` reported success on a
  build that failed with six errors. The repo's own rule — capture the exit code on the very next
  line — exists for exactly this, and I violated it. Fixed by writing `cmd > log; echo "RC=$?"`.
  The same mistake recurred with `publish_pin.sh --verify-release | tail`.
- **I called `publish_pin.sh --verify-release` with a path argument.** It takes none; the extra
  argument makes it print usage and exit 2, which my first draft of the packager would have read as
  a verdict. Found by reading, fixed before it ran.
- **`[[ -e "$STAGE" ]] && die …` under `set -e`** makes the *good* case the statement's failing
  status, aborting the script before staging. Also found by reading.
- **The solver-assumption design for slice 2 over-rejected.** Passing branch guards down as SMT
  assumptions is the more precise design and I implemented it first. It returned
  `ANUBIS_ASSERTION_UNPROVEN` on `if 1 == 2` and `for i in 0..0` — trading a fail-open for a false
  rejection, which is worse. Replaced with constant folding, which cannot over-reject. The failure
  and the reason it failed are recorded in the code, the commit, and the PR rather than quietly
  dropped.
- **A subagent reported a findings file it never wrote.** `PHASE2_INVESTIGATION_FINDINGS.md` did not
  exist. Its line citations were still useful, but only after checking each against the source. One
  of its headline claims — `place_struct_type` "has no `Expr::Index` arm" — was **false**: the arm
  exists at `mod.rs:8569`, added by an earlier slice. The claim came from `docs/CLAIMS.md`'s
  historical root-cause table, which the scout read as current.
- **Its `is_io_taint_source`/builtin fail-open claim was refuted by measurement** (§5). Had I
  written a slice against it, I would have "fixed" a defect that does not exist.
- **My own slice-1 code introduced a FALSE REJECTION, and the corpus did not catch it.** CodeRabbit
  did, on PR #2. `select_param_place_hints` looked up the bare-name candidate map for impl methods
  too, so a method sharing a name with a free function inherited that function's argument types by
  index. A legitimate program — a method printing a public `i64` field — was rejected with
  `ANUBIS_SECRET_EXFILTRATION`. This is `AGENTS.md` law violated verbatim ("never merge
  namespaces"), by me, in a slice whose entire premise is that written-down labels must be read by
  the right consumer. **Every one of the 344 security fixtures passed while this was live.** A green
  corpus is exactly when a new defect is least visible; the guard is now committed as
  `unannotated_param_method_namespace_accepts.anb`.
- **A second review finding did not reproduce, and I did not pretend it did.** The module-descent
  gap is real in the abstract — `collect_param_place_candidates` skipped `Item::Module` while
  `register_program_surface` recurses into it — but I could not construct a witness: a multi-file
  `import` program already rejected before the change. It ships as hardening that can only add
  candidates from real call sites, explicitly not as a closed defect.
- **I nearly falsified historical records.** The obvious response to `G16_docs_drift` is to renumber
  every flagged line. Most flagged lines are dated receipts, and renumbering them would have made
  them lie about what a past epoch measured.

---

## 11. Landing state

Code and docs are in separate commits throughout.

| ref | commit | content |
|---|---|---|
| `main` | `3ccb735c` → `5ff1e87f` | created at the green commit; PR #3 merged |
| `phase2/unified-value-flow` | `86abfc6b` | slice 1 code + 6 fixtures |
| | `8b8e54c0` | slice 2 code + 6 fixtures |
| | `b1e743e0` | docs stamp reconciliation (docs only) |
| | `cf3b2f34` | this report (docs only) |
| | `3b3d932c` | slice 3 code + 5 fixtures |
| | `ddb86919` | slice 3 docs restamp (docs only) |
| | `5b1fd8ba` | report extended for slice 3 (docs only) |
| | `ff7e74d6` | review fixes + namespace regression guard |
| | `58dfab9f` | docs restamp for that guard (docs only) |
| `fix/release-binary-operator-path` | `09fbfd71` | merged → `5ff1e87f` |
| `release/public-packaging-lane` | `ddd21a8d` | release lane, open |

Unrelated work untouched: the shared checkout `/Users/sicarii/anubis-lang` was never staged,
committed, reset, or cleaned. All work happened in `~/anubis-worktrees/phase2-unified-walker`. The
legacy branches `a-plus-maturity/*` were neither deleted nor force-pushed.

Rollback: the default branch can be pointed back at `a-plus-maturity/20260705-1649` with one API
call, and ruleset `20440878` can be deleted with one more. Neither `main` nor any commit needs to be
removed. What cannot be rolled back is the public visibility of the pushed commits — they are in a
public repository and must be treated as published.

---

## 12. Sign-off

**PHASE 1.5 INCOMPLETE — six of seven criteria met; blocked on criterion 5 (a published Release).**

Met: 1 (CI green on `main`), 2 (`main` default + protected + gate-required), 3 (operating files on
the default), 4 (Phase 2 slices landing as PRs with pasted evidence), 6 (sealed lanes documented
out-of-CI), 7 (auto-push understood).

Not met: 5. Zero releases, zero tags. The packaging lane exists and the clean commit-bound pin
mechanism is verified, but no release has been built, scanned, independently reviewed, or published,
and it must be built from a settled `main`.

**Recommendation.** Land PR #2 and PR #4, let `main` settle, then run the release lane end to end:
`publish_pin.sh --release` → `--verify-release` → `build_public_release.sh` → leak gate →
`verify_public_release.py` positive and one-byte-tamper negative → independent read-only asset
review → publish as an honestly-named prerelease (`v0.1.0-phase1-preview.1`), never as 1.0. The
1.0 tag belongs to Phase 7.

This report describes work that is **committed and pushed to a public repository**. It is not
"shipped": there is no release, no tag, no distributed artifact, and no VM seal for this phase.
