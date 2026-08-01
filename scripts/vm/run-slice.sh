#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run-slice.sh — validate the current Anubis working tree inside a THROWAWAY
# macOS VM clone, so heavy builds never run on the host.
#
# WHY: all-core build/seal runs starved WindowServer past its watchdog check-in
# → reset → wedged trackpad. A Tart guest capped at 8 vCPU closes the measured
# CPU axis; the 12 GiB RAM ceiling plus the live host guard below closes the
# measured memory axis and aborts the VM before host headroom is exhausted.
#
# WHAT IT DOES: clone the provisioned golden image (APFS copy-on-write, instant)
# → boot headless → rsync the host working tree in → run the full gate battery
# INCLUDING the self-host seal → assert the seal's binary fixpoint matches
# scripts/vm/EXPECTED_FIXPOINT_VM → tear the clone down. It does NOT commit — a
# commit is a deliberate, human-authored host-side act; this only VALIDATES.
#
# USAGE:  scripts/vm/run-slice.sh [--keep] [--release]
#   --keep   leave the clone running for inspection (default: stop + delete)
#   --release require a fresh clean commit-bound release pin rather than a technical pin
#
# On PASS it prints the exact `git` command to commit on the host. Exit 0 = every
# gate green AND fixpoint unchanged; non-zero = a gate failed or the fixpoint moved.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

BASE="${ANUBIS_VM_BASE:-anubis-xcode}"      # provisioned golden image to clone from
CPU="${ANUBIS_VM_CPU:-8}"                    # vCPU CEILING — do not raise past 8 (re-arms the watchdog)
# RAM CEILING — do not raise past 12288. The vCPU cap above stops CPU starvation but
# said nothing about memory, and that hole caused three unclean restarts (2026-07-24
# panic, 07-26 panic, 07-28 WindowServer watchdog kill + power-button reset). At the
# old 24576 the guest reached ~21 GiB RSS while the host carried ~880 processes, and
# host free RAM measured 755 MB — WindowServer then missed its watchdog check-in
# (40 s) and any sleep/power transition blew its 35 s callback deadline → panic.
# Free RAM jumped straight back to 22 GiB the instant that guest exited. 12288 keeps
# >=36 GiB for the host, and 12 GiB / CARGO_BUILD_JOBS=6 below is ~2 GiB per rustc.
MEM="${ANUBIS_VM_MEM:-12288}"                # MiB
BUILD_JOBS="${ANUBIS_VM_BUILD_JOBS:-6}"       # keep <=6; lower for smaller guests
SCRIPT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
REPO="${ANUBIS_REPO:-$SCRIPT_REPO}"
KEY="${ANUBIS_VM_KEY:-$HOME/.ssh/tart_anubis}"
USER_="admin"
EXPECTED_FILE="$REPO/scripts/vm/EXPECTED_FIXPOINT_VM"
SSHOPTS=(-i "$KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=15)
# shellcheck source=../lib/host_resource_guard.sh
source "$REPO/scripts/lib/host_resource_guard.sh"

KEEP=0
RELEASE_MODE=0
for a in "$@"; do
  case "$a" in
    --keep) KEEP=1 ;;
    --release) RELEASE_MODE=1 ;;
    *) echo "unknown arg: $a"; exit 2 ;;
  esac
done

RUN="anubis-run-$$"
HOST_EVIDENCE_DIR="${ANUBIS_VM_EVIDENCE_DIR:-$REPO/out/vm_runs/$RUN}"
if [[ -e "$HOST_EVIDENCE_DIR" || -L "$HOST_EVIDENCE_DIR" ]]; then
  echo "FATAL: VM evidence directory already exists: $HOST_EVIDENCE_DIR" >&2
  exit 2
fi
mkdir -p "$HOST_EVIDENCE_DIR"
TART_RUN_PID=""
GUEST_MANIFEST_TOOL=".anubis-pin-manifest-${RUN}.py"

write_source_manifest() {
  local out_json="$1"
  python3 "$REPO/scripts/lib/pin_manifest.py" --root "$REPO" \
    --policy scripts/lib/pin_manifest_policy.json \
    --field json > "$out_json"
}

source_tree_sha256() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["tree_sha256"])' "$1"
}

capture_guest_source_manifest() {
  local label="$1"
  local out_json="$HOST_EVIDENCE_DIR/guest_source_manifest_${label}.json"
  local err="$HOST_EVIDENCE_DIR/guest_source_manifest_${label}.stderr"
  local capture_rc
  if ssh "${SSHOPTS[@]}" "${USER_}@${IP}" \
    "python3 \"\$HOME/$GUEST_MANIFEST_TOOL\" --root \"\$HOME/anubis-lang\" --policy scripts/lib/pin_manifest_policy.json --field json" \
    >"$out_json" 2>"$err"; then
    capture_rc=0
  else
    capture_rc=$?
  fi
  printf '%s\n' "$capture_rc" > "$HOST_EVIDENCE_DIR/guest_source_manifest_${label}_exit_code.txt"
  if [[ $capture_rc -ne 0 || ! -s "$out_json" || -L "$out_json" ]]; then
    echo "FATAL: guest source manifest capture failed at $label (rc=$capture_rc)" >&2
    return 1
  fi
}

