#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run-slice.sh — validate the current Anubis working tree inside a THROWAWAY
# macOS VM clone, so heavy builds never run on the host.
#
# WHY: an all-core build/seal on the host twice starved WindowServer past its
# ~120 s watchdog check-in → kernel reset → wedged trackpad. A tart guest capped
# at 8 vCPU structurally reserves >=4 P + 4 E cores for the host, so the host UI
# can never starve. See scripts/vm/README.md.
#
# WHAT IT DOES: clone the provisioned golden image (APFS copy-on-write, instant)
# → boot headless → rsync the host working tree in → run the full gate battery
# INCLUDING the self-host seal → assert the seal's binary fixpoint matches
# scripts/vm/EXPECTED_FIXPOINT_VM → tear the clone down. It does NOT commit — a
# commit is a deliberate, human-authored host-side act; this only VALIDATES.
#
# USAGE:  scripts/vm/run-slice.sh [--keep]
#   --keep   leave the clone running for inspection (default: stop + delete)
#
# On PASS it prints the exact `git` command to commit on the host. Exit 0 = every
# gate green AND fixpoint unchanged; non-zero = a gate failed or the fixpoint moved.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

BASE="${ANUBIS_VM_BASE:-anubis-xcode}"      # provisioned golden image to clone from
CPU="${ANUBIS_VM_CPU:-8}"                    # vCPU CEILING — do not raise past 8 (re-arms the watchdog)
MEM="${ANUBIS_VM_MEM:-24576}"                # MiB
REPO="${ANUBIS_REPO:-/Users/sicarii/anubis-lang}"
KEY="${ANUBIS_VM_KEY:-$HOME/.ssh/tart_anubis}"
USER_="admin"
EXPECTED_FILE="$REPO/scripts/vm/EXPECTED_FIXPOINT_VM"
SSHOPTS=(-i "$KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=15)

KEEP=0
for a in "$@"; do case "$a" in --keep) KEEP=1;; *) echo "unknown arg: $a"; exit 2;; esac; done

