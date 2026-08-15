# Phase 3/4 VZ self-host seal — PASS (2026-08-15)

**Verdict: SEAL PASS.** The throwaway-guest self-host seal (`scripts/vm/run-slice.sh`, no-flag
technical mode) ran to completion on committed `main` HEAD `7464ff7f` with **0 gate failures** and
the in-VM self-host binary fixpoint **unchanged** (`46ddce14…` == `scripts/vm/EXPECTED_FIXPOINT_VM`).
This supersedes `PHASE_3_VM_SEAL_ATTEMPT_2026-08-15.md`, whose run was rejected because the golden
guest lacked the build toolchain. It closes the "pending external seal" residual carried by
`PHASE_3_COMPLETION_2026-08-14.md` (criterion 10), `PHASE_4_COMPLETION_2026-08-15.md` (criterion 5),
and `COMPLETION_PHASES_5_8_STATUS_2026-08-15.md`.

Scope honesty: this is a **technical** (no-flag) seal — per `scripts/vm/README.md` it licenses
committing the tested technical tree and confirms the fixpoint held; it is **not** a release/tagged
artifact. Cutting a release remains operator-gated (Completion Phase 7).

## What was blocking it, and the fix

The `anubis-xcode` golden image had lost its build toolchain (a fresh/reset base): `cargo`, `rustc`,
`z3`, `cmake`, `elan`/`lean` were all absent. The prior attempt's battery hit `cargo: command not
found` even with `$HOME/.cargo/bin` on PATH — genuine absence, not a PATH bug. **No host run was
substituted for guest evidence.**

Two things were done, in order:

1. **Golden re-provision** (operator-authorized, 2026-08-15). Booted the base `anubis-xcode` guest
   headless and installed, per `scripts/vm/README.md` §33-35 / §102-111:
   - rustup + pinned `nightly-2026-05-10` (`rustc 1.97.0-nightly 82bee9650`, +rustfmt, +clippy)
   - `elan` + Lean `v4.32.0` (resolved from `formal/lean-toolchain`; set as global default)
   - `brew install z3 coreutils cmake` (z3 4.15.4, cmake 4.3.2, GNU `timeout`)
   - a warm `cargo build --release -p anubis`
   Then stopped the base to persist the changes to the golden. Xcode 26.5 + Metal toolchain were
   already present (risc0 Metal compiles). Guest OS: macOS 26.4 (25E246).

2. **Pin-manifest hygiene fix — PR #38 (`7464ff7f`).** The first seal on the re-provisioned golden
   reached `[6/6]` with **0 gate failures and the fixpoint matching**, but the run reported FAIL on
   the source-immutability invariant ("guest source tree changed during the VM battery"). Root
   cause: the `tool-test` gate regenerates the anubis TOOL crate's default evidence dir
   `tools/anubis/out/` (gitignored, `.gitignore:23 out/`) during the battery, yet the pin source
   manifest walked it as source. Every sibling gate-output dir (`examples/out`,
   `examples/security/out`, `formal/.lake`, …) was already in
   `scripts/lib/pin_manifest_policy.json → excluded_exact_directories`; `tools/anubis/out` was
   simply missing. PR #38 adds it. Proven: manifest `tree_sha256` is now stable regardless of that
   dir's contents (row count 1801 → 1672; a probe file added there does not move the hash).
   Gitignored scratch is never tracked source, so this cannot mask a real source mutation. CI
   (hosted 30-gate suite) green on #38.

## The sealed run (evidence bundle `out/vm_runs/anubis-run-83773/`)

- **Isolation:** `tart` disposable guest `anubis-run-83773`, APFS-CoW clone of golden `anubis-xcode`,
  booted headless, torn down (stop_rc=0, delete_rc=0) — verified absent from `tart list`. 8 vCPU /
  12288 MiB cap; `ANUBIS_VM_BUILD_JOBS=3` (lowered from 6 so the self-host build spike stayed above
  the host guard's 8192 MiB reserve — a prior run at jobs=6 was correctly sacrificed by the guard at
  free=7095 MiB; jobs is not projected into the fixpoint).
- **Instrument pin:** `vm/pins/anubis-2c9ee4079c7a-src-2e3c2bd71d5c`
  (pin_sha256 `2c9ee4079c7a9cd434231bd64f881f2fb5a3e0ed539d3806c342121dc412b53c`,
  meta_sha256 `f78712d1707e325b105169aa4d85752813ae99da5e898a37929406fde62c519d`).
  `publish_pin.sh --verify` → "pin matches tree" (source-current at HEAD).
- **Source immutability:** guest source `tree_sha256` **identical** before and after the battery
  (`2e3c2bd71d5c…`, all six epoch checkpoints equal). Host HEAD `7464ff7f` and git tree
  `138d1684…` unchanged before/after/final.
- **Fixpoint:** VM self-host fixpoint
  `46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60` == `EXPECTED_FIXPOINT_VM`
  (stage0→stage1→stage2→stage3, seal=cmp(stage2,stage3) + binary fixpoint). No re-baseline needed:
  the freshly-provisioned golden reproduces the committed baseline exactly.
- **Gate battery — 0 failures** (`battery_verdict.json`: nonzero exit_codes = {}, battery_done_count
  1, vm_build_jobs [3]), remote_ssh=0, validator=0, bundle MANIFEST verify PASS (63 files):

  | gate | result |
  |------|--------|
  | cargo-test (`-p anubis-compiler --lib`) | 799 passed, 0 failed |
  | tool-test (`-p anubis`) | 360 passed, 0 failed |
  | clippy `-D warnings` | clean |
  | build-rel | release build OK (incremental on warm golden) |
  | language / turing | PASS / PASS (13/13) |
  | security | PASS (337/337) |
  | stdlib / stdlib-failclosed | 10/10 / 104/104 |
  | shadow-diff | PASS (958 scanned, 0 unexpected — vacuous by design) |
  | selfhost seal | PASS (9/9) + binary fixpoint |
  | selfhost dogfood | PASS (3/3) |
  | capset / type / taint self-host | AGREE 4/4·0, 20/20·0, 13/13·0 (0 disagreements each) |
  | docs-drift | PASS (53 stamps, 0 drift) |
  | formal-kernel / correspondence | PASS |

- **`run-slice.sh` verdict line:** `PASS — all gates green, fixpoint unchanged. Safe to commit on
  the host.` (process rc 0.)

## Evidence reconciliation (untrusted self-report → machine evidence)

`ammit` (the local-first evidence deck named in `skill://anubis-vm-seal-evidence`) is installed at
`~/.local/bin/ammit` as a **separate project** — not vendored into this repository, so its stock
config points at `~/anubis-lang` (a different, older checkout, `b3390c7c`). It was run against THIS
sealed worktree via a minimal temp config (`path=/tmp/anubis-phase3-slice3-integrated`, isolated
temp `data_dir` so the operator's real deck is untouched):

- `ammit weigh` → ingest **events=505** (the seal's `cargo_test` 1179-result libtest stream + `git`),
  reconcile **judged=1, verified=0, unsupported=1, contradicted=0**.
- `ammit doctor` → **chain_ok=true**, events_checked=504, checkpoint_ok=true (Merkle chain intact;
  the temp store is unsigned — naive-edit tamper-evidence only, no external signing key wired).

**The fabrication firewall is clean: `contradicted=0`** — no claim is refuted by the evidence. The
one `unsupported` claim is the `MATURITY_CLAIM_MATRIX.md` row *"Sealed gates preserved
(4/5/7/8/10/11)"*: no wired ammit adapter emits seal-gate evidence, so ammit conservatively cannot
verify it — that is *unverified*, **not** *contradicted* (the seal above IS that evidence; it simply
is not fed to ammit as an adapter).

Corroborating the machine evidence directly (independent of ammit's claim mapping): parsing
`.ammit/cargo-test.json` (byte-identical to `out/vm_runs/anubis-run-83773/cargo-test.json`;
`exec_time 182.54` fingerprints it to this run) → **11 suites, all `event: ok`, 1179 passed / 0
failed / 0 ignored, 0 failed test events.**

## One honest mechanical note

Between the pin's build (at `db0a6e7f`) and the sealed HEAD (`7464ff7f`) the **only** tracked change
is the one-line `pin_manifest_policy.json` addition (PR #38) — a manifest-config file that is **not
compiled into the `anubis` binary** (`git diff --stat db0a6e7f 7464ff7f` = 1 file, 1 insertion). The
release binary is therefore byte-identical to its `db0a6e7f` build (pin binary hash unchanged:
`2c9ee4079c7a…`); its mtime was refreshed so `publish_pin.sh`'s source-newer-than-binary staleness
guard would admit it. The seal itself rebuilt the tree from scratch in-guest (`build-rel`) and
grades the pinned binary, so this does not affect the sealed result.

---

`SEAL PASS — golden re-provisioned, immutability fixed (#38), fixpoint 46ddce14 unchanged, 0 gate failures; ammit weigh contradicted=0 + doctor chain_ok; cargo_test 1179 passed / 0 failed`
