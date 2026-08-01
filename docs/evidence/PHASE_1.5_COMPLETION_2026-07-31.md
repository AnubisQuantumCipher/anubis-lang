# Phase 1.5 completion report — GitHub as system of record

**Verdict: PHASE 1.5 INCOMPLETE.**

The Phase 1.5 reconciliation candidate is clean, committed, public on its bounded branch, and green
under both a clean local hosted audit and the exact-SHA GitHub hosted witness. It is not the
operative default, protected `main` does not exist, the operating files are not on the operative
default, no Release exists, and the forward-looking Phase 2 PR criterion cannot yet be satisfied.
Those distinctions are load-bearing. No Phase 2 work began.

Phase 1 remains `BOUNDED COMPLETE / ACTIVATED` for its exact historical technical epoch. This
report does not turn that dirty-tree pin into a tagged release artifact and does not claim that
Anubis is globally complete, shipped, or free of unknown defects.

## 1. Header and exact identity

```text
verification UTC:      2026-08-01T07:52:56Z
isolated worktree:     /Users/sicarii/anubis-worktrees/phase15-reconciliation-20260801
branch:                codex/phase15-reconciliation-20260801
base:                  0e910c9bb2e83438696eaaf0f49d0e3c5e658960
graded candidate HEAD: e31a9e494fe9d26ff131e3f31167ded75f0193e9
graded candidate tree: 7e777477495fd7deb58210430b1253b28a1bc6c8
remote branch ref:     e31a9e494fe9d26ff131e3f31167ded75f0193e9
local base..HEAD diff: 128 files; 22,480 insertions; 2,228 deletions
GitHub API per-file:   128 files; 15,895 additions; 1,766 deletions
pre-report worktree:   clean; 0 dirty entries
report-draft dirty:    1 entry
owned report path:     docs/evidence/PHASE_1.5_COMPLETION_2026-07-31.md

code/config commit:    7d68b1c4880e4f4e1f53f2e6afd41e2ac5187561
code/config tree:      43f2b668a942a389500690405d15c3b8fa7bac71
docs commit:           2160f4a41c8c6ecb060e389d896fa04d8387efed
docs tree:             a9d3e4876007acd29c2e760fcb1c8bb5f2ee789d
G28 follow-up commit:  e31a9e494fe9d26ff131e3f31167ded75f0193e9
G28 follow-up tree:    7e777477495fd7deb58210430b1253b28a1bc6c8
report-only commit:    intentionally not self-embedded; this file is its sole intended path
```

The local Git diffstat is canonical for the candidate. GitHub's compare API reports different
per-file patch statistics; it is retained only as a separately labeled API observation:

```text
$ git diff --shortstat 0e910c9bb2e83438696eaaf0f49d0e3c5e658960..e31a9e494fe9d26ff131e3f31167ded75f0193e9
128 files changed, 22480 insertions(+), 2228 deletions(-)
rc=0

$ gh api 'repos/AnubisQuantumCipher/anubis-lang/compare/0e910c9bb2e83438696eaaf0f49d0e3c5e658960...e31a9e494fe9d26ff131e3f31167ded75f0193e9' --jq '{file_count:(.files|length),additions:([.files[].additions]|add),deletions:([.files[].deletions]|add)}'
{"additions":15895,"deletions":1766,"file_count":128}
rc=0
```

The controlling 711-line handoff was read in full and reverified at SHA-256
`ea05039fe52f6ae1142cb0894209c47402242e7590793e649310b8dd085604ad`.

Binary status is intentionally non-release-grade:

```text
immutable candidate pin:  NONE
candidate pin source match: NOT AVAILABLE — no lead-published immutable candidate pin exists
graded mutable binary:    /Users/sicarii/anubis-worktrees/phase15-reconciliation-20260801/target/release/anubis
graded binary SHA-256:    71092168defc3dfa5a55b96347ad944575b906c20ab16e5d6a6795be5df307fe
graded binary mtime/size: 2026-08-01T07:06:10Z / 99,318,224 bytes
graded binary source:     built by G4 from clean e31a9e49... before this report; mutable and not a release asset

historical immutable pin: /Users/sicarii/anubis-lang/vm/pins/anubis-51f4a964347a
historical pin SHA-256:   51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027
historical pin mtime/size:2026-07-31T12:53:23Z / 99,433,808 bytes
historical source match: bounded Phase 1 technical epoch only; does not match this candidate/report
```