capture_guest_pin_identity() {
  local label="$1"
  local out="$HOST_EVIDENCE_DIR/guest_pin_identity_${label}.txt"
  local err="$HOST_EVIDENCE_DIR/guest_pin_identity_${label}.stderr"
  local capture_rc
  if ssh "${SSHOPTS[@]}" "${USER_}@${IP}" \
    "set -eu; cd \"\$HOME/anubis-lang\"; expected='$CURRENT_PIN'; test -f vm/pins/CURRENT && test ! -L vm/pins/CURRENT; actual=\$(cat vm/pins/CURRENT); test \"\$actual\" = \"\$expected\"; test -f \"\$expected\" && test ! -L \"\$expected\" && test -x \"\$expected\" && test ! -w \"\$expected\"; test -f \"\$expected.meta\" && test ! -L \"\$expected.meta\" && test ! -w \"\$expected.meta\"; printf 'pin=%s\\npin_sha256=%s\\nmeta_sha256=%s\\n' \"\$expected\" \"\$(shasum -a 256 \"\$expected\" | awk '{print \$1}')\" \"\$(shasum -a 256 \"\$expected.meta\" | awk '{print \$1}')\"" \
    >"$out" 2>"$err"; then
    capture_rc=0
  else
    capture_rc=$?
  fi
  printf '%s\n' "$capture_rc" > "$HOST_EVIDENCE_DIR/guest_pin_identity_${label}_exit_code.txt"
  if [[ $capture_rc -ne 0 || ! -s "$out" || -L "$out" ]]; then
    echo "FATAL: guest pin identity capture failed at $label (rc=$capture_rc)" >&2
    return 1
  fi
}

cleanup() {
  anubis_guard_stop_runtime_watch
  if [ "$KEEP" = 1 ]; then
    anubis_guard_mark_kept "$RUN"
    printf 'retained\n' > "$HOST_EVIDENCE_DIR/teardown_status.txt"
    echo "[keep] clone left running: $RUN (ip $(tart ip "$RUN" 2>/dev/null || echo '?'))"
    return
  fi
  anubis_guard_clear_kept "$RUN"
  echo "[cleanup] stop + delete $RUN"
  if ! anubis_guard_teardown_guest "$RUN"; then
    printf 'failed\n' > "$HOST_EVIDENCE_DIR/teardown_status.txt"
    echo "FATAL: teardown failed for $RUN" >&2
    return 1
  fi
  if [[ -n "$TART_RUN_PID" ]]; then
    set +e
    wait "$TART_RUN_PID" > "$HOST_EVIDENCE_DIR/tart_run_wait.stdout" 2> "$HOST_EVIDENCE_DIR/tart_run_wait.stderr"
    tart_wait_rc=$?
    set -e
    printf '%s\n' "$tart_wait_rc" > "$HOST_EVIDENCE_DIR/tart_run_wait_rc.txt"
  fi
  printf 'torn_down\n' > "$HOST_EVIDENCE_DIR/teardown_status.txt"
  tart list --format json > "$HOST_EVIDENCE_DIR/tart_after_teardown.json" 2> "$HOST_EVIDENCE_DIR/tart_after_teardown.stderr" || return 1
  echo "[cleanup] verified absent: $RUN"
}
trap cleanup EXIT

# ── preconditions ────────────────────────────────────────────────────────────
command -v tart >/dev/null 2>&1 || { echo "FATAL: tart not installed"; exit 1; }
tart list 2>/dev/null | awk '{print $2}' | grep -qx "$BASE" || { echo "FATAL: golden image '$BASE' not found (tart list)"; exit 1; }
[[ -f "$EXPECTED_FILE" && ! -L "$EXPECTED_FILE" ]] || { echo "FATAL: missing/non-regular $EXPECTED_FILE"; exit 1; }
EXPECTED_HASH_LINES="$(grep -E '^[0-9a-f]{64}$' "$EXPECTED_FILE" || true)"
EXPECTED_LINES="$(printf '%s\n' "$EXPECTED_HASH_LINES" | sed '/^$/d' | wc -l | tr -d '[:space:]')"
EXPECTED="$(printf '%s\n' "$EXPECTED_HASH_LINES" | sed '/^$/d' | head -1)"
[[ "$EXPECTED_LINES" == "1" && "$EXPECTED" =~ ^[0-9a-f]{64}$ ]] \
  || { echo "FATAL: $EXPECTED_FILE must contain exactly one uncommented 64-hex line"; exit 1; }
[[ "$BUILD_JOBS" =~ ^[1-6]$ ]] || { echo "FATAL: ANUBIS_VM_BUILD_JOBS must be an integer from 1 to 6"; exit 2; }
mkdir -p "$REPO/.ammit"
rm -f "$REPO/.ammit/cargo-test.json" "$REPO/.ammit/cargo-test.stderr.log"
HOST_HEAD_BEFORE="$(git -C "$REPO" rev-parse --verify 'HEAD^{commit}')"
HOST_GIT_TREE_BEFORE="$(git -C "$REPO" rev-parse --verify 'HEAD^{tree}')"
printf 'head=%s\ngit_tree=%s\nrelease_mode=%s\n' \
  "$HOST_HEAD_BEFORE" "$HOST_GIT_TREE_BEFORE" "$RELEASE_MODE" \
  > "$HOST_EVIDENCE_DIR/git_epoch.txt"
git -C "$REPO" status --porcelain=v1 --untracked-files=all \
  > "$HOST_EVIDENCE_DIR/git_status_before.txt"
PIN_VERIFY_FLAG="--verify"
[[ "$RELEASE_MODE" == "1" ]] && PIN_VERIFY_FLAG="--verify-release"
set +e
(cd "$REPO" && bash scripts/publish_pin.sh "$PIN_VERIFY_FLAG") \
  > "$HOST_EVIDENCE_DIR/pin_verify_before.stdout" \
  2> "$HOST_EVIDENCE_DIR/pin_verify_before.stderr"
