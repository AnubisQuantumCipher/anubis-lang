# Phase 1 completion report — 2026-07-31

This is the controlling Phase 1 record. It supersedes the completion disposition in
`PHASE_1_COMPLETION_2026-07-30.md` without erasing that file's historical measurements. The result
is deliberately narrow:

> **The enumerated Phase 1 technical exit rows are green for the measured dirty working-tree
> epoch. Phase 1 remains INCOMPLETE until the external finalization predicate in sections 8 and 12
> is satisfied. It is uncommitted, unlanded, unpushed, and unshipped; the mandatory phase stop
> remains in force until the operator reads the activated report and explicitly says go.**

It is not a claim that Anubis is totally sound. `docs/CLAIMS.md` item 21 and the unenumerated
whole-`run` builtin surface remain open. All execution-bearing measurements below name the immutable
binary and source epoch they actually measured.

Schema precedence is explicit: this controlling report follows the twelve mandatory sections in
`/Users/sicarii/Desktop/ANUBIS_FINAL_BLUEPRINT_PHASES_2026-07-29.md` Part II.5. The takeover
handoff's alternate July-30 rewrite map conflicts with that controlling schema and is superseded for
report structure. The July-30 file remains a bannered historical receipt; this July-31 file is the
controlling completion record.

## 1. Header — what tree, what commit

```text
Phase:          1 — Evidence & isolation integrity
Tree:           /Users/sicarii/anubis-lang
Commit:         0e910c9bb2e83438696eaaf0f49d0e3c5e658960
Branch:         a-plus-maturity/safe-mode-trust-spine-20260725
Upstream:       0e910c9bb2e83438696eaaf0f49d0e3c5e658960
Dirty:          224 entries
Binary:         /Users/sicarii/anubis-lang/vm/pins/anubis-51f4a964347a
Binary mtime:   2026-07-31T12:53:23Z
Binary bytes:   99,433,808
Binary mode:    0555
Binary SHA-256: 51f4a964347a4a0f3ea2833331eb313315aa502c96c9d7a71fc3b20414eca027
Rebuilt from this commit? no — built from the dirty technical epoch rooted at this commit
Toolchain:      rustc 1.97.0-nightly (82bee9650 2026-05-09)
                cargo 1.97.0-nightly (a343accce 2026-05-08)
                Lean 4.32.2, commit f3b06c705e6c85f5314019d5d3baab0fec5b580c
                Lake 5.0.0-src+f3b06c7
                Z3 4.15.4, 64 bit
                Tart 2.32.1
```

The execution-bearing technical epoch was stable before, at synchronization, and after the
technical-epoch VM battery:

```text
technical source-tree SHA-256: 0281e8034022fc62f4f853906a33173bc0286e9ae9a0e07b26d761a495962b03
technical source entries:      1563
technical source-list SHA-256: cc4e2c6f77682f19d26e95733c9eb2b87550d50552b4fa8a5e2d37f88a714be6
technical pin metadata UTC:    2026-07-31T18:07:15Z
technical pin metadata SHA:    1bb84c802e2590301d714119177508ab3db77486166f4f7fa867a02837555535
```

Rewriting this report, reconciling the other claim documents, and correcting the subsequently
exposed seal/corpus harness defects all change manifest-eligible files after `0281e803…`. They do
not retroactively alter what ran in the earlier guests; instead they create a source-current
finalization epoch whose required refreshes are explicit in sections 8 and 12. That epoch is bound
separately:

```text
final source-tree authority:      [PENDING — frozen report defers to the external receipt]
final pin metadata authority:     [PENDING — predetermined path] out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md
final host seal directory:        [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_host_seal_final_51f4_r2_20260731T230000Z
```

The report itself is in the pin manifest, so embedding its own final source-tree or metadata digest
would create a circular hash commitment. Those literal final digests therefore live only in the
external finalization receipt named above; that receipt is generated after the report is frozen and
is outside the pin source manifest. The same executable SHA must remain in both epochs. If the final
repin changes executable bytes or the final host seal in section 8 is not an actual `SEAL_PASS`, this
report's `COMPLETE` disposition is automatically void and must read `INCOMPLETE`.

## 2. Exit criteria — one row per checkbox, with the command

All PASS rows below name commit `0e910c9b…` and technical source tree `0281e803…`. Exit codes were
captured by the controlling run immediately after each command; the retained machine artifacts are
linked by path and hash in sections 5 and 8.

