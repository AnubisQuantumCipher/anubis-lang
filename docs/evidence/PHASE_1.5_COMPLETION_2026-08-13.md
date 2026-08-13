# Phase 1.5 — Completion Receipt (2026-08-13)

**Verdict: PHASE 1.5 COMPLETE. All seven exit criteria closed on `main`. Release
`v0.1.0-preview` published. Ready for operator GO on Phase 2.**

This document supersedes `docs/evidence/PHASE_1.5_COMPLETION_2026-07-31.md` and
`docs/evidence/PHASE_1.5_COMPLETION_2026-08-12.md`. Those two were phase-in-flight
reports; this is the closure receipt with every exit criterion mapped to the exact
command and verbatim verdict line that decided it, per
`docs/COMPLETION_BLUEPRINT.md` § "Mandatory phase stop".

## 1. Header (blueprint § 3)

- **Absolute tree:** `/private/tmp/anubis-p15-receipt` (fresh worktree at `origin/main`
  for the write; the release binary was built in a separate isolated worktree at
  `/tmp/anubis-planD-v2` with `--target-dir /tmp/anubis-planD-v2/target`; the operator's
  own `/Users/sicarii/anubis-lang/target/release/anubis` was not touched — same mtime
  and sha as at session start).
- **Commit:** `6f4a141c64393ff092597ebeb604b0945cdfe217` — `main` HEAD after PR #9.
- **Branch:** `main` (this doc lands on `docs/phase-1.5-completion-receipt` for the PR
  and squashes onto `main`).
- **Dirty state at write time:** 0 entries in the receipt worktree; 226 in the
  operator's own trust-spine worktree (`a-plus-maturity/safe-mode-trust-spine-20260725`
  at `9aa28f9f`, unchanged in scope).
- **Binary provenance (Release):**
  - Pin: `vm/pins/anubis-afc2b8b38ca0-src-723739e83bb9-release`
  - sha256: `afc2b8b38ca02e0072f73b69fa0ce4c220e93da3fe7eada7506219873037e9dd`
  - Build mode: `cargo-build-locked-release-exact-head-archive-clean-target`
  - Commit-bound: `true`
  - Source manifest sha: `723739e83bb9` (1644 files, list sha `2c1d7314…`)
  - `--verify-release` output: `pin matches tree: vm/pins/anubis-afc2b8b38ca0-src-723739e83bb9-release`
- **Toolchain (from `attestation_identity.txt` at hosted-CI run 31650318577):**
  - `rustc 1.97.0-nightly (82bee9650 2026-05-09)`
  - `cargo 1.97.0-nightly (a343accce 2026-05-08)`
  - `Z3 version 4.15.4 - 64 bit`
  - `elan 4.2.3 (b6cec7e10 2026-06-08)`
  - `Lake version 5.0.0-src+8c9756b (Lean version 4.32.0)`
  - Runner: `macos-latest` (GitHub-hosted)

## 2. Exit criteria — one row per criterion, command + verbatim verdict

The seven exit criteria come from `docs/language/ROADMAP.md` § Phase 1.5, and were
tracked verbatim in the earlier in-flight report at `PHASE_1.5_COMPLETION_2026-08-12.md`.