Toolchain snapshot, every command rc 0:

```text
Xcode 26.6 / build 17F113
rustc 1.97.0-nightly (82bee9650 2026-05-09)
cargo 1.97.0-nightly (a343accce 2026-05-08)
Z3 version 4.15.4 - 64 bit
Lake version 5.0.0-src+f3b06c7 (Lean version 4.32.2)
Lean 4.32.2, arm64-apple-darwin24.6.0, commit f3b06c705e6c85f5314019d5d3baab0fec5b580c
Xcode Python 3.9.6
jq-1.7.1-apple
gh 2.83.2
Darwin 25.5.0 arm64
```

The independently re-derived source manifest for the clean graded candidate, before this report
entered the manifest, was:

```text
schema:        anubis.pin-source-manifest.v2
policy schema: anubis.pin-manifest-policy.v2
source count:  1641
list SHA-256:  d4576581c35b3f36d2027d2a7f401e9146663bb9d8adee487b6c1cc92292087e
tree SHA-256:  4e08ee123013a128cd3efaf4b952ec91a557197421dc50d285b9328f62c1aadb
policy SHA-256:83f24fb1199b0d674c584fee9756097b2167a91b62a1aec1b10ca44f2827847f
```

The final manifest digest is deliberately not embedded in this source-manifested report: changing
the report would change that digest. Git commit/tree identity and the subsequent report-only CI run
are the non-self-referential binding.

Critical candidate hashes:

```text
scripts/publish_pin.sh                         301f1ca27b47305568acd8b07682ab9348411d5948e5973a19cb9d92a55a86b8
scripts/test_corpus_inventory_binding.sh       0cda70e57b46e61a5e5c0128bb7361a8ced5a148c13f23ad90947740b778471a
scripts/lib/host_resource_guard.sh             5c753b54ae9b58d7ea4b5cc50397449a3ae0240fe7e1d02e29e9527d9b63a5aa
scripts/phase1_verdict_diff.py                 02dbba3522ad4a3ccb1d0199b821f5c048d0fcf8f901c8c7addda9a1773aba4b
docs/evidence/PHASE_1_COMPLETION_2026-07-31.md  ec70e1b705b25db9c908fab6d7bda9b9a12a1017bf94796045dc5063ff97293e
```

## 2. Literal exit criteria

An rc of zero means the read-only query executed; it does not turn returned red state into PASS.
The live GitHub census was refreshed after the branch push, and the exact candidate CI run completed
at `2026-08-01T07:50:50Z`. Every row below contains the exact command, decisive verbatim output,
immediate rc, and verdict.