| Criterion | Command run | Verbatim output line | Verdict |
|---|---|---|---|
| Leaking program cannot produce a bundle asserting an independent taint guarantee | `bash scripts/run_pca_gate.sh --out out/phase1_pca_postmetrics_final_20260731T190000Z` | `PASS no_retired_taint_claim (=false)` and `Overall: PASS (19/19)` | **PASS** — PCA v2 does not serialize the retired theorem |
| `anubis verify` no longer confirms an unearned taint claim | same PCA command | `PASS rehashed_v2_retired_field_fails (=1)`; `PASS rehashed_v1_only_claim_fails (=1)`; `PASS rehashed_v2_unknown_field_fails (=1)`; `PASS rehashed_missing_pca_fails (=1)` | **PASS** — consistently rehashed legacy, retired, unknown, and missing semantic claims fail closed |
| `build` refuses or requires consent for the Research lane, using the shared whole-program decision consumed by Run/Prove/REPL | `ANUBIS_VM_MEM=5120 ANUBIS_VM_BUILD_JOBS=3 ANUBIS_REPO=/Users/sicarii/anubis-lang ANUBIS_VM_EVIDENCE_DIR=out/phase1_vm_51f4_postmetrics_final_20260731T182200Z bash scripts/vm/run-slice.sh` plus the disposable-guest falsification launcher in section 5 | `{ "type": "test", "name": "tests::whole_program_callers_share_the_same_mode_derived_research_boundary", "event": "ok" }`; `CASE_RESULT name=research_direct expectation=reject rc=1 marker=ANUBIS_BUILD_RESEARCH_REQUIRES_ALLOW verdict=PASS` | **PASS** — caller tests and direct/carrier/dead-branch Build probes agree |
| `@research` local-`let` field-access over-rejection is closed without weakening the ordinary twin | same technical-epoch VM battery plus the section 5 guest launcher | `{ "type": "test", "name": "backends::run::run_tests::research_block_local_field_access_and_ordinary_twin_both_lower", "event": "ok" }`; `CASE_RESULT name=local_ordinary expectation=accept rc=0 marker=- verdict=PASS` | **PASS** — direct, live-carrier, dead-branch, and ordinary controls accept |
| Full 0-flip verdict diff, VM seal, and offensive `34/34` | `python3 scripts/phase1_verdict_diff.py --old vm/pins/anubis-281e0e846948 --new vm/pins/anubis-51f4a964347a --expected-old-sha256 281e0e8469484b72a954a2570a3fd92d3cda18cb2e615a26933b73640dae5262 --expected-old-meta-sha256 0dc5d51e6af380e69b841254dfe9c5f40a1e70cfe138712cfcf6368c893c6472 --root /Users/sicarii/anubis-lang --out out/phase1_verdict_diff_281e_to_51f4_postmetrics_20260731T185400Z.json --workers 4 --timeout 90 --expected-count 921`; technical-epoch VM command above; `ANUBIS_BIN=/Users/sicarii/anubis-lang/vm/pins/anubis-51f4a964347a ANUBIS_OFFENSIVE_GATE_VM_MEM=5120 ANUBIS_VM_BUILD_JOBS=3 bash scripts/run_offensive_platform_gate.sh --out out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z` | `VERDICT_DIFF_V2 verdict=PASS total=921 flips=0 timeouts=0 rc_changes=0`; `PASS — all gates green, fixpoint unchanged.`; `Overall: PASS (34/34) isolation=tart-disposable-guest expected=34` | **PASS** — all three technical-epoch receipts are source/pin bound and both guests were deleted |

Supporting checks in the same technical epoch also returned:

```text
compiler library:             771 passed, 0 failed
language fixtures:            Overall: PASS (253/253)
security fixtures:            Overall: PASS (327/327)
stdlib fail-closed:            Overall: PASS (104/104) timed_out=0
native-authoritative corpus:   921 files, mismatches=0 disagreements=0
corpus/pin poison suite:       CORPUS_INVENTORY_BINDING: 27 passed, 0 failed
walker completeness:           WALKER_COMPLETENESS_GATE: PASS
promise coherence:             PROMISE_COHERENCE_GATE: PASS (5 restatements, each carrying scope + CLAIMS.md pointer)
host-resource self-test:       HOST_RESOURCE_GUARD_SELFTEST: PASS (pass=52 fail=0)
```

These support the exit rows; they do not enlarge Phase 1 into a total-language soundness claim.

## 3. RED before GREEN — for every fix in the phase

Historical RED files below are retained pre-fix dirty snapshots rooted at this branch. Where the
old instrument did not capture a full source-tree manifest, that identity is not invented here.
The GREEN column is the source-bound `0281e803…` epoch unless explicitly labeled a controlled
helper mutation or later pre-freeze harness correction.

