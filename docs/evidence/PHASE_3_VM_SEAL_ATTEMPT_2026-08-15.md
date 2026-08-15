# Phase 3 VM seal attempt — 2026-08-15

Supplementary evidence for `docs/evidence/PHASE_3_COMPLETION_2026-08-14.md`
criterion 10 (VZ seal). Written after Phase 3 receipt PR #33 merged as
`4d249aee`.

**Verdict: seal ATTEMPTED and REJECTED by the battery validator; not a
PASS.** Recorded as-observed per the admission rule
`docs/COMPLETION_BLUEPRINT.md:88`: "unavailable or external lanes are
reported exactly as skipped/external, never inherited as fresh PASS."

## What ran

```text
Command:   /bin/bash scripts/vm/run-slice.sh
Tree:      /tmp/anubis-phase3-slice3-integrated  (dirty 0 modulo vm/pins)
HEAD:      4d249aee69730e9d97b518d3a9293444cc604618  (origin/main)
Env:       ANUBIS_VM_CPU=2 ANUBIS_VM_MEM=4096 ANUBIS_VM_BUILD_JOBS=1
Pin:       vm/pins/anubis-f3acdeef6bba-src-c2b55d014417
Pin sha:   f3acdeef6bba4ede62595a5939bc0516c724e0d8944dfbd562726584802127b1
Pin verify:pin matches tree (`bash scripts/publish_pin.sh --verify`)
Guest:     anubis-run-5152 (APFS CoW clone of anubis-xcode 140 GiB base)
Bundle:    out/vm_runs/anubis-run-5152/  (63 files, MANIFEST verified)
Bundle root: /private/tmp/anubis-phase3-slice3-integrated/out/vm_runs/anubis-run-5152
```

Host admission:

```text
ANUBIS_HOST_GUARD_PREFLIGHT: PASS cpu=2 mem=4096MiB free=16795MiB required=12288MiB pressure=1
ANUBIS_HOST_GUARD_TEARDOWN:  PASS guest=anubis-run-5152 stop_rc=0 delete_rc=0
BUNDLE_MANIFEST_VERIFY_PASS files=62 path=.../MANIFEST.sha256
```

## Battery outcome

From `out/vm_runs/anubis-run-5152/battery.protocol`
(SHA-256 `6861f1043c2ca389…`):

| Gate | rc | Classification |
|---|---|---|
| `pin-smoke` | 0 | **PASS** |
| `cargo-test` | 127 | **ENV MISSING** — `cargo: command not found` in guest |
| `tool-test` | 127 | ENV MISSING (same) |
| `clippy` | 127 | ENV MISSING (same) |
| `build-rel` | 127 | ENV MISSING (same) |
| `language` | 1 | downstream — needs `anubis run` → cargo |
| `turing` | 1 | downstream (same) |
| `security` | 0 | **PASS** (329/329) |
| `stdlib` | 1 | downstream (same) |
| `shadow` | 0 | **PASS** |
| `seal` | 125 | seal validator refused (fixpoint invalid without toolchain) |
| `dogfood` | 1 | downstream |
| `effect-sh` / `capset-sh` / `type-sh` / `taint-sh` | 1 | downstream |
| `stdlib-fc` | 1 | downstream |
| `native-auth` | 1 | downstream — needs `cargo` for the LRAT cert suite |
| `docs-drift` | 0 | **PASS** (50 stamps, 0 drift) |
| `walker` | 0 | **PASS** (walker completeness) |
| `formal` | 127 | ENV MISSING — `lean: command not found` |
| `formal-kernel` | 1 | downstream — needs cargo to compile the kernel program |
| `correspondence` | 0 | **PASS** (11 citations resolve; TCB enumerated) |

**Net: 6 PASS / 5 ENV-MISSING / 11 downstream FAIL / 1 seal-refused.** Not a
PASS. Recorded here, not inherited.

## Root cause (environmental, not repo)

The `anubis-xcode` base image at the current host does NOT have `cargo`
(via `rustup`) or `lean` (via `elan`) on the guest `admin` user's PATH. The
five ENV-MISSING gates are the direct hits; the eleven downstream FAILs need
either the runtime `anubis run` (which itself invokes `cargo` to compile the
transpiled Rust) or a fresh workspace build via `cargo`.

Evidence:

- `battery.log:3` — `bash: line 63: cargo: command not found`
- `battery.log:86-89` — `scripts/run_native_authoritative_gate.sh: line 34:
  cargo: command not found`
- `battery.log:1168` — `scripts/run_formal_gate.sh: line 10: lean: command
  not found`
- `battery.log:1177` — `Error: cargo spawn failed: No such file or directory
  (os error 2)` from `anubis run`

The gates whose only dependency is the pinned `anubis check` binary (and the
pinned repo tree) all PASSED — proving the guest, the pin transfer, and
the check-lane instrument itself are healthy. The gates that need a Rust
toolchain to compile a target program (`anubis run`, `cargo test`, `cargo
clippy`, `lean`) hit the missing-tool wall uniformly.

## Not attempted / not fabricated

- No host substitution was made. Every gate result is a guest exit code.
- No `--admin` merge, weakening of the validator, or manual gate rescoring
  was attempted.
- The Phase 3 completion report at `docs/evidence/PHASE_3_COMPLETION_2026-08-14.md`
  criterion 10 remains **EXTERNAL / SKIPPED** — this attempt does not flip
  it to PASS.

## To promote criterion 10 to PASS

An operator with base-image access needs to:

1. boot the `anubis-xcode` base and install (as `admin`):
   - `rustup` + the workspace's pinned nightly (via `curl -sSf
     https://sh.rustup.rs | sh`);
   - `elan` + Lean v4.32.0 (via `curl -sSf
     https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh
     | sh`);
2. save the image;
3. rerun `bash scripts/vm/run-slice.sh` — the guest starts from the fixed
   base + rsync of this tree, so the toolchains will then be present for
   the compile-based gates.

Alternatively, the operator can accept criterion 10 as an explicit
EXTERNAL-until-provisioning residual and open Completion Phase 4 with that
residual named in `docs/CLAIMS.md`.

## Bundle files

63 files under
`/private/tmp/anubis-phase3-slice3-integrated/out/vm_runs/anubis-run-5152/`.
The MANIFEST was validated by `BUNDLE_MANIFEST_VERIFY_PASS`. Notable:

- `battery.log`         — 63,191 bytes, sha256 `6861f1043c2ca389…`
- `battery.protocol`    — 1,863 bytes, sha256 `c48…` (per protocol footer)
- `battery_verdict.json`
- `guest_pin_identity_before_battery.txt` / `_after_battery.txt` — both
  match `f3acdeef6bba…`, proving the pin was not swapped mid-run
- `guest_source_manifest_before_battery.json` / `_after_battery.json` —
  the tree hash was recorded before and after; no in-guest modification