| # | Literal criterion | Exact command, decisive output, immediate rc | Verdict |
|---|---|---|---|
| 1 | CI green on the default branch; all failing gates fixed | `gh run view 30177034579 --repo AnubisQuantumCipher/anubis-lang --json conclusion,headSha,url --jq '[.conclusion,.headSha,.url] \| @tsv'; rc=$?`<br>`failure` `4a361b6e2d55f0769cece575fb99d389385dfca6` `https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/30177034579`; `rc=0`<br>Candidate cross-check: identical command for run `30689417765` returned `success` `e31a9e494fe9d26ff131e3f31167ded75f0193e9` and its URL; `rc=0`. Candidate green is not default green. | **FAIL** |
| 2 | `main` exists, is default/protected, and requires the hosted gate | `gh api repos/AnubisQuantumCipher/anubis-lang --jq '[.default_branch,.visibility] \| @tsv'; rc=$?` → `a-plus-maturity/20260705-1649 public`; `rc=0`.<br>`gh api repos/AnubisQuantumCipher/anubis-lang/branches --paginate --jq '.[] \| [.name,.commit.sha,.protected] \| @tsv'; rc=$?` → four rows, each ending `false`; `rc=0`.<br>`gh api repos/AnubisQuantumCipher/anubis-lang/branches/main --jq '[.name,.commit.sha,.protected] \| @tsv' 2>&1; rc=$?` → `gh: Branch not found (HTTP 404)`; `rc=1`.<br>`gh api repos/AnubisQuantumCipher/anubis-lang/rulesets --jq 'length'; rc=$?` → `0`; `rc=0`. | **FAIL** |
| 3 | `CHANGELOG.md`, `CODEOWNERS`, `.gitattributes`, and `dependabot.yml` are on the operative default | `for phase_path in CHANGELOG.md .github/CODEOWNERS .gitattributes .github/dependabot.yml; do gh api "repos/AnubisQuantumCipher/anubis-lang/contents/$phase_path?ref=a-plus-maturity%2F20260705-1649" --jq '.sha' 2>&1; rc=$?; echo "$phase_path rc=$rc"; done` → each probe printed `gh: Not Found (HTTP 404)` followed by its exact path and `rc=1`. Candidate-ref probes for the same four paths returned rc 0. | **FAIL** |
| 4 | Every Phase 2 slice lands as its own PR with pasted gate evidence | `gh pr list --repo AnubisQuantumCipher/anubis-lang --state all --json number,headRefName,baseRefName,state,isDraft --jq '.[] \| [.number,.headRefName,.baseRefName,.state,.isDraft] \| @tsv'; rc=$?` → `1 a-plus-maturity/safe-mode-trust-spine-20260725 a-plus-maturity/20260705-1649 OPEN true`; `rc=0`.<br>`gh pr list --repo AnubisQuantumCipher/anubis-lang --state all --head codex/phase15-reconciliation-20260801 --json number --jq 'length'; rc=$?` → `0`; `rc=0`. No Phase 2 work or PR exists. | **PENDING / FUTURE-DEPENDENT** |
| 5 | At least one GitHub Release has binary and evidence bundle attached | `gh release list --repo AnubisQuantumCipher/anubis-lang --limit 100 --json tagName --jq 'length'; rc=$?` → `0`; `rc=0`.<br>`gh api repos/AnubisQuantumCipher/anubis-lang/git/matching-refs/tags --jq 'length'; rc=$?` → `0`; `rc=0`. | **FAIL** |
| 6 | Runner registered, or sealed job explicitly documented out of CI | `gh api repos/AnubisQuantumCipher/anubis-lang/actions/runners --jq '.total_count'; rc=$?` → `0`; `rc=0`.<br>`gh api repos/AnubisQuantumCipher/anubis-lang/actions/workflows --jq '.workflows[] \| select(.state=="active") \| [.id,.name,.path] \| @tsv'; rc=$?` → `310117105 anubis-ci .github/workflows/ci.yml` and `320108761 metal-prove .github/workflows/metal-prove.yml`; `rc=0`.<br>Default trust-doc GET → `gh: Not Found (HTTP 404)`; `rc=1`. Candidate trust-doc GET → `85b8353ee5ed97252b254649e78d9da68ac60099`; `rc=0`. Candidate contract PASS; operative activation absent. | **FAIL / ACTIVATION PENDING** |
| 7 | `.git/auto-push.log` reviewed and commit-equals-publish understood | From `/Users/sicarii/anubis-lang`: `shasum -a 256 .git/hooks/post-commit .git/auto-push.log; rc=$?` → `f893bad785ae0e9b77cd4fc7ea1e626ca26ce89b836e61af539dfed4f8072556` and `3716aa7467bfe6888d887651ecdde0b8cee812ccdbf16f22628cd5bae223e93f`; `rc=0`.<br>`git ls-remote --heads origin codex/phase15-reconciliation-20260801; rc=$?` → `e31a9e494fe9d26ff131e3f31167ded75f0193e9 refs/heads/codex/phase15-reconciliation-20260801`; `rc=0`. | **PASS** |

Raw criterion ledger, preserving shell pipes without Markdown-table escaping:

```text
# Criterion 1
$ gh run view 30177034579 --repo AnubisQuantumCipher/anubis-lang --json conclusion,headSha,url --jq '[.conclusion,.headSha,.url] | @tsv'
failure	4a361b6e2d55f0769cece575fb99d389385dfca6	https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/30177034579
criterion1_default_rc=0
$ gh run view 30689417765 --repo AnubisQuantumCipher/anubis-lang --json conclusion,headSha,url --jq '[.conclusion,.headSha,.url] | @tsv'
success	e31a9e494fe9d26ff131e3f31167ded75f0193e9	https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/30689417765
criterion1_candidate_rc=0

# Criterion 2
$ gh api repos/AnubisQuantumCipher/anubis-lang --jq '[.default_branch,.visibility] | @tsv'
a-plus-maturity/20260705-1649	public
criterion2_repo_rc=0
$ gh api repos/AnubisQuantumCipher/anubis-lang/branches --paginate --jq '.[] | [.name,.commit.sha,.protected] | @tsv'
a-plus-maturity/safe-mode-trust-spine-20260725	0e910c9bb2e83438696eaaf0f49d0e3c5e658960	false
a-plus-maturity/w2-1-exact-array-places-20260729	66fcade883936a3217a3bdfbf32ae424a9a95291	false
a-plus-maturity/20260705-1649	4a361b6e2d55f0769cece575fb99d389385dfca6	false
codex/phase15-reconciliation-20260801	e31a9e494fe9d26ff131e3f31167ded75f0193e9	false
criterion2_branches_rc=0
$ gh api repos/AnubisQuantumCipher/anubis-lang/branches/main --jq '[.name,.commit.sha,.protected] | @tsv' 2>&1
gh: Branch not found (HTTP 404)
criterion2_main_rc=1
$ gh api repos/AnubisQuantumCipher/anubis-lang/rulesets --jq 'length'
0
criterion2_rulesets_rc=0

# Criterion 3
$ for phase_path in CHANGELOG.md .github/CODEOWNERS .gitattributes .github/dependabot.yml; do gh api "repos/AnubisQuantumCipher/anubis-lang/contents/$phase_path?ref=a-plus-maturity%2F20260705-1649" --jq '.sha' 2>&1; rc=$?; echo "$phase_path rc=$rc"; done
gh: Not Found (HTTP 404)
CHANGELOG.md rc=1
gh: Not Found (HTTP 404)
.github/CODEOWNERS rc=1
gh: Not Found (HTTP 404)
.gitattributes rc=1
gh: Not Found (HTTP 404)
.github/dependabot.yml rc=1

# Criterion 4
$ gh pr list --repo AnubisQuantumCipher/anubis-lang --state all --json number,headRefName,baseRefName,state,isDraft --jq '.[] | [.number,.headRefName,.baseRefName,.state,.isDraft] | @tsv'
1	a-plus-maturity/safe-mode-trust-spine-20260725	a-plus-maturity/20260705-1649	OPEN	true
criterion4_all_rc=0
$ gh pr list --repo AnubisQuantumCipher/anubis-lang --state all --head codex/phase15-reconciliation-20260801 --json number --jq 'length'
0
criterion4_candidate_rc=0

# Criterion 5
$ gh release list --repo AnubisQuantumCipher/anubis-lang --limit 100 --json tagName --jq 'length'
0
criterion5_release_rc=0
$ gh api repos/AnubisQuantumCipher/anubis-lang/git/matching-refs/tags --jq 'length'
0
criterion5_tags_rc=0

# Criterion 6
$ gh api repos/AnubisQuantumCipher/anubis-lang/actions/runners --jq '.total_count'
0
criterion6_runners_rc=0
$ gh api repos/AnubisQuantumCipher/anubis-lang/actions/workflows --jq '.workflows[] | select(.state=="active") | [.id,.name,.path] | @tsv'
310117105	anubis-ci	.github/workflows/ci.yml
320108761	metal-prove	.github/workflows/metal-prove.yml
criterion6_workflows_rc=0
$ gh api 'repos/AnubisQuantumCipher/anubis-lang/contents/docs/CI_TRUST_BOUNDARY.md?ref=a-plus-maturity%2F20260705-1649' --jq '.sha' 2>&1
gh: Not Found (HTTP 404)
criterion6_default_doc_rc=1
$ gh api 'repos/AnubisQuantumCipher/anubis-lang/contents/docs/CI_TRUST_BOUNDARY.md?ref=codex%2Fphase15-reconciliation-20260801' --jq '.sha'
85b8353ee5ed97252b254649e78d9da68ac60099
criterion6_candidate_doc_rc=0

# Criterion 7, from /Users/sicarii/anubis-lang
$ shasum -a 256 .git/hooks/post-commit .git/auto-push.log
f893bad785ae0e9b77cd4fc7ea1e626ca26ce89b836e61af539dfed4f8072556  .git/hooks/post-commit
3716aa7467bfe6888d887651ecdde0b8cee812ccdbf16f22628cd5bae223e93f  .git/auto-push.log
criterion7_hash_rc=0
$ git ls-remote --heads origin codex/phase15-reconciliation-20260801
e31a9e494fe9d26ff131e3f31167ded75f0193e9	refs/heads/codex/phase15-reconciliation-20260801
criterion7_remote_rc=0
```