PIN_VERIFY_BEFORE_RC=$?
set -e
printf '%s\n' "$PIN_VERIFY_BEFORE_RC" > "$HOST_EVIDENCE_DIR/pin_verify_before_exit_code.txt"
if [[ $PIN_VERIFY_BEFORE_RC -ne 0 ]]; then
  echo "FATAL: opening pin verification failed in $PIN_VERIFY_FLAG mode" >&2
  exit 1
fi
CURRENT_PIN="$(cd "$REPO" && bash scripts/publish_pin.sh --current)"
CURRENT_PIN_SHA256="$(shasum -a 256 "$REPO/$CURRENT_PIN" | awk '{print $1}')"
CURRENT_PIN_META_SHA256="$(shasum -a 256 "$REPO/$CURRENT_PIN.meta" | awk '{print $1}')"
printf 'pin=%s\npin_sha256=%s\nmeta_sha256=%s\n' \
  "$CURRENT_PIN" "$CURRENT_PIN_SHA256" "$CURRENT_PIN_META_SHA256" \
  > "$HOST_EVIDENCE_DIR/host_pin_identity.txt"
write_source_manifest "$HOST_EVIDENCE_DIR/host_source_manifest_before.json"
HOST_SOURCE_TREE_BEFORE="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/host_source_manifest_before.json")"
printf 'source_tree_sha256_before=%s\n' "$HOST_SOURCE_TREE_BEFORE" > "$HOST_EVIDENCE_DIR/source_epoch.txt"
anubis_guard_preflight "$CPU" "$MEM" || exit $?
anubis_guard_require_launchagent || exit $?
anubis_guard_start_caffeinate $$
anubis_guard_start_runtime_watch $$ || exit $?

echo "[1/6] clone $BASE -> $RUN (APFS CoW, instant)"
tart clone "$BASE" "$RUN"
tart set "$RUN" --cpu "$CPU" --memory "$MEM"

echo "[2/6] boot headless + wait for SSH"
tart run "$RUN" --no-graphics >/dev/null 2>&1 &
TART_RUN_PID=$!
printf '%s\n' "$TART_RUN_PID" > "$HOST_EVIDENCE_DIR/tart_run_pid.txt"
IP=""
for _ in $(seq 1 75); do
  IP=$(tart ip "$RUN" 2>/dev/null || true)
  if [ -n "$IP" ] && nc -z -w 3 "$IP" 22 2>/dev/null; then break; fi
  sleep 4
done
[ -n "${IP:-}" ] || { echo "FATAL: guest never reached SSH"; exit 1; }
echo "      guest ip=$IP"
anubis_guard_stop_runtime_watch
anubis_guard_start_runtime_watch $$ "$RUN" || exit $?

echo "[3/6] rsync live source -> guest (exclude host-local build/agent/VM artifacts)"
write_source_manifest "$HOST_EVIDENCE_DIR/host_source_manifest_at_sync.json"
HOST_SOURCE_TREE_SYNC="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/host_source_manifest_at_sync.json")"
printf 'source_tree_sha256_at_sync=%s\n' "$HOST_SOURCE_TREE_SYNC" >> "$HOST_EVIDENCE_DIR/source_epoch.txt"
if [[ "$HOST_SOURCE_TREE_SYNC" != "$HOST_SOURCE_TREE_BEFORE" ]]; then
  echo "FATAL: source tree changed before VM sync (before=$HOST_SOURCE_TREE_BEFORE sync=$HOST_SOURCE_TREE_SYNC)" >&2
  exit 1
fi
anubis_guard_sync_tree \
  "ssh ${SSHOPTS[*]}" \
  "$REPO/" \
  "${USER_}@${IP}:anubis-lang/"

# The manifest implementation itself is copied outside the synchronized tree. A gate may mutate a
# source file (including scripts/lib/pin_manifest.py); using that potentially mutated copy to grade
# itself would let the mutation define its own evidence. The policy remains inside the bound tree,
# so any policy mutation changes the manifest and is caught by the exact comparison below.
set +e
scp "${SSHOPTS[@]}" "$REPO/scripts/lib/pin_manifest.py" \
  "${USER_}@${IP}:$GUEST_MANIFEST_TOOL" \
  >"$HOST_EVIDENCE_DIR/guest_manifest_tool.scp.stdout" \
  2>"$HOST_EVIDENCE_DIR/guest_manifest_tool.scp.stderr"
GUEST_MANIFEST_TOOL_SCP_RC=$?
set -e
printf '%s\n' "$GUEST_MANIFEST_TOOL_SCP_RC" > "$HOST_EVIDENCE_DIR/guest_manifest_tool_scp_exit_code.txt"
if [[ $GUEST_MANIFEST_TOOL_SCP_RC -ne 0 ]]; then
  echo "FATAL: trusted guest manifest tool transport failed (rc=$GUEST_MANIFEST_TOOL_SCP_RC)" >&2
  exit 1
fi
if ! capture_guest_source_manifest before_battery; then
  exit 1
fi
if ! capture_guest_pin_identity before_battery; then
  exit 1
fi
if ! cmp -s "$HOST_EVIDENCE_DIR/host_pin_identity.txt" \
  "$HOST_EVIDENCE_DIR/guest_pin_identity_before_battery.txt"; then
  echo "FATAL: guest pin bytes/metadata do not match the host-selected immutable pin" >&2
  exit 1
fi
if ! cmp -s "$HOST_EVIDENCE_DIR/host_source_manifest_at_sync.json" \
  "$HOST_EVIDENCE_DIR/guest_source_manifest_before_battery.json"; then
  echo "FATAL: guest source epoch does not exactly match the host sync epoch before the battery" >&2
  exit 1
fi
GUEST_SOURCE_TREE_BEFORE="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/guest_source_manifest_before_battery.json")"
printf 'guest_source_tree_sha256_before_battery=%s\n' "$GUEST_SOURCE_TREE_BEFORE" \
  >> "$HOST_EVIDENCE_DIR/source_epoch.txt"

