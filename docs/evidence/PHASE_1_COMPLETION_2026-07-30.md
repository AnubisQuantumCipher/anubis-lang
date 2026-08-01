# Phase 1 — Evidence Integrity + Research Isolation Completion Record

> **HISTORICAL / SUPERSEDED — 2026-07-31.** This file preserves the 2026-07-30 measurements and
> decisions as recorded; it is not current Phase-1 completion authority. Its offensive export was
> later shown to have a report/manifest identity mismatch and remains RED history. The current
> bounded technical receipt is `PHASE_1_COMPLETION_2026-07-31.md`. Factual correction: the source
> manifest includes documentation, so documentation edits were never "outside the pin source
> manifest" and could not preserve pin/tree identity. The superseding 2026-07-31 report activates
> bounded acceptance only when its external finalization receipt proves the source-current
> VM/offensive/921-row-diff refreshes required by that report §§8/12 pass and the final docs-bound host
> seal at `out/phase1_host_seal_final_51f4_r2_20260731T230000Z` satisfies the required
> `SEAL_PASS`/20-row/captured-exit/validator predicate and a final read-only review records
> `APPROVE` with no blocking finding.

**Date:** 2026-07-30<br>
**Controlling blueprint:** `.hermes/desktop-attachments/ANUBIS_FINAL_BLUEPRINT_PHASES_2026-07-29-4.md`, Phase 1<br>
**Operator transition:** `OK now go to the next phase and complete it`<br>
**Repository:** `/Users/sicarii/anubis-lang`<br>
**Branch:** `a-plus-maturity/safe-mode-trust-spine-20260725`<br>
**HEAD / upstream:** `0e910c9bb2e83438696eaaf0f49d0e3c5e658960` / same<br>
**Terminal status:** **COMPLETE FOR THE BOUNDED WORKING-TREE RECEIPT / UNLANDED — every Phase-1 technical gate is green; no commit or shipping claim is made**

This record seals the measured dirty tree only: immutable pin, source-bound host checklist, full
disposable-guest battery, offensive guest battery, teardown, corpus binding, walker completeness,
and independent source review. It does not promote uncommitted work into landed or shipped work.

## 1. Goal

Phase 1 had three bounded implementation goals:

1. stop PCA evidence from asserting an independently unearned taint-clean theorem;
2. make Research/Exploit native lowering and execution depend on complete-program mode, explicit operator consent, and the disposable-VZ boundary;
3. close the Research-block local-record field-access false rejection without weakening Safe behavior.

The implementation reaches those goals and the mandatory host, disposable-guest, offensive,
inventory, walker, and teardown evidence. The initial 12 GiB guest admission was correctly refused;
later runs used a smaller supported guest allocation and fewer build jobs without changing the
8,192 MiB host reserve or bypassing either guard.

## 2. Inputs and baseline

- Phase-0 report read before implementation: `docs/evidence/PHASE_0_COMPLETION_2026-07-30.md`.
- Phase-0 controlling baseline preserved at admission: walker completeness RED with 18 findings;
  item 20 OPEN; no pre-fix guest receipt reused. Both conditions were subsequently closed with fresh
  evidence rather than rewritten retrospectively.
- Start metrics artifact: `/tmp/anubis-phase1-start-metrics.txt`.
- Start metrics: `middle/mod.rs=28,801`, duplicated lane pairs `4`, duplicated-pair lines `2,498`, source-walker pair `1,247` lines / `69%`, fused joins `1`, wildcard label arms `12`, walker families `5`.
- Start tree: HEAD `0e910c9b…`, dirty `156` entries.
- Pre-change comparison instrument: `vm/pins/anubis-281e0e846948`, SHA-256 `281e0e8469484b72a954a2570a3fd92d3cda18cb2e615a26933b73640dae5262`.

## 3. Changes made

### 3.1 PCA v2 evidence integrity

- `compiler/src/evidence/mod.rs:693-737`
  - schema version is 2;
  - independent `taint_clean` field removed;
  - `ClaimBlock` now uses `#[serde(deny_unknown_fields)]`.
- `compiler/src/evidence/mod.rs:901-923`
  - missing `pca.json` returns semantic failure instead of hash-only success;
  - semantic verification still re-derives the bounded claim from `source.anubis`.
- Poison coverage:
  - known accepted leak cannot serialize `taint_clean`;
  - consistently rehashed v1 claim rejects;
  - consistently rehashed v2-plus-retired-field rejects;
  - consistently rehashed bundle with no PCA rejects.