Default-content probes for `CHANGELOG.md`, `.github/CODEOWNERS`, `.gitattributes`,
`.github/dependabot.yml`, and `docs/CI_TRUST_BOUNDARY.md` each returned HTTP 404 / rc 1. The same
five candidate-ref probes returned rc 0. The candidate workflow blob is `3993dbd9...`; its CI trust
document blob is `85b8353e...`; candidate `metal-prove.yml` is absent. On the operative default,
`metal-prove` remains active as workflow ID `320108761`, with workflow blob `0770bb7e...`.

## 3. RED before GREEN

The evidence chain retains both failures; neither was rewritten as a pass.

1. Historical remote state was red: the old PR/push head produced `22/26 passed, 3 failed,
   0 skipped, 1 external`; the operative-default run `30177034579` remains a failure.
2. The first clean local hosted audit graded docs commit `2160f4a4...` / tree `a9d3e487...` and
   returned `FAIL (27/29 passed, 1 failed, 0 skipped, 1 external)`. G28 reported
   `CORPUS_INVENTORY_BINDING: 71 passed, 13 failed` because aggregate-only Metal bypass variables
   contaminated its synthetic release-publisher cases.
3. G28 was corrected without weakening the production publisher. The harness starts synthetic
   publisher cases from a clean release contract, then independently re-poisons
   `ANUBIS_SKIP_RISC0_METAL`, `RISC0_SKIP_BUILD_KERNELS`, and `R0_DISABLE_METAL`; each requires
   nonzero exit, its exact variable-specific `PIN_RELEASE_BUILD_ENV_DENIED`, unchanged `CURRENT`,
   and guard-before-Cargo ordering. Exact hosted-environment reproduction passed `88/88`, rc 0.
4. The second clean local audit graded exact commit `e31a9e49...` and returned
   `HOSTED_PASS (28/29 passed, 0 failed, 0 skipped, 1 external)`, rc 0.
5. GitHub run `30689417765` graded that exact SHA and succeeded. Its separate Lean step, full hosted
   aggregate, minimized-report validator, and artifact upload all passed.

```text
first local report SHA-256: 5a773390c2882276525cac5fa3f3ddcf5555eddbb445909b124ea0d55dfdf32d
first G28 log SHA-256:       76367d8b2a7a79d4cdde305c76f007b715aaf2c456a4ac53c9a236a2a158f617
green local report SHA-256: 27400e56f2ce4dcbeaac5273003b913124ef85cc735c9fdf4e8646c6e364ac31
green local log SHA-256:    69a11ce478a2a3fd5bd922fa815828d6411b6f7ffc2dc4758b6770cac017c284
green G28 log SHA-256:      4406fa2dff2ce0dc83e2c3d4b4576488b03b165b9360650da9905685549f2264
```

## 4. Over-rejection guards and verdict diff

The clean hosted candidate passed:

```text
workspace tests:             1215 passed
language fixtures:           253/253
security fixtures:           327/327
stdlib fail-closed:           104/104; timed_out=0
PCA:                          19/19
prove:                        11/11
native-authoritative corpus: 921 files; mismatches=0; disagreements=0
walker completeness:         0 findings
formal:                       162 theorems across 15 modules
```