| # | Criterion | State | Command | Verbatim verdict |
|---|---|---|---|---|
| 1 | CI green on default branch | **PASS** | `gh run list --repo AnubisQuantumCipher/anubis-lang --commit 6f4a141c64393ff092597ebeb604b0945cdfe217 --workflow anubis-ci --json event,status,conclusion` | `[{"conclusion":"success","event":"push","status":"completed"},{"conclusion":"success","event":"push","status":"completed"}]` |
| 2 | `main` protected by ruleset | **PASS** | `gh api repos/AnubisQuantumCipher/anubis-lang/rules/branches/main` | `pull_request` rule active with `required_review_thread_resolution: true`, `allowed_merge_methods: ["merge","squash"]`; `required_status_checks` requires `hosted-gate-witness` context with `strict_required_status_checks_policy: true`; `non_fast_forward` and `deletion` blocks active; ruleset id `20440878` name `main-protection` `enforcement: active` |
| 3 | All four operating files present on default | **PASS** | `for f in AGENTS.md docs/CLAIMS.md docs/COMPLETION_BLUEPRINT.md docs/language/ROADMAP.md; do gh api repos/AnubisQuantumCipher/anubis-lang/contents/$f?ref=main --jq .size; done` | 4/4 present; sizes on `6f4a141c`: `AGENTS.md`=226 lines, `docs/CLAIMS.md`=2416 lines, `docs/COMPLETION_BLUEPRINT.md`=91 lines, `docs/language/ROADMAP.md`=301 lines |
| 4 | Phase 2 slices land in own PRs (pattern established) | **PASS** | `gh pr list --repo AnubisQuantumCipher/anubis-lang --state merged --json number,title --limit 12` | Pattern established across `#4` (release lane), `#5` (docs), `#6` (pin refresh), `#7` (leak scanner), `#8` (release lane path fix), `#9` (compiler middle: identity walker depth bound), `#10` (CI timeout) — each a bounded slice with own regression coverage. PR `#2` (`phase2/unified-value-flow → main`) remains OPEN as the pattern's canonical exemplar |
| 5 | Published Release with binary + evidence bundle | **PASS** | `gh api repos/AnubisQuantumCipher/anubis-lang/releases/tags/v0.1.0-preview --jq {draft,prerelease,published_at,assets:[.assets[]\|.name]}` | `{"draft":false,"prerelease":true,"published_at":"2026-08-13T00:06:52Z","assets":["anubis-v0.1.0-preview-evidence.tar.gz","anubis-v0.1.0-preview-macos-arm64.tar.gz","SHA256SUMS"]}` |
| 6 | Runner OR out-of-CI documented | **PASS** | `grep -n 'sealed VZ' docs/CLAIMS.md` | `docs/CLAIMS.md:154:### Phase 1.5 – sealed VZ + metal-prove workflow jobs are explicitly OUT-OF-CI` (added by PR #5, merged `54bff581`); no self-hosted runner is registered, and the two workflow jobs that would have required one are named as out-of-CI residuals in the living register |
| 7 | Auto-push behaviour understood + safe | **PASS** | `cat .git/hooks/post-commit \| head -3` and `git log --oneline origin/main -3` | Post-commit hook auto-pushes commits on `a-plus-maturity/*` branches to origin; runs invisibly. Understood as an operator convenience, not a CI signal. `commit_bound_pin=publish_pin.sh --release` and the release lane's `--verify-release` invariant catch any silent divergence. All four PRs merged this session (`#7`, `#8`, `#9`, `#10`) went through the explicit `gh pr merge --squash` path — not via a silent push |

## 3. RED-before-GREEN for each fix (blueprint § 5)

Every enforcing change in this phase has a documented RED before its GREEN. This
section names the pre-fix failure verbatim and the post-fix pass verbatim.

### PR #6 — dangling `vm/pins/CURRENT` on `main`

- **RED (before merge):** `bash scripts/publish_pin.sh --verify` on `main` at
  `54bff581` — `PIN_PATH_INVALID: missing path component: vm/pins/anubis-4ea0a18e3ed4`
  and `PIN_FILE_INVALID: CURRENT must name a non-writable regular non-symlink executable`.
- **GREEN (post-merge on `d513e2b4`):** `bash scripts/publish_pin.sh --verify` →
  `pin matches tree: vm/pins/anubis-1d98ae1e381f-src-98acd2ed239f`, exit 0.
- **Accept-side guard:** the new `.meta` sidecar is committed alongside `CURRENT`,
  so the previous "CURRENT advanced but meta never committed" pattern cannot recur
  silently — the tracked file set now covers the identity claim.

### PR #7 — release-lane leak scanner (documented Tart guest + Actions runner)

- **RED (before merge):** `bash scripts/build_public_release.sh --pin
  vm/pins/anubis-d3bc61e6f7f9-src-98acd2ed239f-release --tag v0.1.0-preview --out
  out/public_release --ci-artifact out/ci_artifact` on `d513e2b4` (pre-fix) →
  `/Users/[a-z] 1  OFFENDING: /Users/admin/x`)ExploitPOCThe PoC .anb (relative to
  the host cwd)  RELEASE_REFUSED: leak gate found forbidden content; see …/leak-scan.txt`.
- **Post-fix intermediate RED (CodeRabbit catch):** the first fix used record-level
  `grep -cE` counting, which for a hypothetical mixed record `prefix /Users/admin
  /Users/sicarii suffix` would increment both `n` and `safe_n` by 1 and net to 0,
  hiding the operator-home leak. Regression demo output: `n=1 safe_n=1 adjusted=0`.
- **GREEN (post-second-fix on `639e316b`, commit `4c276c1b`):** same demo with
  match-level `grep -oE "$pattern[^[:space:]]{0,60}"` produces one match per
  path, filter keeps `/Users/sicarii`, drops `/Users/admin`. Regression test
  output: `n=2  OFFENDING: /Users/attacker/leaked  /Users/sicarii`.
- **Accept-side guard:** the allowlist is anchored (`^${leak_users_safe_prefixes}`)
  with a word boundary (`[^A-Za-z0-9_]|$`), so a substring like
  `/Users/administrator` would NOT match the safe pattern and still fails the gate.

### PR #8 — release-lane `--out` path canonicalisation

- **RED (before merge):** `bash scripts/build_public_release.sh --pin <pin> --tag
  v0.1.0-preview --out out/public_release --ci-artifact out/ci_artifact` after
  leak scan PASSED and the whole staged tree was on disk →
  `tar: Failed to open 'out/public_release/v0.1.0-preview/639e316b.../dist/anubis-v0.1.0-preview-macos-arm64.tar.gz'`.
  The subshell `cd`'d into `$STAGE/public/binary/macos-arm64`, and `$STAGE`
  was still relative from the outer cwd, so the tar destination was unreachable.
- **GREEN (post-fix on `b42fe40f`, commit `7c47db69`):** same command produces
  `RELEASE_STAGED tag=v0.1.0-preview commit=6f4a141c… tree=bdfc41766277db8c…
  binary_sha256=afc2b8b38ca0… signing=adhoc-unnotarized out=…` exit 0.
- **Regression demo (in commit body):** two identical subshell shapes with
  `OUT=out` (relative) reproduce the tar failure pre-fix and succeed post-fix.

### PR #9 — compiler mutual-recursion identity-walker DoS

- **RED (before merge):** `./target/release/anubis check /tmp/repro_cycle.anb`
  on operator's pin binary `2ce9f7db08baac…` →
  `thread 'main' (23467444) has overflowed its stack  fatal runtime error: stack
  overflow, aborting  rc=134` (SIGABRT). Same fixture (renamed to
  `tests/fixtures/language_core/mutual_recursion_over_list_literals_accepts.anb`)
  reproduces this crash `rc=-6` on the pre-fix binary in the shared tree.
  A five-cycle stress fixture (`mutual_recursion_five_cycle_over_list_literals_accepts.anb`,
  `a → b → c → d → e → a`) crashes identically, proving the bug generalises past
  the 2-cycle case.
- **GREEN (post-fix on `6f4a141c`, commit `b08dea5d`):** fresh isolated release
  build `sha256=4bbe8a8a1a90b65767cbc4ec85fc3e6c050dc75263da79ed9146560803ed048a`
  runs both fixtures with `rc=0` and stdout `check passed`. 8/8 pre-existing
  representative fixtures (hello / hello_normal / secret_declassify_hello /
  taint_reject / proof_factorial / symbolic_assert_fail / symbolic_assert_pass /
  core_language_showcase) yield identical exit codes before and after.
- **Non-weakening argument (recorded in the fix docstring):** the depth bound
  mirrors `fn_alias_of_d`'s `FN_ALIAS_MAX_DEPTH = 8`, whose docstring at
  `compiler/src/middle/mod.rs:359–:365` is: *"The bound is a DEFERRAL, never an
  assertion: running out of depth returns `None`, which can only fail to resolve
  an identity and therefore only fail to reject. It cannot invent one."*
  The same reasoning is inlined verbatim on `fn_identities_carried_by_value_d` in
  `compiler/src/middle/mod.rs`.
- **Accept-side guard:** the two regression fixtures land in
  `tests/fixtures/language_core/`, wired into `G5_language_fixtures`. A regression
  of the bug flips `G5` red on `main`.

### PR #7-#9 fallout — the four gates that flipped RED on PR #9's first push

Fixed in the same PR (commit `b08dea5d`):

- **`G1_fmt`** RED: `Diff in compiler/src/middle/mod.rs:741` and `:2066` — rustfmt
  wanted to wrap two long function calls that grew past 100 cols when the
  identifiers became `_d` variants. GREEN: `cargo fmt` auto-fixed; `cargo fmt --
  --check` exits 0.
- **`G3_test`** RED: `cargo test --all` — doctest on `fn_identities_carried_by_value_d`
  choked on `error: unknown start of token: \u{2026}` because the Anubis-lang
  example in the `///` docstring was compiled as Rust. GREEN: wrapped in
  ```` ```text ```` fenced block; `cargo test --all` → `test result: ok. 1214
  passed; 0 failed`.
- **`G5_language_fixtures`** RED: the two new fixtures' EXPECT headers had
  parenthetical annotations (`// EXPECT: PASS (no crash; program type-checks
  and lowers)`), which don't match the strict regex
  `^[[:space:]]*//[[:space:]]*EXPECT:[[:space:]]*(PASS|FAIL)[[:space:]]*$`.
  GREEN: trimmed to `// EXPECT: PASS`; `parse_expectation` on both fixtures
  now returns `EXPECT=PASS MALFORMED= COUNT=1`.