- `scripts/run_pca_gate.sh` expanded from 13 to 18 cases; floor raised to 18 without removing any prior control.
- Committed real RISC0 receipt wrapper migrated from PCA v1 to strict PCA v2:
  - `tests/fixtures/zk_prove_bundle/pca.json`;
  - `tests/fixtures/zk_prove_bundle/MANIFEST.sha256`.
  - Receipt, ImageID, and journal bytes were not replaced.

### 3.2 Complete-program Research boundary

- `tools/anubis/src/main.rs:6720-6777`
  - one mode-derived decision function distinguishes Safe, missing consent, and disposable-VZ-required;
  - a redundant `--allow-research` on a Safe program does not enable research lowering.
- Product callers now consult that predicate:
  - REPL: `tools/anubis/src/main.rs:2031-2044`;
  - Build: `tools/anubis/src/main.rs:2450-2472`;
  - Prove: `tools/anubis/src/main.rs:4786-4795`;
  - Run: `tools/anubis/src/main.rs:5692-5724`;
  - lower signed/unsigned Run helpers recheck at `tools/anubis/src/main.rs:7085-7087,7155-7157`.
- `build` and `prove` now expose explicit `--allow-research`.
- Build, Run, Prove, and REPL reject Research/Exploit without consent and reject consented Research/Exploit on the host before lowering/artifact/execution.
- Run preserves ordinary checker rejections before emitting the secondary consent diagnostic; canonical check/run parity returned `PASS_WITH_KNOWN_NON_RUN`.
- `compiler/src/backends/run.rs:6052-6116`
  - signed execution has a second explicit Anubis-VZ-marker guard;
  - production signed execution preserves parse/typecheck failure;
  - invalid-source runtime-only helpers are test-only/private rather than public product APIs.

### 3.3 Research local-field false rejection

- `compiler/src/backends/run.rs:156-347`
  - local-binding collection is total over current `Stmt` and `Expr` variants;
  - Research/Exploit/Hybrid bodies and invariant expressions are traversed;
  - wildcard catch-alls were removed from this security-relevant walker.
- `tests/fixtures/language_core/research_block_local_field_access_accepts.anb` added as an accept fixture.
- `research_block_local_field_access_and_ordinary_twin_both_lower` checks both the Research-block reproducer and an ordinary Safe twin.

## 4. Removals and deprecations

- Retired: PCA-v1 `taint_clean` as a semantic claim.
- Retired: accepting unknown PCA claim fields.
- Retired: treating missing `pca.json` as PCA success.
- Retired: Build/Prove research permission inferred from `ir.has_research` or taint metadata.
- Retired: public raw invalid-source execute helpers; the fallback remains only as an explicitly test-only runtime seam.
- Preserved but migrated: the real committed RISC0 receipt fixture, now wrapped by PCA v2 metadata.
- Not removed: open item 21 carrier witnesses or any other named residual.
- Subsequently retired under explicit sign-off: the two incomplete reduced block-label walkers;
  one shared total walker now discharges their registered completeness contract.

## 5. Tests run and exact results