These results exercise accept and reject fixtures, but they are not a newly generated old/new
Phase 1 verdict diff.

**Fresh old/new 921-row verdict diff: SKIPPED — no immutable candidate pin and deliberately selected
baseline/candidate pin pair existed.** No claim to the contrary is made. The historical Phase 1
result of 921 files, zero flips, and zero timeouts remains bounded to its `51f4...` technical epoch.
G18's 921-file native-versus-reference agreement is a different assertion and is not relabeled as
an old/new diff. This missing required over-rejection receipt remains an explicit closure gap.

## 5. Falsification and break attempts

Exactly **202 bounded automated break/fault cases** were executed in the three counted suites below:
`88 + 79 + 35 = 202`. They include positive instrument baselines as well as negative poisons; the
count is an executed-case count, not a vulnerability count. The final candidate survived all
202/202 after the RED G28 instrument/configuration failure in section 3 was corrected.

- G28: 88/88 corpus, manifest, publication-race, symlink, Git-environment, Cargo-environment,
  Python-isolation, immutable-pin, release-binding, and weakening-control cases.
- G29: 79/79 host-resource admission, watch, job-cap, LaunchAgent, teardown, sync, and evidence
  contract cases.
- verdict-diff harness: 35/35, including private pinned Z3 binding, isolated Python, helper-path
  binding, BASH environment poisoning, and opening/closing identity.
- Phase metrics ledger fault suite, seal ledger, freshness ledger, carrier totality, malformed
  fixture preflight, promise RED guards, docs drift, and walker-completeness checks.
- Full workspace tests were rerun from a clean committed tree after generator fixes; neither
  `tools/anubis/out` nor `compiler/hello_phase5.txt` appeared.
- The release-publisher source stayed byte-identical while the G28 test instrument changed.

Direct-form, alternate-carrier, and dead-branch semantic closure twins are **N/A — Phase 1.5 makes
no new language-closure claim**. The candidate contains enforcing changes inherited from the Phase 1
dirty tree, but this report does not promote them to a new closure claim because the fresh old/new
verdict diff and claim-specific twin mapping were skipped. Existing security accept/reject fixtures
ran 327/327; they are regression evidence, not a substitute for that missing mapping.

Crash-capable, Research, PoC, fuzz, exploit, agent, C2, offensive, and Apple-VZ execution were
**SKIPPED** in this phase. No host fallback occurred. G9 remained `EXTERNAL`; G14 was exactly the
non-executing hosted isolation witness, 5/5.

## 6. Independent audit reruns

Four independent read-only/adversarial lanes returned nonempty evidence:

1. Commit-split audit: APPROVE. It verified an exact code/config/fixture versus docs partition,
   no unknown/omitted/extra paths, clean index, whitespace checks, expected ignored roots, and
   absence of generated artifacts.
2. Verdict-diff integrity audit: 35/35; Ruff and Python compilation PASS; private Z3 `4.15.4`
   binding SHA-256 `ae6c8df33db9c9ae9a80b6044e77cd66529a141d8b25f0620f1e89b409594f48`;
   exact native inventory `921`; opening/closing binding unchanged.
3. Final G28 adversarial audit: APPROVE. It independently reproduced 88/88 under the hosted
   variables, confirmed all four new PASS rows, exact denials, unchanged `CURRENT`, pre-Cargo
   rejection, publisher hash stability, syntax/whitespace checks, and artifact absence.
4. Live GitHub audit: exact candidate ref matched; default/protection/rulesets/files/workflows,
   runners, PRs, tags, Releases, and exact-SHA runs were re-queried. It independently returned
   `PHASE 1.5 INCOMPLETE` with the same criterion mapping in section 2.

No lane performed a GitHub mutation or edited the final report.

## 7. Convergence metrics

Phase-start output, pasted from the actual `bash scripts/phase_metrics.sh` run, immediate rc 0:

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 224 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28604   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                            38   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  1934   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        7   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
phase_metrics_start_rc=0
```

Phase-end pre-report output, pasted from the actual `bash scripts/phase_metrics.sh` run, immediate
rc 0:

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-worktrees/phase15-reconciliation-20260801
commit    : e31a9e494fe9d26ff131e3f31167ded75f0193e9
branch    : codex/phase15-reconciliation-20260801
dirty     : 0 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28604   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                            38   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  1934   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        7   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
phase_metrics_end_rc=0
```