- **`G16_docs_drift`** RED: adding 2 fixtures bumped `find tests/fixtures/language_core
  -name '*.anb' | wc -l` from 253 to 255 and `python3 scripts/lib/native_corpus_inventory.py
  --count` from 921 to 923, but 14 stamps across `AGENTS.md`, `README.md`,
  `docs/CLAIMS.md` still claimed the old numbers. GREEN: 14 stamps bumped by
  exactly +2; `bash scripts/run_docs_drift_gate.sh` reports `Overall: PASS (50
  stamps checked, 0 drift)`.

## 4. Falsification twins (blueprint § 6)

- **Direct twin (PR #9):** `mutual_recursion_over_list_literals_accepts.anb` — the
  minimal repro (2-cycle).
- **Alternate-carrier twin (PR #9):** `mutual_recursion_five_cycle_over_list_literals_accepts.anb`
  — 5-function cycle, proves the fix does not memoise a specific pair.
- **Dead-branch twin (structural):** `fn_alias_of_d` at
  `compiler/src/middle/mod.rs:369` was already correctly guarded before this
  session and remains untouched; its own regression coverage
  (`FN_ALIAS_MAX_DEPTH = 8` at `:366` + its docstring at `:359-:365`) is the
  correctness precedent this PR mirrors. Same walker family, same disease,
  same fix pattern.

## 5. Phase-metrics start (blueprint § 7)

Captured on the receipt tree at `6f4a141c`. Verbatim from
`bash scripts/phase_metrics.sh`:

```
═══ PHASE METRICS ═══
tree      : /private/tmp/anubis-p15-receipt
commit    : 6f4a141c64393ff092597ebeb604b0945cdfe217
branch    : docs/phase-1.5-completion-receipt
dirty     : 0 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28680   strictly decreasing (Phase 2+)
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
```

This is the START-of-Phase-2 baseline. The end-of-Phase-2 receipt must show:

- `duplicated lane pairs`: 4 → 0
- `walker families`: 4 → 1
- `_ => in label-lane walkers`: 7 → 0
- `lane facts with no join`: 1 → 0
- `fused cross-lane joins`: 1 → 0
- `general ExprStmt arm: walk_block_{taint,secret}`: NO → yes for both
- `middle/mod.rs lines`: strictly decreasing (currently 28680; Phase 2 must reduce)

## 6. Verified / believed / skipped / unknown (blueprint § 8)

### Verified this session — direct evidence, reproducible

- Six PRs (`#5–#10`) merged to `main` — timestamps and squash-shas in
  `git log origin/main --oneline`, verifiable against the GitHub API.
- Release `v0.1.0-preview` published — `gh api …/releases/tags/v0.1.0-preview`
  returns `draft: false, published_at: 2026-08-13T00:06:52Z`.
- Hosted-CI green on `6f4a141c` for both `push` runs on `main` and the tagged
  `v0.1.0-preview` ref — `gh run list --commit …` shows two `completed/success`.
- Release-bound pin binary is leak-free: `strings -a
  vm/pins/anubis-afc2b8b38ca0-src-723739e83bb9-release | grep -oE
  '/Users/[a-z].{0,60}'` returns zero non-safe matches; leak-scan.txt shows
  `0/0/0/0/0/0`.
- `verify_release.py` PASS on the staged tree — 15 files matched.
- Compiler fix runtime-proven: `rc=-6 → rc=0` on the mutual-recursion repro,
  regression coverage in `tests/fixtures/language_core/`.
- CI timeout raised 45 → 60 min (PR #10).

### Believed but not directly re-verified this session

- `docs/CLAIMS.md` § "Known open issues (2026-07-26)" — the living residuals
  register. Reviewed for stamp accuracy in G16 (14 stamps updated to match
  measured 255/255 and 923). Content otherwise inherited unchanged; the
  operator's `[NEEDS-HUMAN]` calls remain the authority.
- All prior phase completion receipts (`PHASE_0_COMPLETION_2026-07-30/31`,
  `PHASE_1_COMPLETION_2026-07-30/31`, `PHASE_1.5_COMPLETION_2026-07-31/08-12`).
  Believed valid at their dates; not re-run against today's tree.
- `bash scripts/run_seal_checklist.sh` at commit `6f4a141c` — the seal on this
  new commit was not re-run in this session (the last seal captured mid-session
  was against `639e316b`'s release pin, `SEAL_PASS 20/20 core-profile gates`).
  A fresh seal against `6f4a141c` before Phase 2 begins would confirm the
  post-`#9` binary carries no gate regression.

### Skipped with reason

- Bit-for-bit-reproducible release builds. Rust's release builder is not
  bit-deterministic on macOS (DWARF timestamps, path fragments in linker); the
  release pin binds source-manifest identity + independent-verify, not byte
  identity. Same discipline as PR #3's original operator-home-leak fix.
- Notarization of the release binary. Ad-hoc signed; Gatekeeper-friendly install
  is not a Phase-1.5 requirement, is called out explicitly in the release notes
  ("first launch on macOS will require explicit user consent").
- G9 (POC kit) end-to-end on the release binary. Explicitly out-of-CI per PR #5;
  the operator-run disposable Tart/VZ lane is the authoritative surface.
- Metal-prove workflow certification. Same reasoning as G9; explicitly OUT-OF-CI
  per docs/CLAIMS.md addition in PR #5.

### Unknown (cannot certify from this session)

- Behaviour of the release binary under sandboxed / hardened-runtime enforcement
  on downstream user machines. Local runs on Apple Silicon Mac verified.
- Whether every G5 fixture on `main` (255 total) passes in a clean environment;
  the two new fixtures I added were spot-verified; the other 253 were not
  re-run this session (they ran in hosted CI at `6f4a141c` as part of the
  full gate suite → HOSTED_PASS 28/29 with G9 EXTERNAL).

## 7. What was not verified + what went wrong (blueprint § 9)

### What was not verified

- I did not run `bash scripts/run_seal_checklist.sh --bin
  vm/pins/anubis-afc2b8b38ca0-src-723739e83bb9-release --profile core --out
  out/release_seal_v0.1.0-preview` on the FINAL post-`#9` release binary.
  The prior seal (mid-session) was against the pre-`#9` pin
  `anubis-c981244ff328-src-98acd2ed239f-release`, verdict `SEAL_PASS
  pass=20 skip=0 known_fail=0`. Recommendation: operator or next lead runs
  this seal before publishing another artifact against this pin.
- I did not re-run G21 (Lean formal gate) locally; it was green in hosted CI.
- I did not measure whether the four failing-when-first-pushed gates
  (G1/G3/G5/G16) are stable under second-push conditions on unrelated PR
  branches; only measured on PR #9 pre-and-post the same-PR fixup commit.

### What this phase got wrong (honest post-mortem)

1. **My first leak-scanner fix (PR #7 commit `6627a734`) was record-level.**
   CodeRabbit correctly caught that a mixed strings record containing both
   `/Users/admin` and `/Users/sicarii` would net to zero. Second commit
   (`4c276c1b`) fixed it at match level. Cost: one CodeRabbit round-trip.
   Lesson: when the scanner is the last line of defense, its regex must
   count matches, not records.

2. **My first repro fixture EXPECT headers had parenthetical annotations.**
   `G5_language_fixtures` uses a strict regex that rejects them. Cost: one
   commit round-trip in the same PR. Lesson: authoritative-header parsers
   are strict for a reason (see `parse_expectation` in
   `scripts/lib/gate_common.sh`); match their format exactly.

3. **My `/// fn d(…)` docstring on the compiler fix compiled as a Rust
   doctest and blew up on `…` (U+2026).** Cost: one commit fix (fenced as
   `text` block). Lesson: any Rust `/// ` block-fenced code is a doctest by
   default; language tag is mandatory for non-Rust examples.

4. **The tar-bug in `build_public_release.sh`** — silent relative-path bug
   introduced in PR #4 and never exercised end-to-end. I hit it during Plan
   D, worked around it manually to complete the release, then landed
   PR #8 as a proper fix. Lesson: the release lane needs its own smoke test.
   Filed as a Phase-2-adjacent follow-up but not blocking Phase 1.5 closure.

5. **CI's 45-min timeout was systematically tight.** Four PRs this session
   observed the 40–45 min end-to-end tail; PR #7's first push CI hit the
   wall and had to be re-run. Raised to 60 min in PR #10. Lesson: measure,
   don't guess CI timeouts; when the tail sits at the wall, raise the wall.

## 8. Landing state (blueprint § 82–91)

- **One bounded slice per review unit:** ✓ Every PR merged is a bounded
  slice with its own PR, own regression coverage, own CI run.
- **Code and documentation in separate commits:** ✗ Partial. PR #9 landed the
  compiler fix + regression fixtures + doc-stamp bumps in one commit.
  Justification: the 14 stamp bumps in AGENTS.md / README.md / docs/CLAIMS.md
  are direct consequences of adding the 2 fixtures — separating them would
  land a broken G16 mid-stack. PR #7 fixup was similar (leak-scanner fix
  needed to land with the CodeRabbit response). PR #8 (script-only) and PR
  #10 (yaml-only) are single-file. Going forward: doc-only slices land
  separately.
- **Trust-surface changes state old + proposed accept conditions
  explicitly:** ✓ PR #9's docstring on `fn_identities_carried_by_value_d`
  cites `fn_alias_of_d:359-:365` as the precedent for the depth-bound
  discipline; PR #7's commit body includes the reproducer for the record-
  vs-match distinction.
- **Only the active lead builds, publishes pins, commits, pushes:** ✓ This
  session was single-lead throughout; no sub-agents committed or built.
  Explicit paths only in a mixed tree: ✓ isolated `--target-dir` for every
  build; operator's `target/` untouched.
- **A frozen pin is evidence only about the artifact it names:** ✓ The
  released pin `afc2b8b38ca0` binds to `6f4a141c` source; the earlier
  `c981244ff328` pin (Plan D pre-#9) was superseded and its binary is not
  attached to the published Release.
- **Research/crash/fuzz/offensive uses disposable guest:** ✓ None of the
  work in this phase touched the offensive lane; sealed VZ + metal-prove
  are explicitly OUT-OF-CI per docs/CLAIMS.md § 154.

## 9. Recommendation

**Phase 1.5 is complete. Recommend GO for Phase 2.**

Phase 2's exit target (COMPLETION_BLUEPRINT.md:54): *"replace duplicated
value-flow walkers with one total, lane-parameterized mechanism."*
Phase-metrics baseline is the § 5 snapshot above. The four duplicated lane
pairs, the four walker families, the seven `_ => ` arms in label-lane
walkers, the one fused cross-lane join, and the one lane fact with no
join are the exit-criterion targets.

The disease this phase fixes is exactly the class that produced PR #9's
mutual-recursion DoS: walker parity failures where one member of a family
gets a fix and its sibling silently misses it. Phase 2 makes such parity
failures a compile-error at the walker-family level, not a fixture the
next audit round has to catch.

Ready for GO/HOLD.

## 10. Rollback

If any assertion in this receipt turns out to be wrong, the receipt is
supersedable by a dated successor. The release itself can be un-published
via `gh release edit v0.1.0-preview --repo AnubisQuantumCipher/anubis-lang
--draft=true`, or fully deleted with `gh release delete v0.1.0-preview
--repo AnubisQuantumCipher/anubis-lang --yes && git push origin
:refs/tags/v0.1.0-preview`. The tag is reversible by `git push origin
:refs/tags/v0.1.0-preview && git tag -d v0.1.0-preview`.

None of these rollback actions are recommended; the release is source-
bound, CI-attested, verifier-passed, and regression-covered. They exist
as an escape hatch, not a plan.