| Command / artifact | Result | Scope and caveat |
|---|---:|---|
| `cargo test -p anubis-compiler --lib -- --nocapture` | **PASS — 771/771** | Final audited Rust source; `/tmp/anubis-phase1-compiler-lib-final-audited.txt` |
| `cargo test -p anubis -- --nocapture` | **PASS** | Unit `359/359`; integration binaries `6 + 1 + 1 + 2 + 1 + 1 + 5`, all green; `/tmp/anubis-phase1-tool-full-final-audited.txt` |
| `cargo test -p anubis-compiler evidence::pca_tests` | **PASS — 12/12** | Strict-v2, missing-PCA, legacy, tamper, signature, receipt-binding controls |
| `bash scripts/run_pca_gate.sh --out out/phase1_pca_gate` | **PASS — 18/18** | Includes rehashed v1, v2-plus-retired-field, and missing-PCA poisons |
| `bash scripts/run_prove_gate.sh` | **PASS — 11/11** | Migrated PCA-v2 real receipt cold-verifies; tamper and wrong ImageID reject |
| `cargo test -p anubis --test safe_mode_program_gate` | **PASS — 5/5** | Build consent/VZ, artifact absence, Safe redundant-flag accept guard |
| `cargo test -p anubis --test research_policy_callers` | **PASS — 2/2** | Prove and REPL caller closure |
| `cargo test -p anubis --test research_rejection_parity` | **PASS — 1/1** | Run preserves check rejection before consent |
| `cargo fmt --all -- --check` | **PASS** | Final source |
| `cargo clippy -p anubis-compiler --all-targets -- -D warnings` | **PASS** | Final source |
| `ANUBIS_SKIP_RISC0_METAL=1 cargo clippy -p anubis --all-targets -- -D warnings` | **PASS** | Tool lint only; not Metal execution evidence |
| unqualified all-target tool Clippy | **BLOCKED** | Host Xcode lacks the Metal Toolchain component; the failure is retained in `/tmp/anubis-phase1-clippy.txt` |
| promise policy/self/live + Phase-0 record tests/self/live | **PASS** | All eight deciding return codes were zero before final inventory reconciliation |
| `bash scripts/run_walker_completeness_gate.sh` | **PASS — 0 findings** | Shared total `walk_block_labels`; `/tmp/anubis-phase1-walker-shared-green.txt` |
| `bash scripts/test_corpus_inventory_binding.sh` | **PASS — 9/9** | Tracked corpus, all-example pin binding, count-floor/root-floor mutation poisons |
| `bash scripts/test_host_resource_guard.sh` | **PASS — 49/49** | Guard limits, reserve admission, runtime watch, orphan cleanup, teardown |
| effect / type / dogfood selfhost gates | **PASS — 27/27, 20/20, 3/3** | Missing-floor failures retained, then floors set to observed nonzero complete corpora |
| `scripts/run_seal_checklist.sh --bin vm/pins/anubis-a6f7f05fd132` | **SEAL_PASS — 19/19** | Source-bound host receipt, 0 skip/known-fail |
| `ANUBIS_VM_MEM=5120 ANUBIS_VM_BUILD_JOBS=3 scripts/vm/run-slice.sh` | **PASS — 22/22, 0 failures** | Guest `anubis-run-65901`; fixpoint unchanged; teardown verified |
| `ANUBIS_OFFENSIVE_GATE_VM_MEM=5120 scripts/run_offensive_platform_gate.sh` | **PASS — 34/34** | Guest `anubis-offensive-gate-82951`; export secret scan PASS; teardown verified |
| final old/new fixture verdict diff | **PASS — 921 files, 0 flips, 0 timeouts** | `out/phase1_verdict_diff_complete.json` |

RED-first evidence retained:

- PCA-v2 unknown-field poison: rc 101 before strict parsing (`/tmp/p1-pca-v2-unknown-red.txt`).
- Missing-PCA downgrade poison: rc 101 before fail-closed behavior (`/tmp/p1-pca-missing-red.txt`).
- Research Build without consent emitted an artifact before the fix (`/tmp/anubis-phase1-research-build-red.txt`).
- Research-block local field lowering failed with unknown local before the fix (`/tmp/anubis-phase1-research-field-red.txt`).
- Run/check rejection parity failed before ordering was corrected (`/tmp/anubis-phase1-run-rejection-parity-red.txt`).
- Prove gate failed 8/11 before the real receipt wrapper migrated to v2 (`/tmp/anubis-phase1-prove-gate-legacy-red.txt`).

## 6. Artifacts and hashes

Final immutable candidate:

```text
path:    vm/pins/anubis-a6f7f05fd132
sha256:  a6f7f05fd132ed7ad9891b2884acf15e80625ba3f7f967939cbf808804320793
meta:    d7378309132ea7a24f950a715be8957461458431290765d88d78c8dc634ce3e7
src_tree 658f3ebaa4274b168f61519beac9dfcd3560d07a3aa653e68cc287521df400ca
mode:    -r-xr-xr-x
```

`scripts/publish_pin.sh --verify` returned `pin matches tree: vm/pins/anubis-a6f7f05fd132` before and
after the deciding host/guest runs. Subsequent edits in this record were documentation-only, but
documentation is included in the pin source manifest; those edits therefore moved the source
identity. The original claim that they were outside the manifest was incorrect, and this pin/tree
receipt is historical rather than current.

Selected evidence hashes:

| Artifact | SHA-256 |
|---|---|
| `out/phase1_host_seal_complete_final_20260730T191739Z/seal_verdict.json` | `8f16864550e3860a1d96315377d0ab1fb0b120fa9f9bf9f3ba336409c18ace26` |
| `out/phase1_host_seal_complete_final_20260730T191739Z/instrument.txt` | `1a21e99857191e5bf9331b2890d0a07aa039d6e4137fa6a7acac005b3bfa7a2d` |
| `out/phase1_verdict_diff_complete.json` | `68b54f89dd4db61d2e471c7a62a0db862a607a756728b8ac4748dc2faa858551` |
| `/tmp/anubis-phase1-vm-slice-complete-v4.log` | `208b270ea9bc8d41706391b5f266efcf6e877f55d40d890ebbc45f88bb3edcd3` |
| `out/phase1_offensive_complete_v4_20260730T191631Z/report.json` | `17592b9ccbccd0aa40b07e72a30492fc8f86649a43e2eda7581de5e67be29997` |
| `out/phase1_offensive_complete_v4_20260730T191631Z/isolation.json` | `5935f06399e28f13aa0960702fe95a2723d3c5c861cdcdccff572ae263ad07b1` |
| `out/phase1_offensive_complete_v4_20260730T191631Z/export_manifest.json` | `253524c398b9f968ded2f066fda936aca5166b277e8e2f5c9c9785de399856ac` |
| walker completeness green log | `160c0d348cb0d0578ff83f931e24a0d769c76ea91a612f1e60be2e0ce89008af` |
| corpus/pin binding 9/9 log | `81a7816795e04338d8342484f4a6fbd19e480f45b8fe4bb50b6256cafcf5539d` |

## 7. Fixture verdict-diff

A controlled before/after check classified every on-disk `.anb` under `examples/` and `tests/fixtures/`:

```text
old:      vm/pins/anubis-281e0e846948
new:      vm/pins/anubis-a6f7f05fd132
files:    921
flips:    0
timeouts: 0
artifact: out/phase1_verdict_diff_complete.json
```

This establishes zero `check` verdict drift for the measured corpus. It does not generalize beyond those 921 files or to runtime behavior.

## 8. Independent audits

Initial independent source audit:

- artifact: `/Users/sicarii/.hermes/cache/delegation/subagent-summary-0-20260730_093212_784988.txt`;
- SHA-256: `aaf273b8988119b88e53a1e894e3de4ab5d5c02683040f107b895ca36fd03909`;
- decision: **REJECT**;
- zero reviewer writes;
- blockers: v2 unknown-field acceptance, missing-PCA downgrade, v1/prove-gate contradiction, Prove/REPL/public-execution caller gaps.

All listed blockers received source changes and host tests. The second independent re-review
completed:

- artifact: `/Users/sicarii/.hermes/cache/delegation/subagent-summary-0-20260730_114919_225146.txt`;
- SHA-256: `1f3061eaf579e95b810fe07397f77b3cc8b04500b5c604a502d8ede28f617c56`;
- decision: **PASS — bounded STATIC/SOURCE review**;
- remaining source blockers in requested Phase-1 scope: none found;
- zero reviewer writes;
- explicitly not runtime, VM, offensive, verdict-diff, or release-seal evidence.

The independent-audit criterion is therefore **PASS for source review only**. Runtime and isolation
criteria remain governed by the mechanical receipts elsewhere in this report.

## 9. Resolved blockers and remaining residuals

### 9.1 Mandatory VZ/offensive evidence — RESOLVED WITHOUT GUARD BYPASS

The first 12,288 MiB admission attempts were correctly refused at 7,837–7,943 MiB free versus
20,480 MiB required. No clone was created and no host fallback occurred. Later 8,192/7,168 MiB
attempts that transiently crossed the 8,192 MiB host reserve were stopped and deleted by the
independent LaunchAgent; those failures remain in
`~/Library/Logs/anubis-host-resource-guard.log`.

The successful full battery used the supported lower resource envelope
`ANUBIS_VM_MEM=5120 ANUBIS_VM_BUILD_JOBS=3`. The reserve stayed 8,192 MiB. Guest
`anubis-run-65901` completed all 22 named gates with zero failures, produced fixpoint
`46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60`, matched
`scripts/vm/EXPECTED_FIXPOINT_VM`, and was verified absent after teardown.

The final offensive run used `ANUBIS_OFFENSIVE_GATE_VM_MEM=5120`. Guest
`anubis-offensive-gate-82951` passed 34/34, bound the transported binary hash to the immutable pin,
passed the two doctor cases, exported only the allow-listed report with `secret_scan: PASS`, and was
verified `torn_down`. `tart list` afterward contained only the stopped named bases. No offensive,
crash, or fuzz execution fell through to the host.