| Fix | Proof it failed before | Proof it passes after |
|---|---|---|
| Remove the independent PCA `taint_clean` theorem | `/tmp/p1-pca-v2-unknown-red.txt`, SHA `677743f02fa4660cac619e890178845bf584b5caaebac669cc90cbecb8799710`: `verify_pca_rejects_rehashed_v2_with_a_retired_taint_claim ... FAILED` | PCA `19/19`: `PASS no_retired_taint_claim (=false)` and `PASS rehashed_v2_retired_field_fails (=1)` |
| Fail closed on absent semantic PCA | `/tmp/p1-pca-missing-red.txt`, SHA `75b2c00cab5f1d527d9bd22fa8a53e0e13b143b1fb4539e6b1fa54114e0440ca`: `verify_pca_rejects_rehashed_bundle_with_no_semantic_claim ... FAILED` | PCA `19/19`: `PASS rehashed_missing_pca_fails (=1)` while `PASS verify_clean_passes (=0)` |
| Require Research Build consent/VZ through the shared program-derived boundary | `/tmp/anubis-phase1-research-build-red.txt`, SHA `8291d4d35baf69ac55973e38de9263d61eb3e00f615213201b13854c237bd875`: `research_build_requires_explicit_consent_and_vz_before_lowering ... FAILED` | Final cargo JSON: `research_build_requires_explicit_consent_and_vz_before_lowering ... "event": "ok"`; guest direct/carrier/dead branches reject and consented guest control accepts |
| Close Research-block local-field false rejection | `/tmp/anubis-phase1-research-field-red.txt`, SHA `0670f1ec543e61950f38ccaa7941b8e940005fde209307808964379fbb3f6b76`: `unknown name 'local' used as a value` | Final cargo JSON named test is `ok`; four guest accept controls are PASS |
| Preserve checker rejection before the Research consent diagnostic | `/tmp/anubis-phase1-run-rejection-parity-red.txt`, SHA `170c90339e912517c656b22b811cc58c340b27ec2db5aada46f2e6d496a4945e`: named test `FAILED` | Final cargo JSON: `run_preserves_check_rejection_before_the_research_consent_boundary ... "event": "ok"` |
| Bind generated evidence to the requested source and refuse non-PASS Build evidence | `/tmp/anubis-evidence-source-binding-red.log`, SHA `358baff9bc00df55fc4d2c3e350a3946315d2b1dbba5bf9df6c5757223334f7b`: `manifestless_build_evidence_binds_only_the_requested_program ... FAILED` | `/tmp/anubis-evidence-source-binding-green.log`, SHA `fae370a805febf0f9a1888652b746a4dc940158f7bb439147b120bc5a4039002`: `1 passed; 0 failed` |
| Bind CLI integration tests into the immutable pin contract | `/tmp/anubis-pin-cli-tests-red.log`, SHA `7e28f59326701c3bc1953f22b1bbcd770c908074d6acbc1472255ec46a11614c`: `CORPUS_INVENTORY_BINDING: 9 passed, 2 failed` | `/tmp/anubis-pin-cli-tests-green.log`, SHA `f1de93149270f37f940881539ac386d1e3d420a44f5af1319e9b93b48b13b2c5`: `CORPUS_INVENTORY_BINDING: 11 passed, 0 failed` |
| Complete the VM roster and propagate job caps | `/tmp/anubis-phase1-final-review-red.log`, SHA `5e0d59d229ba4cd0ac5de67a16d1276f6c283a186de1322650769d609eb4bf34`: `HOST_RESOURCE_GUARD_SELFTEST: FAIL (pass=49 fail=3)` | `/tmp/anubis-phase1-final-review-green.log`, SHA `bcfa7e5a282ea6cec23debaa0625941adef77781c14d00d7f755b8f59db714ad`: `HOST_RESOURCE_GUARD_SELFTEST: PASS (pass=52 fail=0)` |
| Make the VM wrapper consume a machine fixpoint and exact log protocol | Guest `anubis-run-83786` executed the workload, but wrapper validation failed with `malformed seal fixpoint`, corrupted GNU-`stat` protocol, `gate failures : 1`, and verified teardown; log SHA `f9afd01f04dc930c8ed7918c5e506a33ae5c71dc79a4fa1c8f07f62f86c32c66` | Guest `anubis-run-23962`: all 22 exact exit codes zero, machine fixpoint matches, validator PASS, manifest rehash PASS, teardown PASS |
| Refuse a stale offensive PASS and bind exact old diff identity | Controlled mutation log `/tmp/anubis-phase1-controlled-red-green-v3-20260731T184000Z.log`, SHA `dcff0b9cd322c027c9c04ea6732b0e49fd4e5258de67f26c563d9647fc937cff`: substituted old pin was accepted, old metadata mismatch was not raised, and a bad typed argument left a stale PASS | Same log ends `PHASE1_CONTROLLED_RED_GREEN_V3: PASS`; the real strict offensive validator and exact-old-pin/meta 921-file diff both return PASS |
| Count shared thin label adapters as one traversal architecture, while still counting a structural wrapper | `/tmp/anubis-phase1-metrics-red-green-20260731T184700Z.log`, SHA `820776d8b375755e90feeb1ce1f51883e69210c09c42cf3d00678108fbf0c636`: old classifier reports two failures and six families | The same controlled log proves thin adapters produce four families, a structural wrapper produces five, `10 passed, 0 failed`, and ends `PHASE1_METRICS_CONTROLLED_RED_GREEN: PASS` |
| Keep the structured host-seal scorer portable to macOS system Bash 3.2 | Failed host seal log SHA `8347b368d39ed08ef41a8e366c844d48e1646941608b7c62c0cdb6415a024051` ends `mapfile: command not found`; `/tmp/anubis-phase1-seal-bash32-red.log`, SHA `1000fc36c910ec201c86098e756ea5b30c3f7029d9a42731055f1b6dbedd3775`, records `FAILED (failures=1)` across six tests | The pre-follow-up `/tmp/anubis-phase1-seal-bash32-green.log`, SHA `1d0c64cc113df6793b1374edf87f53b15fc86dd1b5f3cbbef9d3ba92fbf91b24`, records all six tests `OK`; `/bin/bash -n scripts/run_seal_checklist.sh` exits 0 with empty stdout digest `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| Prevent the corpus/pin poison gate from prompting on a read-only scratch metadata file | `/tmp/anubis-phase1-final-corpus-binding-refresh.log`, SHA `4815a6b85f701492eace8b5428c0df2ec3c0b549ad0bc0cfe4f1f0571f919371`, ends at BSD `rm`'s `override r--r--r-- ...?` prompt without a verdict; the controller terminated that incomplete run | The two exact scratch-metadata removals use `rm -f --`; an interactive-PTY rerun exits 0 and `/tmp/anubis-phase1-corpus-binding-tty-green.log`, SHA `6036020b34b8303d6c68f0082f4899d9a707abf1dfa6bc0029a96ed847c384bf`, ends `CORPUS_INVENTORY_BINDING: 27 passed, 0 failed` |
| Preserve base scorer reasons for no-marker and known-failing policy branches while recording marker counts | `/tmp/anubis-phase1-seal-reason-policy-red.log`, SHA `cf6202804f7ff7eb6432df747d2c5c47adea2192376721f711f45cf5de4b4100`, records both focused cases failing because the marker count was appended to `_v_reason` and `_v_marker_count` was unset | `_v_reason` and `_v_marker_count` are separate; `/tmp/anubis-phase1-seal-reason-policy-green.log`, SHA `1c3d915d9107e0d893a7797a8f483609ee3b3c64edd6b7dd8217044aba57055f`, records all eight tests `OK`, including one declared FAIL and one no-marker case through the actual shell function |
| Classify the corpus/pin poison self-test without weakening the gate-common candidate detector | Failed host seal `out/phase1_host_seal_final_51f4_20260731T220000Z` is an exact 19/20 `SEAL_FAIL`: `/tmp/anubis-phase1-host-seal-final-51f4-20260731T220000Z.log` SHA `605c6a6c6b82d178739d662b0f258963e937caa58e4e9d055426a930ec50710d`; `out/phase1_host_seal_final_51f4_20260731T220000Z/seal_verdict.json` SHA `f8362c8dad9b0509988cbf158d2e7f8391eccc57efd97310af7a87fd19d4b0da`; the sole row is `GATE_COMMON_ADOPTION_GATE: FAIL (candidates=48 migrated=18 exceptions=29 anomalies=1; 1 fixture(s) failed)` | A single named exception records that this script poison-tests temporary inventory/publication fixtures rather than parsing language expectations; `/tmp/anubis-phase1-gate-common-adoption-green.log`, SHA `f5372c92665c06a7ac8ae1ae02552f696d803bdb627cc888d9b8daae7615d1c0`, ends `GATE_COMMON_ADOPTION_GATE: PASS (candidates=48 migrated=18 exceptions=30 anomalies=0)`, while the underlying poison suite remains 27/27 |

No pre-fix RED was discarded because a later gate became green.

The portable controlled-RED/GREEN bundle contains 16 hash-bound files. Its
`out/phase1_controlled_red_green_postmetrics_20260731T190500Z/MANIFEST.sha256` has SHA-256
`c6107fc3024c15488a2007b6e1cea256633190fa59ea4ccac9007de2f59795fb`, and all 16 entries rehash.
The four later harness-policy/portability RED/GREEN pairs are not retroactively inserted into that
sealed 16-file bundle; the external finalization receipt separately hash-binds the six earlier
named logs plus the adoption RED seal/verdict and focused GREEN log.

## 4. Over-rejection guard — for every enforcing change

| Enforcing change | Guard fixture | Proof the accept-side still accepts |
|---|---|---|
| Strict PCA v2 rejects retired, unknown, legacy, missing, and tampered claims | Clean unsigned PCA and signed PCA with matching public key | `PASS verify_clean_passes (=0)`; `PASS verify_signed_passes (=0)`; `PASS verify_pubkey_match (=0)` |
| Program-derived Research boundary rejects missing consent and host lowering | Authorized disposable-guest Build and a Safe program with redundant `--allow-research` | `research_guest_consent ... rc=0 ... PASS`; `safe_redundant_flag ... rc=0 ... PASS` |
| Research-block local-field collector traverses nested blocks | Direct field access, live `if 1 == 1` carrier, dead `if 1 == 2` carrier, ordinary Safe twin | all four `CASE_RESULT` lines report `expectation=accept rc=0 ... verdict=PASS` |
| Build evidence is program-specific and non-PASS fails closed | Clean requested program beside a rejected sibling and old rejected evidence | `manifestless_build_evidence_binds_only_the_requested_program ... ok` and exact requested bytes/hash are asserted by the test |
| Offensive receipt validator rejects stale/malformed evidence | Exact allow-listed guest export with matching report size/hash and `secret_scan: PASS` | original validation and independent unique-path revalidation both emit `OFFENSIVE_EVIDENCE_VALIDATE_PASS` |
| Verdict-diff binds exact old binary and metadata | Immutable `281e…` pin, metadata SHA `0dc5…`, immutable `51f4…` new pin | real diff verdict `PASS`; both pin modes `0555`, both metadata modes `0444`, opening and closing pin verification true |
| VM validator rejects protocol ambiguity, duplicate/missing gates, and unsafe fixpoint inputs | Final exact 22-name protocol, one fixpoint, one log binding, jobs `[3]` | `VM_BATTERY_VALIDATE_PASS gates=22`; every transport exit code is zero |
| Metrics family classifier excludes only genuinely structure-free adapters | Thin shared adapters and a deliberately structural wrapper in the controlled suite | `thin_shared_adapters_not_families ... families=4`; `structural_wrapper_is_family ... families=5`; suite `10 passed, 0 failed` |

```text
0-flip verdict-diff: 327 security + 253 language fixtures, accept→reject flips: 0
Full bound inventory: 921 files (420 examples + 501 tests/fixtures), flips: 0, timeouts: 0, rc changes: 0
Old/new classifications: ACCEPT 490, REJECT 431
```

The diff says exactly what it measured: `check` classification over the 921-file inventory. It is
not runtime equivalence and it is not a proof about programs outside that inventory.

## 5. ▸ FALSIFICATION — try to break your own closure ▸ **the section that matters**

The Phase 1 semantic twins ran inside disposable Tart guest `anubis-vz-ephemeral-23472`, not on the
host. The pin verified before launch, the guest used 8 vCPU / 5,120 MiB, all nine cases passed, the
teardown guard returned PASS, and the guest was absent from the final Tart inventory.

| Claim | Direct twin | Carrier twin | Dead-branch twin | Verdict |
|---|---|---|---|---|
| Research Build cannot silently acquire the Research lane | `research_direct`: missing consent rejects rc `1` with `ANUBIS_BUILD_RESEARCH_REQUIRES_ALLOW` | `research_carrier`: a later Research function behind a Safe first function and live `if 1 == 1` still rejects rc `1` | `research_dead_branch`: Research body containing a dead branch still rejects rc `1` | **SURVIVED** |
| Authorized Research Build is not over-rejected | `research_guest_consent`: same Research source accepts rc `0` only with explicit consent in the approved guest | `safe_redundant_flag`: redundant flag on a Safe program accepts rc `0` without changing its lane | N/A — the rejecting dead-branch twin is in the row above | **SURVIVED** |
| Research local-field repair is structural, not one-fixture special casing | `local_direct`: Research-local record field accepts rc `0` | `local_live_carrier`: wrapped in `if 1 == 1`, accepts rc `0` | `local_dead_branch`: wrapped in `if 1 == 2`, accepts rc `0`; ordinary Safe twin also accepts rc `0` | **SURVIVED** |
| PCA v2 cannot resurrect the retired taint theorem | Rehashed v2 with retired field fails rc `1` | Rehashed v1, unknown-field v2, and missing-PCA carriers each fail rc `1` | N/A — schema identity is not control-flow sensitive | **SURVIVED** |
| PCA strictness does not reject legitimate evidence | Clean PCA verifies rc `0` | Signed PCA with matching key verifies rc `0` | N/A — schema control | **SURVIVED** |
| Offensive export is one internally bound object | Strict validator recomputes report hash/size | Allow-list manifest, binary hash, memory/jobs, isolation, secret scan, and teardown are all rechecked | N/A — receipt identity is not control-flow sensitive | **SURVIVED twice** — original and independent unique-path validation |

I tried to break the Research/local-field closure **9 ways** in the dedicated guest: three missing-
consent Research shapes, two consent/Safe controls, and four local-field accept shapes. All nine
behaved as required. I tried to break PCA semantics with **8 poison/control mutations**: retired
field, v1, unknown field, missing PCA, source tamper, claim tamper, public-key mismatch, and signed-
claim tamper. Every poison failed closed, while clean unsigned and signed controls accepted.

What survived was the bounded Phase 1 falsification matrix and its named technical claims—not an
early closure declaration. What did not disappear is the published residual soundness surface:
item 21 carriers and the whole unenumerated builtin runtime domain remain open.

Retained falsification evidence:

| Artifact | SHA-256 |
|---|---|
| `out/phase1_falsification_51f4_postmetrics_v2_20260731T181800Z/guest_falsification.stdout` | `074645baa36d8b83a6ce38a69e05c255adef33bcf6c6634ac49d43805ed996d0` |
| `out/phase1_falsification_51f4_postmetrics_v2_20260731T181800Z/isolation.txt` | `0de9b5a55b5972ebdc587381d9a14b253c9f5de988fa9e0c6703858d16342728` |
| `out/phase1_falsification_51f4_postmetrics_v2_20260731T181800Z/tart_after_teardown.json` | `203b45994151d8c4f7f45a5644592ff42705ddada19fc7ce0f5b1d6165c75470` |
| `/tmp/anubis-phase1-pca-postmetrics-final-20260731T190000Z.log` | `27e494fe2e4183d0c01e11c5f84c6e7a3fc1b2b92b895d2001d9552195a0ffc4` |

## 6. The audit re-run

```text
Workflow({scriptPath: ".claude/projects/…/workflows/scripts/anubis-completeness-audit-wf_849c0fb2-478.js"})
```

**`[SKIPPED — the controlling blueprint declares this harness inadmissible]`**

The workflow is reported present, but it still conflicts with current policy in three ways:

1. it requires mutable `./target/release/anubis` rather than one immutable verified pin;
2. it invites host runtime-effect witnesses rather than disposable-guest evidence;
3. it uses obsolete CLAIMS list numbering.

No surface count or finding count from that workflow is promoted here. This is neither a quiet
omission nor a PASS. Per the blueprint, repairing it is a **Phase 2.0 prerequisite** (`Phase 6.5`
pulled forward): pin-bind the compiler, move runtime-effect witnesses into the guest, and update the
stable claim identifiers before Phase 2 begins. Phase 1's explicit technical exit criteria were
instead exercised by the poison gates, 9-case guest falsification, full 22-gate VM battery,
34-case offensive guest, and exact 921-file diff above.

## 7. Convergence metrics — the numbers that decide progress

Phase start, run at commit `0e910c9b…`, dirty tree `156`; artifact SHA
`80dda2217a951c63977b11f34cdc0d085483fd71aab90c8601753b1c923564be`:

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 156 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

Phase end, run after the classifier correction at the same commit, dirty tree `223`; artifact SHA
`fe7de6213409ab6d11d92f9d28d651715b753533a493aa0e38e6bd6adb58b172`:

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 223 entries

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
```