echo "[4/6] run full gate battery in guest (this is the heavy part — in the capped VM)"
PROTOCOL_LOG="$HOST_EVIDENCE_DIR/battery.protocol"
if [[ -e "$PROTOCOL_LOG" || -L "$PROTOCOL_LOG" ]]; then
  echo "FATAL: host protocol capture path already exists: $PROTOCOL_LOG" >&2
  exit 2
fi
set +e
ssh "${SSHOPTS[@]}" "${USER_}@${IP}" \
  "ANUBIS_VM_BUILD_JOBS=$BUILD_JOBS ANUBIS_VM_PIN=$CURRENT_PIN ANUBIS_VM_PIN_SHA256=$CURRENT_PIN_SHA256 ANUBIS_VM_PIN_META_SHA256=$CURRENT_PIN_META_SHA256 bash -s" \
  >"$PROTOCOL_LOG" 2>"$HOST_EVIDENCE_DIR/remote_battery.stderr" <<'REMOTE'
set -u
. "$HOME/.cargo/env" 2>/dev/null || true
# Put Apple's native tools ahead of Homebrew's GNU aliases deterministically, while keeping the
# one required GNU command (`timeout`) ahead of any inherited PATH entry. Putting gnubin first
# replaces `/usr/bin/stat`; preserving inherited order can make the same bug environment-dependent.
export PATH=/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH
export CARGO_BUILD_JOBS="$ANUBIS_VM_BUILD_JOBS" RAYON_NUM_THREADS="$ANUBIS_VM_BUILD_JOBS" \
  CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864
# Say WHERE we are rather than making every gate infer it. There is no nested virtualization here,
# so gates whose lane needs a disposable guest must SKIP with a reason and score against a guest
# floor — inferring that from `command -v tart` works today only because the image happens not to
# carry tart, which is a coincidence, not a statement.
export ANUBIS_IN_VM_GUEST=1
ulimit -n 65536 2>/dev/null || true
STAT_BIN="$(command -v stat 2>/dev/null || true)"
TIMEOUT_BIN="$(command -v timeout 2>/dev/null || true)"
[ "$STAT_BIN" = /usr/bin/stat ] \
  || { echo "FATAL: native stat shadowed in guest: ${STAT_BIN:-missing}"; exit 3; }
[ "$TIMEOUT_BIN" = /opt/homebrew/opt/coreutils/libexec/gnubin/timeout ] \
  || { echo "FATAL: GNU timeout missing/shadowed in guest: ${TIMEOUT_BIN:-missing}"; exit 3; }
"$TIMEOUT_BIN" --version 2>/dev/null | grep -q 'GNU coreutils' \
  || { echo "FATAL: timeout is not GNU coreutils"; exit 3; }
cd "$HOME/anubis-lang"
# Bind every binary-aware guest gate to the immutable pin selected and verified by the host. The
# release build below still validates that the synchronized source compiles, but it is not allowed
# to silently replace the compiler whose bytes this receipt names.
[[ "$ANUBIS_VM_PIN" =~ ^vm/pins/anubis-[0-9a-f]{12}(-src-[0-9a-f]{12}(-release)?)?$ ]] \
  || { echo "FATAL: malformed selected pin path"; exit 3; }
[[ "$ANUBIS_VM_PIN_SHA256" =~ ^[0-9a-f]{64}$ && "$ANUBIS_VM_PIN_META_SHA256" =~ ^[0-9a-f]{64}$ ]] \
  || { echo "FATAL: malformed selected pin digest"; exit 3; }
[[ -f vm/pins/CURRENT && ! -L vm/pins/CURRENT ]] \
  || { echo "FATAL: guest CURRENT is not a regular file"; exit 3; }
GUEST_CURRENT_PIN="$(bash scripts/publish_pin.sh --current)" \
  || { echo "FATAL: guest CURRENT resolution failed"; exit 3; }
[[ "$GUEST_CURRENT_PIN" == "$ANUBIS_VM_PIN" ]] \
  || { echo "FATAL: guest CURRENT changed before the battery"; exit 3; }
[[ -f "$ANUBIS_VM_PIN" && ! -L "$ANUBIS_VM_PIN" && -x "$ANUBIS_VM_PIN" && ! -w "$ANUBIS_VM_PIN" ]] \
  || { echo "FATAL: selected guest pin is not immutable/executable"; exit 3; }
[[ -f "$ANUBIS_VM_PIN.meta" && ! -L "$ANUBIS_VM_PIN.meta" && ! -w "$ANUBIS_VM_PIN.meta" ]] \
  || { echo "FATAL: selected guest pin metadata is not immutable"; exit 3; }
GUEST_PIN_SHA256="$(shasum -a 256 "$ANUBIS_VM_PIN" | awk '{print $1}')" \
  || { echo "FATAL: selected guest pin hash failed"; exit 3; }
GUEST_PIN_META_SHA256="$(shasum -a 256 "$ANUBIS_VM_PIN.meta" | awk '{print $1}')" \
  || { echo "FATAL: selected guest pin metadata hash failed"; exit 3; }
[[ "$GUEST_PIN_SHA256" == "$ANUBIS_VM_PIN_SHA256" \
   && "$GUEST_PIN_META_SHA256" == "$ANUBIS_VM_PIN_META_SHA256" ]] \
  || { echo "FATAL: selected guest pin identity mismatch"; exit 3; }
