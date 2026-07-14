# VM build isolation — heavy Anubis builds run in a throwaway macOS guest

Heavy Anubis work (workspace build, `cargo test`, the fixture gates, and the
multi-stage **self-host seal**) is all-core and sustained. Run on the host, it
twice starved macOS's `WindowServer` past its ~120 s userspace-watchdog check-in,
which the kernel answers with a deliberate panic + reset — and the reset left the
internal trackpad driver wedged. The fix is **not** "build gently"; it is to run
every heavy build inside a macOS guest VM whose **vCPU count is a hard ceiling the
host scheduler always sits above**.

## Why these exact numbers (M4 Max, 12 P + 4 E, 48 GiB)

- **8 vCPU** — on Apple Silicon each vCPU is a high-QoS host thread that lands on a
  P-core and **cannot be demoted** (`nice`/`taskpolicy`/QoS on the VMM do nothing).
  The vCPU *count* is the only lever. 8 structurally reserves **≥4 P + 4 E** physical
  cores the guest can never claim → `WindowServer` always checks in → watchdog
  disarmed. Never raise past 8; treat `--cpu >8` as re-arming the crash.
- **24 GiB RAM** — clears risc0's ~16 GB build/prove floor with headroom, leaves 24
  for the host.
- **`CARGO_BUILD_JOBS=6`** (a second, memory belt), `RAYON_NUM_THREADS=6`,
  `CARGO_INCREMENTAL=0`, `RUST_MIN_STACK=64M`, `ulimit -n 65536`. The risc0 C worker
  threads get their 64 MB stacks from the vendored `risc0-sys` patch (the env var
  can't reach pthread-created workers) — that patch must stay in the tree.

## Tooling

- **tart** (`brew install cirruslabs/cli/tart`) — CLI-first, clones are instant APFS
  copy-on-write. Chosen over VirtualBuddy (GUI-only) for scripted throwaway runs.
- Golden image: **`anubis-xcode`** = `ghcr.io/cirruslabs/macos-tahoe-xcode` provisioned
  with the pinned Rust nightly (`nightly-2026-05-10`, == host `rustc 82bee9650`), z3,
  cmake, the rsync'd repo (incl. vendored/patched risc0), and a warm `target/`.
  The Xcode image (not `-base`) is required: building the `anubis` binary compiles
  risc0's **Metal** kernels, which need the metal toolchain that ships with Xcode.

## Usage

```sh
# validate the current working tree end-to-end in a throwaway clone:
scripts/vm/run-slice.sh
# leave the clone up to poke at it:
scripts/vm/run-slice.sh --keep
```

`run-slice.sh` clones the golden → boots headless → rsyncs the host tree in → runs
the whole battery (test, clippy, language/turing/security/stdlib, shadow diff,
**self-host seal**, dogfood) → asserts the seal's binary fixpoint equals
`EXPECTED_FIXPOINT_VM` → deletes the clone. Exit 0 = all green + fixpoint unchanged.

It does **not** commit. On PASS it prints the `git` command; you commit on the host
deliberately (a commit is a human-authored act, and `git commit` is not a heavy
build, so it is safe on the host).

## Fixpoint parity

`EXPECTED_FIXPOINT_VM` (`dc680001…`) is the in-VM self-host fixpoint. It differs
from the **host** fixpoint (`c640badd…`, macOS 26.5.2) because the guest is on a
different macOS point-build — a Mach-O-normalization byte difference, not a
correctness change. The seal's real invariant is `stage2 == stage3` AND the hash is
stable across every slice in the same golden image. Re-baking the golden (OS update,
toolchain bump, risc0 change) means re-baselining `EXPECTED_FIXPOINT_VM`
**deliberately** — a logged change, never a silent drift.

## What still runs on the bare host

No Metal/GPU is passed to the guest, so the **risc0/Metal attested prove+verify path
(gate G10)** and the hermetic-Docker external repro gate stay on the host. Those are
GPU-bound / short and never caused the reset; only the all-core CPU build/seal —
which the VM now contains — did.

## Re-provisioning the golden from scratch

If `anubis-xcode` is lost, rebuild it: `tart clone ghcr.io/cirruslabs/macos-tahoe-xcode:latest anubis-xcode`
→ `tart set --cpu 8 --memory 24576 --disk-size 150` → boot headless → install SSH key
→ `rustup` pinned `nightly-2026-05-10` (fetch-to-file then run; a `curl | sh` pipe is
blocked by the host guard) → `brew install z3 coreutils` (**coreutils is required** — macOS
ships no GNU `timeout`, which `run_shadow_diff.sh` wraps every check in; without it that gate
silently runs zero checks and reports `UNEXPECTED=0` vacuously) → put
`/opt/homebrew/opt/coreutils/libexec/gnubin` first on PATH → rsync the repo → warm `cargo
build --release -p anubis` → re-run the seal to re-establish `EXPECTED_FIXPOINT_VM`.