| Metric | Phase start | Phase end | Direction |
|---|---:|---:|---|
| `middle/mod.rs` lines | `28,801` | `28,604` | down `197` |
| Duplicated lane pairs | `4` / `2,498` lines | `4` / `1,934` lines | pair count flat; duplicate lines down `564` |
| Fused cross-lane joins | `1` / `19` call sites | `1` / `16` call sites | join count flat; call sites down `3` |
| `_ =>` in label-lane walkers | `12` | `7` | down `5` |
| Lane facts with no join | `1` | `1` | flat; still Phase 2 work |
| Walker families | `5` | `4` | down `1`, satisfying non-increasing |

The family change is not an unconditional subtraction. The controlled `10/10` metric suite proves
that only structure-free taint/secret adapters over the shared statement traversal cease to count;
a wrapper that regains AST matching or control flow is counted again. The remaining flat metrics
are carried into Phase 2, not mislabeled as Phase 1 regressions.

## 8. Seal and CI

```text
VM battery:   22/22 EXIT=0, validator PASS, fixpoint
              46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60
Expected:     scripts/vm/EXPECTED_FIXPOINT_VM matches? yes
VM guest:     anubis-run-23962, torn down and absent? yes
VM resources: 8 vCPU, 5120 MiB, build jobs 3, host reserve unchanged at 8192 MiB
Offensive:    34/34, strict validator PASS twice
Offense guest: anubis-offensive-gate-41607, torn down and absent? yes
Falsification guest: anubis-vz-ephemeral-23472, 9/9, torn down and absent? yes
Host seal authority: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_host_seal_final_51f4_r2_20260731T230000Z/seal_verdict.json
Host-seal predicate: status=SEAL_PASS; profile=core; exact roster=20/20; captured exit=0;
                     independent seal-verdict validator=PASS
Source-current VM refresh: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_vm_51f4_final_epoch_r2_20260731T210000Z
Source-current offensive refresh: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_offensive_51f4_final_epoch_r2_20260731T220000Z
Source-current verdict diff: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_verdict_diff_281e_to_51f4_final_epoch_r2_20260731T223000Z.json
Finalization receipt: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_finalization_51f4_r2_20260731T230000Z/receipt.md
Independent review: [PENDING — predetermined path] /Users/sicarii/anubis-lang/out/phase1_finalization_51f4_r2_20260731T230000Z/independent_review.md
CI run:       [NOT VERIFIED] URL unavailable; conclusion unavailable; gates unavailable
```