export ANUBIS_BIN="$HOME/anubis-lang/$ANUBIS_VM_PIN"
LOG="$HOME/battery.log"; : > "$LOG" || exit 125
PROTOCOL_TMP="$(mktemp "$HOME/.battery.protocol.XXXXXX")" || exit 125
GATE_LOG_DIR="$HOME/battery-gates"; rm -rf "$GATE_LOG_DIR" || exit 125; mkdir -p "$GATE_LOG_DIR" || exit 125
exec 3>>"$PROTOCOL_TMP" || exit 125
exec 4<"$PROTOCOL_TMP" || exit 125
rm -f "$PROTOCOL_TMP" || exit 125
protocol_emit(){ printf '%s\n' "$1" >&3 || exit 125; }
protocol_emit "ANUBIS_VM_PROTOCOL_V1"
protocol_emit "ANUBIS_VM_BUILD_JOBS=$ANUBIS_VM_BUILD_JOBS"
protocol_emit "ANUBIS_VM_SELECTED_PIN $ANUBIS_VM_PIN $GUEST_PIN_SHA256 $GUEST_PIN_META_SHA256"
run(){
  name="$1"; shift
  gate_log="$GATE_LOG_DIR/$name.log"
  protocol_emit "ANUBIS_VM_GATE_BEGIN $name"
  if "$@" > "$gate_log" 2>&1 3>&- 4>&-; then rc=0; else rc=$?; fi
  if ! cat "$gate_log" >> "$LOG"; then rc=125; fi
  if [ "$name" = seal ]; then
    # Consume the gate's clean-slate machine artifact, not its human-readable note. The old parser
    # independently reconstructed that prose and silently drifted when the note gained its Mach-O
    # normalization qualifier. The helper admits only one regular, non-symlink file containing
    # exactly 64 lowercase hex bytes plus one newline.
    fixpoint_file="out/selfhost_gate/binary_fixpoint.sha256"
    if seal_fixpoint="$(python3 scripts/lib/read_exact_sha256.py "$fixpoint_file" 2>/dev/null)"; then
      protocol_emit "ANUBIS_VM_SEAL_FIXPOINT $seal_fixpoint"
    else
      protocol_emit "ANUBIS_VM_SEAL_FIXPOINT_INVALID"
      rc=125
    fi
  fi
  protocol_emit "ANUBIS_VM_GATE_RESULT $rc $name"
}
# Execute the selected immutable compiler before any source build. The strict protocol validator
# binds this named gate and the three-field pin identity to the host's opening values.
run pin-smoke "$ANUBIS_BIN" check tests/fixtures/language_core/hello_minimal.anb
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
# Build the synchronized source in release mode as a separate compilation check. Binary-aware gates
# below inherit ANUBIS_BIN and therefore grade the selected immutable pin, not this mutable build or
# a stale release binary baked into the golden image.
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
# Gates the seal advertises but never ran. A seal that does not execute these certifies a board
# whose headline numbers it never checked: 162 Lean theorems, the 104/104 fail-closed stdlib
# matrix, the 882-file native-authoritative corpus, the docs-drift stamps, and walker totality.
# `run_stdlib_gate.sh` above is the fixed-scenario integration gate — NOT the fail-closed matrix.
run stdlib-fc  bash scripts/run_stdlib_failclosed_gate.sh --out out/vm_stdlib_failclosed
run native-auth bash scripts/run_native_authoritative_gate.sh
run docs-drift bash scripts/run_docs_drift_gate.sh --out out/vm_docs_drift
run walker     bash scripts/run_walker_completeness_gate.sh
run formal     bash scripts/run_formal_gate.sh
# The formal-kernel demo, under DEFAULT Safe verification. It was registered in NO pipeline — not
# here, not audit_unified, not the seal checklist — so a 19/19 battery certified a tree whose
# formal-kernel lane was red. The gate refuses to run under ANUBIS_WRAP_SAFETY=0.
run formal-kernel bash scripts/run_formal_kernel_gate.sh
# The correspondence map + TCB list: cited theorems and paths must still resolve.
run correspondence bash scripts/run_proof_correspondence_gate.sh
# Bind the wrapper-only protocol to the exact child-output bytes after every gate has returned.
LOG_SHA256="$(shasum -a 256 "$LOG" | cut -d' ' -f1)"
LOG_BYTES="$(/usr/bin/stat -f '%z' "$LOG")"
protocol_emit "ANUBIS_VM_LOG_SHA256 $LOG_SHA256 $LOG_BYTES"
protocol_emit "ANUBIS_VM_BATTERY_DONE"
exec 3>&-
cat <&4 || exit 125
exec 4<&-
REMOTE
REMOTE_RC=$?
set -e
printf '%s\n' "$REMOTE_RC" > "$HOST_EVIDENCE_DIR/remote_ssh_exit_code.txt"

if capture_guest_source_manifest after_battery; then
  GUEST_MANIFEST_AFTER_RC=0
else
  GUEST_MANIFEST_AFTER_RC=$?
fi
if capture_guest_pin_identity after_battery; then
  GUEST_PIN_AFTER_RC=0
else
  GUEST_PIN_AFTER_RC=$?
fi
if [[ $GUEST_MANIFEST_AFTER_RC -eq 0 ]]; then
  GUEST_SOURCE_TREE_AFTER="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/guest_source_manifest_after_battery.json")"
  printf 'guest_source_tree_sha256_after_battery=%s\n' "$GUEST_SOURCE_TREE_AFTER" \
    >> "$HOST_EVIDENCE_DIR/source_epoch.txt"
else
  GUEST_SOURCE_TREE_AFTER=""
fi

echo "[5/6] collect + validate exact results"
BATTERY_LOG="$HOST_EVIDENCE_DIR/battery.log"
CARGO_EVIDENCE="$HOST_EVIDENCE_DIR/cargo-test.json"
set +e
scp "${SSHOPTS[@]}" "${USER_}@${IP}:battery.log" "$BATTERY_LOG" \
  >"$HOST_EVIDENCE_DIR/battery.scp.stdout" 2>"$HOST_EVIDENCE_DIR/battery.scp.stderr"