Phase 1.5 did not claim Phase 2 convergence. These open structural metrics remain Phase 2 work and
were not silently pulled forward.

## 8. Seal and CI

Local clean hosted witness:

```text
output:  out/phase15_hosted_candidate_green_20260801T065824Z
HEAD:    e31a9e494fe9d26ff131e3f31167ded75f0193e9
tree:    7e777477495fd7deb58210430b1253b28a1bc6c8
verdict: HOSTED_PASS
roster:  28 PASS, 0 FAIL, 0 SKIP, 1 EXTERNAL (G9), 29 total
rc:      0
```

Remote exact-SHA hosted witness:

```text
workflow/run:   anubis-ci / 30689417765
job/check:      hosted-gate-witness / 91341360589
URL:            https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/30689417765
event/ref:      push / refs/heads/codex/phase15-reconciliation-20260801
HEAD/tree:      e31a9e494fe9d26ff131e3f31167ded75f0193e9 / 7e777477495fd7deb58210430b1253b28a1bc6c8
conclusion:     success
duration:       34m14s
artifact:       hosted-gate-report / 8815499796 / 3613 bytes / expires 2026-08-15
roster:         HOSTED_PASS; 28 PASS; 0 FAIL; 0 SKIP; 1 EXTERNAL (G9); 29 total
```

Downloaded artifact verification returned rc 0 for its SHA-256 manifest, exact report predicate,
run ID, GitHub SHA, Git HEAD, and Git tree.

```text
MANIFEST.sha256:         e29e1445d468df1931d3b64349782151ac82726d7267908c527a900ea1a3ed9e
attestation_identity:    fe515304924a6a8b04104e288ffbb6439ff9432ba5261c3ed6bbbe1a46d25c05
gate_log:                d9b70839492dfa1eb93b9669e08c45801fc514195368f8a9909194be522d5e2f
gate_report:             7213a9599ba8d8d1530bf7b34cde7a604dab1083423b4b4590d2f428a0c8ab0d
profile_environment:     a95cab71814dda840858cc62e423063d43f0e72a2cfb872c5e95685d80eb4a2b
```

The Actions run emitted non-fatal Node.js action-runtime and untrusted Homebrew tap warnings; all
named workflow steps still concluded success. Those warnings are not hidden and are not gate FAILs.

Fresh non-hosted batteries were handled individually:

- **Phase 1 VM battery: SKIPPED — no immutable source-current candidate pin existed, so a new
  disposable-guest roster could not be honestly bound to this candidate.**
- **Offensive battery: SKIPPED — no Research/offensive execution was dispatched for Phase 1.5;
  mandatory VZ isolation forbids host fallback, and G9 remained `EXTERNAL`.**
- **Apple Metal battery: SKIPPED — stock hosted CI is not an Apple-Metal/VZ witness, no self-hosted
  runner is registered, and no separately authorized proof/release run occurred.**
- **Fresh 20/20 full seal: SKIPPED — its immutable-pin, VM, offensive, and old/new diff
  prerequisites were absent.**
- **Fresh old/new verdict diff: SKIPPED — no immutable baseline/candidate pin pair was selected;
  section 4 keeps G18's different 921-file claim separate.**
- **Release-pin battery: SKIPPED — no exact tag/release candidate or upload transaction was
  authorized; the historical dirty-epoch pin is ineligible.**

Historical Phase 1 evidence remains bounded input only:

```text
pin:          vm/pins/anubis-51f4a964347a
pin SHA-256:  51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027
host seal:    20/20 SEAL_PASS; captured rc 0; validator PASS
VM:           22/22; teardown verified
offensive:    34/34; validators PASS; teardown verified
verdict diff: 921 files; 0 flips; 0 timeouts
final review: APPROVE
```

That pin was built from a mixed dirty technical epoch and is **FORBIDDEN** as a tagged-release
artifact. No tag or Release was created.

## 9. What was not verified