The technical-epoch VM verdict has exactly one completion marker, exactly the 22 expected gate names, no
unknown/missing gates, every exit code zero, jobs `[3]`, one expected fixpoint, one log binding, and
all five transport/validator codes zero. Source tree `0281e803…` was identical before sync, at sync,
and after the run. `MANIFEST.sha256` rehashes all 31 retained files successfully.

Selected technical-epoch receipts:

| Artifact | SHA-256 |
|---|---|
| `/tmp/anubis-phase1-vm-51f4-postmetrics-final-20260731T182200Z.log` | `4bf9445bbaa0c51b9a0d251d4ea956512a24bed73fb49208dae9ff44d7d32446` |
| `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z/battery.log` | `85058167c1cc4b30f27647d784d0384cf1c3cf3194bf9cb3eb6219096cf2d5e4` |
| `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z/battery.protocol` | `2ccd78aa9520d70b19921c89f8995d7068b1f3ef9690c66a9f41454efe7e924f` |
| `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z/battery_verdict.json` | `71c0abebf6b4da7143ef57d0acf41b69e2ace8e9f86af2629172bb45f87feed3` |
| `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z/cargo-test.json` | `ac8877e7065e7dd79634a7048fba731f1e70ff32bc8f02e4f5718ac5c61200f8` |
| `out/phase1_vm_51f4_postmetrics_final_20260731T182200Z/MANIFEST.sha256` | `3330e1d9b82435010fe79bc42155e3b1518991dfa869df566dddd668e13b578c` |
| `/tmp/anubis-phase1-offensive-51f4-postmetrics-final-20260731T185000Z.log` | `cce0532164e3c53bd4a91b40cd13d382e82cec9e2e06a09321a27ff7cac40621` |
| `out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z/report.json` | `1f93c22c8b9cd37124b50680e3b1bad70dade178b362060736019616023b18ee` |
| `out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z/export_manifest.json` | `d1ef0a556f512af59eeb801ff817a20ae8596fabe07969fe534e3ecc33c00b71` |
| `out/phase1_offensive_51f4_postmetrics_final_20260731T185000Z/offensive_verdict.json` | `05f2a561cad7c6e72c9738bccc751e319fb0e1377c6065c77014256a00cbfa99` |
| independent offensive revalidation log | `2d9bf4469af9944722bf13afe3e262a4362e10f3c288447863e6a73e23dff536` |
| `out/phase1_verdict_diff_281e_to_51f4_postmetrics_20260731T185400Z.json` | `1ebdc8b1c352a183d1fcd9bb3a0e2ee6f7950da2b90c7a0903399fbe634c8372` |
| verdict-diff log | `69d5838b5cc7272eef8a6c485a2a9c55f364fcea2deb4fcc6d3078f133ee1a04` |