BATTERY_SCP_RC=$?
scp "${SSHOPTS[@]}" "${USER_}@${IP}:anubis-lang/.ammit/cargo-test.json" "$CARGO_EVIDENCE" \
  >"$HOST_EVIDENCE_DIR/cargo-test.scp.stdout" 2>"$HOST_EVIDENCE_DIR/cargo-test.scp.stderr"
CARGO_SCP_RC=$?
set -e
printf '%s\n' "$BATTERY_SCP_RC" > "$HOST_EVIDENCE_DIR/battery_scp_exit_code.txt"
printf '%s\n' "$REMOTE_RC" > "$HOST_EVIDENCE_DIR/protocol_transport_exit_code.txt"
printf '%s\n' "$CARGO_SCP_RC" > "$HOST_EVIDENCE_DIR/cargo_test_scp_exit_code.txt"

rc=0
if [[ $REMOTE_RC -ne 0 ]]; then
  echo "  ✗ remote battery SSH exited $REMOTE_RC"
  rc=1
fi
if [[ $GUEST_MANIFEST_AFTER_RC -ne 0 ]]; then
  echo "  ✗ guest source manifest could not be captured after the battery"
  rc=1
elif ! cmp -s "$HOST_EVIDENCE_DIR/guest_source_manifest_before_battery.json" \
  "$HOST_EVIDENCE_DIR/guest_source_manifest_after_battery.json"; then
  echo "  ✗ guest source tree changed during the VM battery (before=$GUEST_SOURCE_TREE_BEFORE after=$GUEST_SOURCE_TREE_AFTER)"
  rc=1
fi
if [[ $GUEST_PIN_AFTER_RC -ne 0 ]]; then
  echo "  ✗ guest pin identity could not be captured after the battery"
  rc=1
elif ! cmp -s "$HOST_EVIDENCE_DIR/guest_pin_identity_before_battery.txt" \
  "$HOST_EVIDENCE_DIR/guest_pin_identity_after_battery.txt"; then
  echo "  ✗ guest immutable pin or metadata changed during the VM battery"
  rc=1
fi
if [[ $BATTERY_SCP_RC -ne 0 || ! -s "$BATTERY_LOG" || -L "$BATTERY_LOG" \
  || ! -s "$PROTOCOL_LOG" || -L "$PROTOCOL_LOG" ]]; then
  echo "  ✗ battery log/protocol were not exported as non-empty regular files (log scp=$BATTERY_SCP_RC protocol transport=ssh-stdout)"
  rc=1
  VALIDATOR_RC=2
else
  set +e
  python3 "$REPO/scripts/lib/vm_battery_validate.py" \
    --log "$BATTERY_LOG" \
    --protocol "$PROTOCOL_LOG" \
    --out "$HOST_EVIDENCE_DIR/battery_verdict.json" \
    --expected-fixpoint "$EXPECTED" \
    --expected-jobs "$BUILD_JOBS" \
    --expected-pin "$CURRENT_PIN" \
    --expected-pin-sha256 "$CURRENT_PIN_SHA256" \
    --expected-pin-meta-sha256 "$CURRENT_PIN_META_SHA256" \
    > "$HOST_EVIDENCE_DIR/validator.stdout" \
    2> "$HOST_EVIDENCE_DIR/validator.stderr"
  VALIDATOR_RC=$?
  set -e
  if [[ $VALIDATOR_RC -ne 0 ]]; then
    echo "  ✗ strict battery state-machine validation failed (rc=$VALIDATOR_RC)"
    rc=1
  fi
fi
printf '%s\n' "$VALIDATOR_RC" > "$HOST_EVIDENCE_DIR/validator_exit_code.txt"

if [[ $CARGO_SCP_RC -ne 0 || ! -s "$CARGO_EVIDENCE" || -L "$CARGO_EVIDENCE" ]]; then
  echo "  ✗ .ammit/cargo-test.json was not exported as non-empty machine evidence (scp rc=$CARGO_SCP_RC)"
  rc=1
else
  mkdir -p "$REPO/.ammit"
  cp "$CARGO_EVIDENCE" "$REPO/.ammit/cargo-test.json"
  shasum -a 256 "$CARGO_EVIDENCE" > "$HOST_EVIDENCE_DIR/cargo-test.sha256"
  echo "      collected .ammit/cargo-test.json (Ammit evidence)"
fi

if [[ -f "$HOST_EVIDENCE_DIR/battery_verdict.json" && ! -L "$HOST_EVIDENCE_DIR/battery_verdict.json" ]]; then
  python3 - "$HOST_EVIDENCE_DIR/battery_verdict.json" <<'PY'
import json, sys
verdict = json.load(open(sys.argv[1], encoding="utf-8"))
for name in verdict.get("expected_gates", []):
    code = verdict.get("exit_codes", {}).get(name, "MISSING")
    print(f"      EXIT={code} {name}")
print(f"      BATTERY_DONE_COUNT={verdict.get('battery_done_count')}")
print(f"      VM_BUILD_JOBS={verdict.get('vm_build_jobs')}")
print(f"      FIXPOINTS={verdict.get('fixpoints')}")
for error in verdict.get("errors", []):
    print(f"      VALIDATION_ERROR: {error}")
PY
  VMFP="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); f=d.get("fixpoints") or []; print(f[0] if len(f)==1 else "")' "$HOST_EVIDENCE_DIR/battery_verdict.json")"
  NFAIL="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(sum(1 for x in d.get("exit_codes",{}).values() if x != 0))' "$HOST_EVIDENCE_DIR/battery_verdict.json")"