### 9.2 Cross-gate / pin inventory disagreement — RESOLVED

The five material `.anb` files were reviewed and staged. One shared fail-closed helper now derives
the exact tracked native-authoritative corpus for native grading and docs drift. The pin manifest
binds all `examples/`, `tests/fixtures`, gate scripts, and both `*.count_floor` and root `*.floor`
thresholds. The poison suite proves that an untracked corpus file, a count-floor mutation, a root
gate-floor mutation, and an untracked showcase fixture all invalidate the corresponding trust path.

Current result: **921 tracked files** for both consumers; native-authoritative **0 mismatches, 0
disagreements**; docs drift **48 stamps, 0 drift**; corpus/pin binding **9/9**.

### 9.3 Walker completeness — RESOLVED

The duplicated `walk_block_taint` and `walk_block_secret` statement traversals now delegate to one
total `walk_block_labels` traversal. It visits conditions, while-let scrutinees, loop invariants,
Research/Exploit/Hybrid bodies, and every current `Stmt` variant without a security-relevant
catch-all. The registered completeness gate reports **0 findings** on the host and `EXIT=0` in the
fresh full VM battery.

### 9.4 Residuals that remain open

- `docs/CLAIMS.md` item 21 remains OPEN; PCA v2 bounds its claim rather than hiding those carriers.
- The approximately 213-builtin `run` domain/arity/wrong-type/I/O surface is not fully enumerated;
  the bounded instrumented claim remains the only available claim.
- The bare `anubis` alias remains documented rather than fixed.
- All Phase-1 work remains dirty and unlanded; no shipping claim is available.

## 10. Claims/docs updated

Updated surfaces:

- `docs/CLAIMS.md` — strict PCA-v2 consequence, inventory closure, current VM seal, and item 20
  post-fix guest-evidence closure;
- `docs/CLI.md` — Build/Prove consent flags and shared Build/Run/Prove/Repl boundary;
- `MATURITY_CLAIM_MATRIX.md` — strict v2 fields, 18/18 schema/tamper gate, no taint theorem;
- `docs/HANDOFF.md` / `docs/HANDOFF_LIVE.md` — bounded source/VZ closure with unlanded qualifier;
- `AGENTS.md` — final pin, 921-file shared inventory, walker closure, final host seal, both guest receipts.

The final docs gate passed **48 stamps, 0 drift**. Historical 916-file receipts remain explicitly
dated while every current native-corpus stamp resolves to the shared 921-file inventory.

## 11. Landing state

- **No commit or push was made.**
- HEAD/upstream remain `0e910c9bb2e83438696eaaf0f49d0e3c5e658960`.
- Final measured working tree: `195` status entries; `24` staged paths; `724` untracked paths.
- Native corpus: `921` tracked `.anb` files and **0** untracked `.anb` files under
  `examples/` + `tests/fixtures/`.
- `git diff --check`: PASS.
- Phase-1 code/docs are working-tree-only.
- No unrelated staged content was committed or rewritten.

## 12. Phase disposition / next admission

| Exit criterion | State |
|---|---:|
| Evidence producer no longer asserts unearned taint guarantee | **PASS — host/source** |
| Strict v2, missing PCA, retired v1, prove fixture compatibility | **PASS — 12/12 + 18/18 + 11/11** |
| Build/Run/Prove/Repl shared mode-derived boundary | **PASS — host/source** |
| Research local-record field access accepts with Safe twin | **PASS — host/source** |
| Fixture verdict-diff | **PASS — 921 files, 0 flips, 0 timeouts** |
| Final source-bound host checklist | **SEAL_PASS — 19/19, 0 skip/known-fail** |
| Fresh disposable-guest required-gate seal | **PASS — 22/22, 0 failures, unchanged fixpoint** |
| Offensive platform gate 34/34 | **PASS — source-matched disposable guest** |
| Verified teardown | **PASS — both generated guests absent** |
| Independent re-review | **PASS — bounded static/source; zero writes** |
| Walker completeness | **PASS — shared total walker, 0 findings** |
| Landing / push | **NOT DONE** |

**Decision: Phase 1 is COMPLETE for the bounded working-tree receipt and remains UNLANDED.**

No technical Phase-1 gate remains red. The next action is an architect landing decision: review the
dirty slice, separate code/fixtures from documentation as repository policy requires, commit/push
only explicit reviewed paths, then re-run the commit-bound seal. Do not begin Phase 2 or describe the
work as shipped merely because the working-tree receipt is green.