The offensive export allow-lists only `report.json`, binds its exact 4,157 bytes and hash, records
`secret_scan: PASS`, records the exact binary SHA, and has `teardown_status: torn_down`. The
independent revalidation wrote a fresh verdict at a unique `/tmp` path with the same verdict hash;
it did not consume an old PASS file.

## 9. What I did NOT verify

- `[SKIPPED — controlling blueprint declares it inadmissible]` The completeness-audit workflow in
  section 6. Repair is a Phase 2.0 prerequisite.
- `[NOT VERIFIED]` Any GitHub Actions URL or conclusion. No queued/skipped job is represented as a
  pass.
- `[PENDING — external authority]` The final source-current metadata publication and strict pin/tree
  verification do not exist at report-freeze time.
- `[PENDING — predetermined paths]` The source-current VM 22/22 refresh, offensive 34/34 refresh,
  and 921-row zero-flip diff named in section 8 do not exist at report-freeze time.
- `[PENDING — predetermined paths]` The final 20-row host seal, finalization receipt, and independent
  review named in section 8 do not exist at report-freeze time. Only the external activation
  predicate may promote them after they are created and validated.
- `[NOT VERIFIED]` A commit-bound, pushed, merged, release-tagged, notarized, or shipped result.
- `[NOT VERIFIED]` Universal absence of unknown Anubis soundness defects. This report closes the
  enumerated Phase 1 claims, not all possible value-flow paths.