RUN="anubis-run-$$"
cleanup() {
  if [ "$KEEP" = 1 ]; then echo "[keep] clone left running: $RUN (ip $(tart ip "$RUN" 2>/dev/null || echo '?'))"; return; fi
  echo "[cleanup] stop + delete $RUN"
  tart stop "$RUN" >/dev/null 2>&1 || true
  tart delete "$RUN" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ── preconditions ────────────────────────────────────────────────────────────
command -v tart >/dev/null 2>&1 || { echo "FATAL: tart not installed"; exit 1; }
tart list 2>/dev/null | awk '{print $2}' | grep -qx "$BASE" || { echo "FATAL: golden image '$BASE' not found (tart list)"; exit 1; }
[ -f "$EXPECTED_FILE" ] || { echo "FATAL: missing $EXPECTED_FILE"; exit 1; }
EXPECTED=$(grep -oE '[0-9a-f]{64}' "$EXPECTED_FILE" | head -1)
[ -n "$EXPECTED" ] || { echo "FATAL: no fixpoint hash in $EXPECTED_FILE"; exit 1; }

echo "[1/6] clone $BASE -> $RUN (APFS CoW, instant)"
tart clone "$BASE" "$RUN"
tart set "$RUN" --cpu "$CPU" --memory "$MEM"

echo "[2/6] boot headless + wait for SSH"
tart run "$RUN" --no-graphics >/dev/null 2>&1 &
IP=""
for _ in $(seq 1 75); do
  IP=$(tart ip "$RUN" 2>/dev/null || true)
  if [ -n "$IP" ] && nc -z -w 3 "$IP" 22 2>/dev/null; then break; fi
  sleep 4
done
[ -n "${IP:-}" ] || { echo "FATAL: guest never reached SSH"; exit 1; }
echo "      guest ip=$IP"

echo "[3/6] rsync host working tree -> guest (delta; excl target/ out/)"
RSYNC_RSH="ssh ${SSHOPTS[*]}" rsync -aH --delete \
  --exclude 'target/' --exclude 'out/' --exclude '.DS_Store' \
  "$REPO/" "${USER_}@${IP}:anubis-lang/"

echo "[4/6] run full gate battery in guest (this is the heavy part — in the capped VM)"
ssh "${SSHOPTS[@]}" "${USER_}@${IP}" 'bash -s' <<'REMOTE'
set -u
. "$HOME/.cargo/env" 2>/dev/null || true
# coreutils/libexec/gnubin FIRST so GNU `timeout` resolves — macOS ships none, and
# scripts/run_shadow_diff.sh wraps every check in `timeout`; without it that gate silently
# no-ops (runs zero checks, reports UNEXPECTED=0 vacuously). The golden image has coreutils.
export PATH=/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$PATH
export CARGO_BUILD_JOBS=6 RAYON_NUM_THREADS=6 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864
ulimit -n 65536 2>/dev/null || true
command -v timeout >/dev/null || { echo "FATAL: GNU timeout missing in guest — run: brew install coreutils"; exit 3; }
cd "$HOME/anubis-lang"
LOG="$HOME/battery.log"; : > "$LOG"
run(){ name="$1"; shift; echo "===== $name =====" | tee -a "$LOG"; if "$@" >> "$LOG" 2>&1; then echo "EXIT=0 $name" | tee -a "$LOG"; else echo "EXIT=$? $name" | tee -a "$LOG"; fi; }
# cargo-test emits the libtest JSON stream to `.ammit/cargo-test.json` so Ammit can weigh the
# real test result as machine-produced evidence (agent self-reports are untrusted). stdout →
# the report file, exit code preserved (no pipe), tail echoed to the battery log for the summary.
run cargo-test bash -c 'mkdir -p .ammit; cargo test -p anubis-compiler --lib -- -Z unstable-options --format json 1>.ammit/cargo-test.json 2>.ammit/cargo-test.stderr.log; rc=$?; tail -2 .ammit/cargo-test.json; exit $rc'
# tool-test APPENDS the anubis TOOL crate's stream (unit + the 3 native fail-closed integration
# tests — no risc0/Metal dependency) into the SAME evidence file: Ammit's cargo_test adapter sums
# per-binary suite events, so the proof-runtime matrix rows can cite their REAL test names
# (anbp_blob_roundtrip_header, journal_named_fields_decode, …) instead of staying unverifiable.
run tool-test bash -c 'cargo test -p anubis -- -Z unstable-options --format json 1>>.ammit/cargo-test.json 2>>.ammit/cargo-test.stderr.log; rc=$?; tail -2 .ammit/cargo-test.json; exit $rc'
run clippy     cargo clippy -p anubis-compiler -- -D warnings
# Build the RELEASE `anubis` binary from the rsync'd source BEFORE the fixture gates. Gates that
# resolve `./target/release/anubis` (e.g. run_security_fixtures.sh) would otherwise run the STALE
# release binary baked into the golden image — silently validating old code (a fresh security
# fixture that needs current behaviour would spuriously fail). The `cargo run`-based gates (language)
# were already fresh; this makes the release-binary gates fresh too.
run build-rel  cargo build --release -p anubis
run language   bash scripts/run_language_fixtures.sh
run turing     bash scripts/run_turing_core_fixtures.sh
run security   bash scripts/run_security_fixtures.sh
run stdlib     bash scripts/run_stdlib_gate.sh
run shadow     bash scripts/run_shadow_diff.sh
run seal       bash scripts/run_selfhost_gate.sh
run dogfood    bash scripts/run_selfhost_dogfood_gate.sh
run effect-sh  bash scripts/run_effect_selfhost_gate.sh
run capset-sh  bash scripts/run_capset_selfhost_gate.sh
run type-sh    bash scripts/run_type_selfhost_gate.sh
run taint-sh   bash scripts/run_taint_selfhost_gate.sh
echo "BATTERY_DONE"
REMOTE

echo "[5/6] collect results"
ssh "${SSHOPTS[@]}" "${USER_}@${IP}" \
  'grep -E "^===== |^EXIT=|test result:|\"type\":\"suite\"|Overall:|SELFHOST_GATE:|SELFHOST_DOGFOOD_GATE:|EFFECT_SELFHOST_GATE:|CAPSET_SELFHOST_GATE:|TYPE_SELFHOST_GATE:|stdlib gate:|SHADOW_DIFF:|binary_fixpoint sha256" "$HOME/battery.log"'
VMFP=$(ssh "${SSHOPTS[@]}" "${USER_}@${IP}" 'grep "binary_fixpoint sha256" "$HOME/battery.log" | grep -oE "[0-9a-f]{64}" | head -1' || true)
NFAIL=$(ssh "${SSHOPTS[@]}" "${USER_}@${IP}" 'grep -cE "^EXIT=[1-9]" "$HOME/battery.log" || true')
NFAIL=${NFAIL:-0}

# Collect the Ammit cargo-test evidence (the real VM-produced libtest JSON) back to the host BEFORE
# the clone is torn down — unconditional, since a failing run is itself evidence Ammit should weigh
# (a contradicted test claim). `ammit weigh` on the host then ingests it.
mkdir -p "$REPO/.ammit"
scp "${SSHOPTS[@]}" "${USER_}@${IP}:anubis-lang/.ammit/cargo-test.json" "$REPO/.ammit/cargo-test.json" >/dev/null 2>&1 \
  && echo "      collected .ammit/cargo-test.json (Ammit evidence)" \
  || echo "      (no cargo-test.json to collect)"

echo "[6/6] verdict"
echo "      gate failures : $NFAIL"
echo "      VM fixpoint   : ${VMFP:-<none>}"
echo "      expected      : $EXPECTED"
rc=0
[ "$NFAIL" = 0 ] || { echo "  ✗ $NFAIL gate(s) failed"; rc=1; }
if [ -z "${VMFP:-}" ]; then echo "  ✗ no fixpoint produced (seal did not run/finish)"; rc=1
elif [ "$VMFP" != "$EXPECTED" ]; then
  echo "  ✗ FIXPOINT MOVED — investigate (real defect, or a deliberate re-baseline: update EXPECTED_FIXPOINT_VM)"; rc=1
else echo "  ✓ fixpoint matches baseline"; fi

if [ "$rc" = 0 ]; then
  echo
  echo "PASS — all gates green, fixpoint unchanged. Safe to commit on the host:"
  echo "  cd $REPO && git add <your slice files> && git commit"
else
  echo
  echo "FAIL — do NOT commit. See the battery output above."
fi
exit $rc