else
  VMFP=""
  NFAIL="unknown"
fi

shasum -a 256 "$BATTERY_LOG" > "$HOST_EVIDENCE_DIR/battery.log.sha256" 2>/dev/null || true
shasum -a 256 "$PROTOCOL_LOG" > "$HOST_EVIDENCE_DIR/battery.protocol.sha256" 2>/dev/null || true
printf 'remote_ssh=%s\nbattery_scp=%s\nprotocol_transport=%s\ncargo_scp=%s\nvalidator=%s\n' \
  "$REMOTE_RC" "$BATTERY_SCP_RC" "$REMOTE_RC" "$CARGO_SCP_RC" "$VALIDATOR_RC" \
  > "$HOST_EVIDENCE_DIR/transport_exit_codes.txt"

echo "[6/6] verdict"
echo "      gate failures : $NFAIL"
echo "      VM fixpoint   : ${VMFP:-<none>}"
echo "      expected      : $EXPECTED"
echo "      evidence      : $HOST_EVIDENCE_DIR"

write_source_manifest "$HOST_EVIDENCE_DIR/host_source_manifest_after.json"
HOST_SOURCE_TREE_AFTER="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/host_source_manifest_after.json")"
printf 'source_tree_sha256_after=%s\n' "$HOST_SOURCE_TREE_AFTER" >> "$HOST_EVIDENCE_DIR/source_epoch.txt"
if [[ "$HOST_SOURCE_TREE_AFTER" != "$HOST_SOURCE_TREE_BEFORE" ]]; then
  echo "  ✗ host source tree changed during VM run (before=$HOST_SOURCE_TREE_BEFORE after=$HOST_SOURCE_TREE_AFTER)"
  rc=1
fi
HOST_HEAD_AFTER="$(git -C "$REPO" rev-parse --verify 'HEAD^{commit}' 2>/dev/null || true)"
HOST_GIT_TREE_AFTER="$(git -C "$REPO" rev-parse --verify 'HEAD^{tree}' 2>/dev/null || true)"
printf 'head_after=%s\ngit_tree_after=%s\n' "$HOST_HEAD_AFTER" "$HOST_GIT_TREE_AFTER" \
  >> "$HOST_EVIDENCE_DIR/git_epoch.txt"
if [[ "$HOST_HEAD_AFTER" != "$HOST_HEAD_BEFORE" || "$HOST_GIT_TREE_AFTER" != "$HOST_GIT_TREE_BEFORE" ]]; then
  echo "  ✗ Git commit/tree changed during VM run"
  rc=1
fi
set +e
(cd "$REPO" && bash scripts/publish_pin.sh "$PIN_VERIFY_FLAG") \
  > "$HOST_EVIDENCE_DIR/pin_verify_after.stdout" \
  2> "$HOST_EVIDENCE_DIR/pin_verify_after.stderr"
PIN_VERIFY_AFTER_RC=$?
set -e
printf '%s\n' "$PIN_VERIFY_AFTER_RC" > "$HOST_EVIDENCE_DIR/pin_verify_after_exit_code.txt"
if [[ $PIN_VERIFY_AFTER_RC -ne 0 ]]; then
  echo "  ✗ closing pin verification failed in $PIN_VERIFY_FLAG mode"
  rc=1
else
  set +e
  CURRENT_PIN_AFTER="$(cd "$REPO" && bash scripts/publish_pin.sh --current)"
  CURRENT_PIN_AFTER_RC=$?
  set -e
  printf '%s\n' "$CURRENT_PIN_AFTER_RC" > "$HOST_EVIDENCE_DIR/pin_current_after_exit_code.txt"
  if [[ $CURRENT_PIN_AFTER_RC -ne 0 ]]; then
    echo "  ✗ closing CURRENT pin resolution failed"
    rc=1
  else
    CURRENT_PIN_SHA256_AFTER="$(shasum -a 256 "$REPO/$CURRENT_PIN_AFTER" | awk '{print $1}')"
    CURRENT_PIN_META_SHA256_AFTER="$(shasum -a 256 "$REPO/$CURRENT_PIN_AFTER.meta" | awk '{print $1}')"
    printf 'pin=%s\npin_sha256=%s\nmeta_sha256=%s\n' \
      "$CURRENT_PIN_AFTER" "$CURRENT_PIN_SHA256_AFTER" "$CURRENT_PIN_META_SHA256_AFTER" \
      > "$HOST_EVIDENCE_DIR/host_pin_identity_after.txt"
    if ! cmp -s "$HOST_EVIDENCE_DIR/host_pin_identity.txt" \
      "$HOST_EVIDENCE_DIR/host_pin_identity_after.txt"; then
      echo "  ✗ host CURRENT pin identity changed during VM run"
      rc=1
    fi
  fi
fi

# Teardown is part of the verdict, not an after-verdict best effort. A stopped
# guest is still present and still owns host resources; only absence from Tart's
# inventory seals this disposable run. `--keep` is a debugging mode and can
# never emit the committable PASS below.
trap - EXIT
if [ "$KEEP" = 1 ]; then
  cleanup
  echo "  ✗ --keep requested: guest retained, so disposable-isolation seal is unavailable"
  rc=1
elif ! cleanup; then
  echo "  ✗ guest teardown was not verified"
  rc=1
fi

if ! python3 "$REPO/scripts/lib/bundle_manifest.py" rehash --bundle "$HOST_EVIDENCE_DIR"; then
  echo "  ✗ VM evidence manifest could not be sealed"
  rc=1
fi