- `[NOT VERIFIED]` `docs/CLAIMS.md` item 21 carriers. They remain load-bearing and open.
- `[NOT VERIFIED]` Total fail-closed behavior over the roughly 213-builtin `anubis run` domain,
  arity, wrong-type, and I/O surface.
- `[NOT VERIFIED]` Apple code signing, notarization, entitlements, framework ABI, Swift/Objective-C
  bridging, Secure Enclave, Metal proof workloads, or Apple-platform release claims. Those belong
  to later roadmap phases and were not smuggled into Phase 1.
- `[NOT VERIFIED]` Ownership of every unrelated dirty entry in the shared worktree. No blanket
  cleanup or staging was attempted.
- `[REPORTED, NOT PROMOTED]` Older `a6f7…` / `anubis-run-65901` /
  `anubis-offensive-gate-82951` receipts remain historical. The final disposition uses the `51f4…`
  receipts above.

This section is intentionally non-empty. Nothing here is converted into a silent PASS.

## 10. What I got wrong during this phase

1. The 2026-07-30 record called the bounded work complete before the later review exposed an
   internally mismatched offensive receipt. A `34/34` stdout line and a teardown file did not prove
   that the checked host report was the exact manifest-bound guest object. The strict validator and
   independent fresh-path revalidation now close that identity gap.
2. `scripts/publish_pin.sh --current` was initially treated as if it published current metadata. It
   only reads the selected pin. The correction was a lead-owned release build, an explicit publish,
   then `--verify` before measurement.
3. Guest `anubis-run-52393` passed admission but runtime boot/sync overhead later crossed the
   8,192 MiB reserve. The independent guard stopped the guest and verified deletion. An earlier
   controlling-session note recorded an exact process status and free-memory value, but those two
   values are not present in the retained log, so this report retracts rather than promotes them.
   The recoverable log evidence is `GUEST_STOPPED` plus teardown PASS at
   `/tmp/anubis-phase1-vm-slice-51f4-final-source-20260731T154724Z.log`, SHA
   `e81714aed9412c0ed3d5888748f2f6b348c6273cabec3227591060140f937f8a`. That is a successful safety
   refusal, not a battery failure and not permission to lower the reserve.
4. Guest `anubis-run-83786` did useful work, but the wrapper consumed stale human prose for the
   fixpoint and BSD/GNU `stat` disagreement corrupted the machine protocol. The wrapper correctly
   refused the receipt. Exact machine artifacts and a no-follow regular-file reader replaced that
   path; the failed evidence remains preserved at
   `/tmp/anubis-phase1-vm-51f4-source-freeze-20260731T163455Z.log`, SHA
   `f9afd01f04dc930c8ed7918c5e506a33ae5c71dc79a4fa1c8f07f62f86c32c66`.
5. Guest `anubis-run-9606` failed before gates because PATH selected
   `/opt/homebrew/bin/timeout` instead of the required GNU `gnubin` path. The deterministic PATH
   order was corrected and the next full guest completed. Retained log:
   `/tmp/anubis-phase1-vm-51f4-harness-fixed-20260731T172000Z.log`, SHA
   `850004fcc256bd6a7b64578677f7f2fdfce11a6ef017ebfc399f425bf9ba65a3`.
6. A preflight helper was sourced once under zsh even though it relies on Bash semantics;
   zsh's readonly `status` and different `BASH_SOURCE` behavior invalidated that attempt. It was
   rerun under `/bin/bash` rather than explained away.
7. The first phase-end metric read six families because the instrument counted two structure-free
   domain adapters as independent AST walkers. I did not simply hand-edit the number: a controlled
   RED/GREEN suite now proves thin adapters count as shared while any wrapper that regains AST
   matching/control flow counts as a family. The measured result is five to four.
8. The first verdict-diff helper did not bind the exact old binary and metadata digests, and the
   first offensive validator could leave a stale PASS on early argument failure. Controlled
   mutants reproduced both defects before the real 921-file diff and fresh offensive receipt ran.