- No operative-default CI success, `main`, branch protection, required check, or ruleset.
- No merge or candidate PR; no second GitHub/CODEOWNER approval.
- No future Phase 2 slice or pasted PR evidence.
- No self-hosted runner registration, secret lifecycle, untrusted-PR runner exercise, or operative
  default activation of the out-of-CI sealed-lane contract.
- No fresh exact-tag release build, immutable release pin, release evidence bundle, binary leak
  scan, code signing, notarization, tag, GitHub Release, or asset upload.
- No current-commit disposable Tart/VZ, offensive, Research, PoC, fuzz, exploit, agent, C2, or
  Apple Metal proof run.
- No new old/new 921-row verdict diff or refreshed full Phase 1 seal.
- No closure of `docs/CLAIMS.md` item 21, whole-runtime fail-closure, or unknown defects.
- No Phase 2, 3, 4, 5, 6, 7, or selected Phase 8 work.

## 10. What was gotten wrong or corrected

- The stale draft said no clean candidate existed. That became false after authorization and is
  replaced by the exact clean commits and receipts above.
- Blueprint missing-file assumptions were narrowed: the operating files existed on the old PR head
  and now exist on this candidate, but they remain absent from the operative default.
- The initial candidate hosted audit was not green. Its G28 failure was retained and traced to
  inherited hosted Metal-bypass variables contaminating synthetic release cases.
- Merely unsetting those variables would have reduced negative coverage. Three explicit isolated
  poison cases plus a pre-Cargo ordering assertion were added before adversarial approval.
- The first independent local JSON predicate assumed a nested summary schema and returned false.
  Inspecting the actual flat report schema and rerunning the corrected predicate returned true,
  rc 0. The parser mistake is not recorded as an audit failure.
- Candidate branch CI success does not satisfy default-branch CI success.
- Candidate out-of-CI documentation does not satisfy criterion 6 until the operative default
  carries it and stops advertising the permanently unavailable hosted Metal job.
- The 921-file native/reference agreement is not an old/new semantic verdict diff.
- The candidate spans 128 files in the canonical local base-to-HEAD diffstat, 22,480/2,228.
  GitHub's compare API reports per-file patch statistics of 15,895/1,766; section 1 labels that
  separate API observation instead of presenting it as the local diffstat.

## 11. Landing state and rollback

Completed public mutation, after explicit operator authorization:

```text
created/pushed branch: codex/phase15-reconciliation-20260801
remote candidate SHA:  e31a9e494fe9d26ff131e3f31167ded75f0193e9
PRs opened/merged:     0
default changed:       no
protection/rulesets:   unchanged
runners:               unchanged
tags/Releases/assets:  0 / 0 / 0
issues/comments/posts: none
force-push/deletions:  none
```

The commits were intentionally split: code/config/fixtures, then docs, then the single G28
code-only correction. This report is intended as one subsequent docs-only commit. The shared dirty
checkout was not reset, reformatted, staged, or swept; work occurred in the isolated worktree.

Rollback is bounded. The remote branch ref can be deleted only by a separately authorized public
mutation; deletion would not guarantee erasure of already public commit objects or Actions logs.
The three candidate commits can be reverted individually on a future integration branch. No
default-branch or repository-setting rollback is necessary because none was changed.

Prepared but unapplied transaction material remains under
`out/phase15_local_preparation_20260801T012220Z/` in the shared checkout. It is not operative GitHub
state and was not executed.

## 12. Sign-off

**PHASE 1.5 INCOMPLETE.**

Mechanical reconciliation is green on the bounded candidate branch, but literal criteria 1, 2, 3,
5, and 6 remain unsatisfied on the operative repository state. Criterion 4 is explicitly
future-dependent on Phase 2. Only criterion 7 is complete. Therefore this is not the narrower
`mechanical work complete; closure pending only on Phase 2 PRs` state.

The operator must decide whether to authorize a separate default-branch/protection transaction,
release transaction, runner/out-of-CI activation, or a sequencing exception that allows Phase 2
while Phase 1.5 remains closure-pending. None is inferred from this report.

**STOP. No Phase 2 work begins without explicit operator `GO`.**