# Close the epoch again only after verified teardown and an initial complete bundle rehash. These
# records make that last check inspectable; the second rehash below incorporates them, and the
# read-only verify is the final operation before a PASS can be emitted.
FINAL_EPOCH_OK=1
if ! write_source_manifest "$HOST_EVIDENCE_DIR/host_source_manifest_final.json"; then
  echo "  ✗ final host source manifest could not be captured after teardown"
  FINAL_EPOCH_OK=0
else
  HOST_SOURCE_TREE_FINAL="$(source_tree_sha256 "$HOST_EVIDENCE_DIR/host_source_manifest_final.json" 2>/dev/null || true)"
  printf 'source_tree_sha256_final=%s\n' "$HOST_SOURCE_TREE_FINAL" >> "$HOST_EVIDENCE_DIR/source_epoch.txt"
  if [[ "$HOST_SOURCE_TREE_FINAL" != "$HOST_SOURCE_TREE_BEFORE" ]]; then
    echo "  ✗ host source tree changed after teardown (before=$HOST_SOURCE_TREE_BEFORE final=${HOST_SOURCE_TREE_FINAL:-<invalid>})"
    FINAL_EPOCH_OK=0
  fi
fi
HOST_HEAD_FINAL="$(git -C "$REPO" rev-parse --verify 'HEAD^{commit}' 2>/dev/null || true)"
HOST_GIT_TREE_FINAL="$(git -C "$REPO" rev-parse --verify 'HEAD^{tree}' 2>/dev/null || true)"
printf 'head_final=%s\ngit_tree_final=%s\n' "$HOST_HEAD_FINAL" "$HOST_GIT_TREE_FINAL" \
  >> "$HOST_EVIDENCE_DIR/git_epoch.txt"
if [[ "$HOST_HEAD_FINAL" != "$HOST_HEAD_BEFORE" || "$HOST_GIT_TREE_FINAL" != "$HOST_GIT_TREE_BEFORE" ]]; then
  echo "  ✗ Git commit/tree changed after teardown"
  FINAL_EPOCH_OK=0
fi
set +e
(cd "$REPO" && bash scripts/publish_pin.sh "$PIN_VERIFY_FLAG") \
  > "$HOST_EVIDENCE_DIR/pin_verify_final.stdout" \
  2> "$HOST_EVIDENCE_DIR/pin_verify_final.stderr"
PIN_VERIFY_FINAL_RC=$?
set -e
printf '%s\n' "$PIN_VERIFY_FINAL_RC" > "$HOST_EVIDENCE_DIR/pin_verify_final_exit_code.txt"
if [[ $PIN_VERIFY_FINAL_RC -ne 0 ]]; then
  echo "  ✗ final pin verification failed after teardown in $PIN_VERIFY_FLAG mode"
  FINAL_EPOCH_OK=0
else
  set +e
  CURRENT_PIN_FINAL="$(cd "$REPO" && bash scripts/publish_pin.sh --current)"
  CURRENT_PIN_FINAL_RC=$?
  if [[ $CURRENT_PIN_FINAL_RC -eq 0 ]]; then
    CURRENT_PIN_SHA256_FINAL="$(shasum -a 256 "$REPO/$CURRENT_PIN_FINAL" | awk '{print $1}')"
    CURRENT_PIN_SHA256_FINAL_RC=$?
    CURRENT_PIN_META_SHA256_FINAL="$(shasum -a 256 "$REPO/$CURRENT_PIN_FINAL.meta" | awk '{print $1}')"
    CURRENT_PIN_META_SHA256_FINAL_RC=$?
  else
    CURRENT_PIN_SHA256_FINAL=""
    CURRENT_PIN_SHA256_FINAL_RC=1
    CURRENT_PIN_META_SHA256_FINAL=""
    CURRENT_PIN_META_SHA256_FINAL_RC=1
  fi
  set -e
  printf '%s\n' "$CURRENT_PIN_FINAL_RC" > "$HOST_EVIDENCE_DIR/pin_current_final_exit_code.txt"
  printf 'pin=%s\npin_sha256=%s\nmeta_sha256=%s\n' \
    "$CURRENT_PIN_FINAL" "$CURRENT_PIN_SHA256_FINAL" "$CURRENT_PIN_META_SHA256_FINAL" \
    > "$HOST_EVIDENCE_DIR/host_pin_identity_final.txt"
  if [[ $CURRENT_PIN_FINAL_RC -ne 0 || $CURRENT_PIN_SHA256_FINAL_RC -ne 0 \
     || $CURRENT_PIN_META_SHA256_FINAL_RC -ne 0 ]] \
     || ! cmp -s "$HOST_EVIDENCE_DIR/host_pin_identity.txt" \
       "$HOST_EVIDENCE_DIR/host_pin_identity_final.txt"; then
    echo "  ✗ host CURRENT pin identity changed or became unreadable after teardown"
    FINAL_EPOCH_OK=0
  fi
fi
if [[ $FINAL_EPOCH_OK -ne 1 ]]; then
  rc=1
fi

if ! python3 "$REPO/scripts/lib/bundle_manifest.py" rehash --bundle "$HOST_EVIDENCE_DIR"; then
  echo "  ✗ final VM evidence manifest rehash failed"
  rc=1
fi
if ! python3 "$REPO/scripts/lib/bundle_manifest.py" verify --bundle "$HOST_EVIDENCE_DIR"; then
  echo "  ✗ final VM evidence manifest verification failed"
  rc=1
fi

if [ "$rc" = 0 ]; then
  echo
  echo "PASS — all gates green, fixpoint unchanged. Safe to commit on the host:"
  echo "  cd $REPO && git add <your slice files> && git commit"
else
  echo
  echo "FAIL — do NOT commit. See the battery output above."
fi
exit $rc