9. The first final docs-bound host seal at
   `out/phase1_host_seal_final_51f4_20260731T193000Z` terminated at Bash's
   `mapfile: command not found` after the security corpus itself passed 327/327: the structured
   scorer used Bash 4's `mapfile`, which macOS system Bash 3.2 does not provide. The controlling
   session observed exit `127`, but that value was printed outside the redirected log and is not
   promoted as a durable file receipt. The failed directory and log SHA
   `8347b368d39ed08ef41a8e366c844d48e1646941608b7c62c0cdb6415a024051` are preserved. A new
   regression test was RED first, then the scorer was changed to four explicit Bash-3-compatible
   reads; no gate threshold, roster entry, or declared-verdict rule changed.
10. A preseal corpus/pin poison rerun reached BSD `rm`'s interactive override prompt because the
    test deliberately made scratch pin metadata read-only and then removed it without `-f`. The
    incomplete run emitted no verdict and was terminated; it is not counted. The two exact
    scratch-metadata removals now use `rm -f --`, and an interactive-PTY rerun passed 27/27. This
    changes only test cleanup behavior, not the pin publication or verification contract.
11. The structured scorer initially appended `declared_marker_count=N` to the policy reason string.
    That remained fail-closed for the active roster but made the exact no-marker diagnostic and
    known-failing comparisons unreachable. Two focused tests reproduced both failures through the
    actual shell function before `_v_reason` and `_v_marker_count` were separated; the eight-test
    suite, exact-roster validator, and Bash syntax check are now green.
12. The first source-current final seal reached all 20 core rows but correctly finished
    `SEAL_FAIL` at 19/20 because `scripts/test_corpus_inventory_binding.sh` matched the conservative
    gate-common candidate heuristic without a declared role. The underlying poison self-test was
    green 27/27; it creates temporary `.anb` files to challenge the inventory and pin trust spine
    rather than scoring language expectations. I did not suppress the detector or reuse the failed
    seal. A single explicit exception with that narrow reason makes the adoption gate 48/48, and
    this report names a wholly new source-bound VM/offensive/diff/seal/finalization epoch.

These corrections are part of the evidence, not editorial footnotes.

## 11. Landing state

```text
Commits:      none for the Phase 1 takeover; local HEAD remains
              0e910c9bb2e83438696eaaf0f49d0e3c5e658960
Pushed:       not performed; upstream SHA == local HEAD only because no new commit exists
Dirty:        224 status entries
Mine:         final-repair lane changed scripts/check_gate_common_adoption.sh plus the seven
              activation restatements in AGENTS.md, MATURITY_CLAIM_MATRIX.md, docs/CLAIMS.md,
              docs/HANDOFF.md, docs/HANDOFF_LIVE.md, and the July-30/July-31 Phase-1 reports
Untouched:    [NOT DETERMINED] mixed staged/unstaged/untracked entries belong to shared sessions;
              no ownership count is fabricated
Rollback:     [NEEDS-HUMAN] reverse only the reviewed final-repair hunks; the July-31 report alone
              is untracked and can be removed with the exact rm command below
```

The report-only rollback is
`rm -- /Users/sicarii/anubis-lang/docs/evidence/PHASE_1_COMPLETION_2026-07-31.md`; it removes only
this untracked report. There is no safe single-command
rollback for the complete Phase 1 slice because its changes are interleaved with pre-existing and
other-session dirty work. Any broader rollback requires an explicit reviewed path/hunk manifest.
No `git add`, commit, push, merge, branch rewrite, or cleanup was performed by this takeover.
Code and documentation remain unlanded and unshipped.

## 12. Sign-off block

```text
Phase 1:  COMPLETE iff the external finalization receipt satisfies the activation predicate;
          otherwise INCOMPLETE — always UNCOMMITTED / UNLANDED / UNSHIPPED
Blocking: final source-current repin; VM/offensive/921-row-diff refreshes; exact host seal;
          external receipt; independent review. Until explicit operator GO, the next phase is blocked
Recommend: STOP — operator reads this report, then either says go or sends Phase 1 back for repair
```

Finalization invariant: the predetermined external host-seal authority named in section 8 must
contain the actual result from

```text
bash scripts/run_seal_checklist.sh \
  --out out/phase1_host_seal_final_51f4_r2_20260731T230000Z \
  --bin vm/pins/anubis-51f4a964347a \
  --profile core
```

and the external finalization receipt must validate it. That command must exit `0` after the final
source-current repin. This report's `COMPLETE`
disposition activates only when the external finalization receipt records the captured exit code,
the exact 20-row core roster, the validated `SEAL_PASS` verdict, the unchanged executable SHA, and
the final source-tree/metadata digests, and a post-seal independent read-only review records
`APPROVE` with no blocking finding and zero reviewer writes to source-controlled or pin-manifest-
eligible paths. The operator may capture that read-only verdict only in the ignored external review
artifact named in section 8. Missing or invalid
seal/review evidence changes the sign-off to `INCOMPLETE`; it is never waived.

Because the failed finalization exposed manifest-eligible harness defects after
the original `0281e803…` guest epoch, activation also requires source-current refresh receipts at
the three paths in section 8: VM 22/22 with verified teardown; offensive 34/34 with strict identity
and teardown, bracketed by strict pin verification and full source-manifest hashes before and after;
and the exact 921-row old/new diff with zero flips/timeouts/rc changes. These external
receipts supplement rather than rewrite the historical technical-epoch rows above.

**Mandatory stop:** do not begin Phase 1.5 or Phase 2 until the operator has read this report and
explicitly said go.
