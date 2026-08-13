# VM build isolation — heavy Anubis builds run in a throwaway macOS guest

Heavy Anubis work (workspace build, `cargo test`, the fixture gates, and the
multi-stage **self-host seal**) is sustained CPU and memory load. It has starved
macOS's `WindowServer` past its watchdog check-in and has also collided with
sleep/power-transition deadlines; dirty resets can leave the internal trackpad
wedged. Heavy work therefore runs in a capped macOS guest, with a host-side
circuit breaker that sacrifices the VM/test run before it sacrifices the host.

## Why these exact numbers (M4 Max, 12 P + 4 E, 48 GiB)

- **8 vCPU hard ceiling** — on Apple Silicon each vCPU is a high-QoS host thread.
  The vCPU count is the reliable scheduling lever. Never raise past 8.
- **12 GiB RAM hard ceiling** — 24 GiB was observed at ~21 GiB guest RSS while the
  host had only 755 MiB free; WindowServer then missed its check-in. The cap is
  validated before clone creation, not merely supplied as an overridable default.
- **Guest allocation + 8 GiB admission reserve / 8 GiB runtime breaker** — a VM
  is refused unless immediately free RAM can cover its full configured allocation
  and still leave 8 GiB for the host (for example, a 12 GiB guest requires 20 GiB
  free). A persistent user LaunchAgent checks every five seconds and stops all
  running Tart VMs if immediately free memory falls below 8 GiB or macOS reports
  elevated memory pressure. Named VMs are never auto-deleted; only ownerless
  generated clones are reaped.
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

Guest sync preserves the golden image's warm `target/` cache, then overwrites the selected CLI and
rebuilds against checksum-compared source; transferred files get current mtimes so Cargo cannot
mistake changed source for an older cached output. It explicitly removes host-only `out/`, agent-worktree,
adversary, export, and scratch trees instead of using recursive `--delete-excluded` over the
48-GiB cache. `vm/pins/` copies only `CURRENT`, that selected immutable binary, and its metadata;
archived guest pin binaries remain untouched and cannot become current.

Every host-side gate also runs `caffeinate -dimsu -w <gate-pid>` so macOS cannot
enter idle sleep/standby while the disposable guest is active. The persistent
guard is installed as `~/Library/LaunchAgents/com.anubis.host-resource-guard.plist`
and logs actions (not healthy polling noise) to
`~/Library/Logs/anubis-host-resource-guard.log`. Verify it with:

```sh
bash scripts/test_host_resource_guard.sh
bash scripts/lib/host_resource_guard.sh once
launchctl print "gui/$(id -u)/com.anubis.host-resource-guard"
```

## Fixpoint parity

`EXPECTED_FIXPOINT_VM` (currently `a01a1e8b…`; re-baselined on each `anubis_sh.anb`
slice — see the log inside the file) is the in-VM self-host fixpoint. It differs
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
→ `tart set anubis-xcode --cpu 8 --memory 12288 --disk-size 150` → boot headless → install SSH key
→ `rustup` pinned `nightly-2026-05-10` (fetch-to-file then run; a `curl | sh` pipe is
blocked by the host guard) → `brew install z3 coreutils` (**coreutils is required** — macOS
ships no GNU `timeout`, which `run_shadow_diff.sh` wraps every check in; without it that gate
silently runs zero checks and reports `UNEXPECTED=0` vacuously) → put
`/opt/homebrew/opt/coreutils/libexec/gnubin` first on PATH → rsync the repo → warm `cargo
build --release -p anubis` → re-run the seal to re-establish `EXPECTED_FIXPOINT_VM`.
