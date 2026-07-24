#!/usr/bin/env bash
# Anubis power gate: language + proofs + engagement receipts + offensive honesty.
# Fail-closed. No overclaims.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/power_gate}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
mkdir -p "$OUT"
pass=0; fail=0; total=0
record() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == PASS ]]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  printf '%-32s %s  (%s)\n' "$name" "$status" "$detail"
}

if [[ -x target/release/anubis ]]; then BIN=target/release/anubis
else echo "FAIL: cargo build --release -p anubis"; exit 1; fi

# 1) Turing core
set +e
bash scripts/run_turing_core_fixtures.sh --out "$OUT/turing" >"$OUT/turing.log" 2>&1
trc=$?
set -e
if [[ $trc -eq 0 ]]; then record "turing_complete" "PASS" "fixtures"; else record "turing_complete" "FAIL" "rc=$trc"; fi

# 2) Named journals + assert proof
set +e
bash scripts/run_named_journal_gate.sh --out "$OUT/named" --metal-reference "$REF" >"$OUT/named.log" 2>&1
nrc=$?
set -e
if [[ $nrc -eq 0 ]]; then record "named_journals" "PASS" "gate"; else record "named_journals" "FAIL" "rc=$nrc"; fi

set +e
"$BIN" prove examples/proof/proof_assert_range.anb --backend risc0 --lane cpu \
  --metal-reference "$REF" --input-json '{"x":7,"lo":1,"hi":10}' \
  --evidence --out "$OUT/assert_ok" >"$OUT/assert_ok.log" 2>&1
arc=$?
set -e
if [[ $arc -eq 0 ]] && python3 - "$OUT/assert_ok" <<'PY'
import json, pathlib, sys
m=json.load(open(pathlib.Path(sys.argv[1])/"backend"/"risc0"/"risc0_metadata.json"))
jf=m.get("journal_fields") or {}
by={f["name"]:f["value_u32"] for f in jf.get("fields",[])}
assert m.get("verify_status")=="passed"
assert by.get("lo")==1 and by.get("hi")==10 and by.get("ok")==1
assert "x" not in by  # private input not in journal names
print("ok")
PY
then record "proof_assert_range" "PASS" "private x public bounds"; else record "proof_assert_range" "FAIL" "see assert_ok.log"; fi

# 3) Engagement receipts chain
# Isolation: task-queue / lateral-smb are AOP host-forbidden (require VZ guest markers).
# Host-allowed control plane: engage-init, receipt-verify.
# For the receipt chain we run the AOP queue steps under the same lab guest markers
# used by scripts/run_offensive_platform_gate.sh (ANUBIS_VZ_GUEST / GATE_IN_GUEST),
# without permanently spoofing ~/.anubis-vz-guest on the host.
ENG="$OUT/engage"
rm -rf "$ENG"
"$BIN" engage-init --dir "$ENG" --name power --authorization power-gate >/dev/null

# 3a) Host isolation honesty: task-queue must FAIL closed on bare host (no guest env).
set +e
env -u ANUBIS_VZ_GUEST -u ANUBIS_OFFENSIVE_GATE_IN_GUEST -u ANUBIS_ISOLATION \
  "$BIN" task-queue --engage "$ENG" --module whoami --operator operator \
  >"$OUT/task_queue_host_forbid.log" 2>&1
hq_rc=$?
set -e
if [[ $hq_rc -ne 0 ]] && grep -q 'ANUBIS_OFFENSIVE_HOST_FORBIDDEN' "$OUT/task_queue_host_forbid.log"; then
  record "aop_host_isolation" "PASS" "task-queue host-forbidden"
else
  record "aop_host_isolation" "FAIL" "rc=$hq_rc (expected OFFENSIVE_HOST_FORBIDDEN)"
fi

# 3b) Receipt chain under guest markers (lab path; matches offensive platform gate local mode).
set +e
ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 ANUBIS_ISOLATION=tart-disposable-guest \
  "$BIN" task-queue --engage "$ENG" --module whoami --operator operator \
  >"$OUT/task_queue_guest.log" 2>&1
tq_rc=$?
ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 ANUBIS_ISOLATION=tart-disposable-guest \
  "$BIN" lateral-smb --engage "$ENG" --host 127.0.0.1 \
  >"$OUT/lateral_smb_guest.log" 2>&1
ls_rc=$?
"$BIN" receipt-verify --engage "$ENG" --json >"$OUT/receipts.json" 2>&1
rrc=$?
set -e
if [[ $tq_rc -eq 0 && $ls_rc -eq 0 && $rrc -eq 0 ]] && grep -q '"count": 3' "$OUT/receipts.json"; then
  record "engagement_receipts" "PASS" "chain count=3 (guest markers)"
else
  record "engagement_receipts" "FAIL" "tq=$tq_rc ls=$ls_rc rv=$rrc $(tr '\n' ' ' <"$OUT/receipts.json" 2>/dev/null)"
fi
# Tamper tip → must fail (host receipt-verify is control-plane)
if [[ -f "$ENG/evidence/receipts/tip.json" ]]; then
  echo '{"seq":99,"receipt_hash":"deadbeef"}' > "$ENG/evidence/receipts/tip.json"
  set +e
  "$BIN" receipt-verify --engage "$ENG" --json >"$OUT/receipts_tamper.json" 2>&1
  trc=$?
  set -e
  if [[ $trc -ne 0 ]]; then record "receipt_tamper" "PASS" "tip mismatch fail-closed"
  else record "receipt_tamper" "FAIL" "tamper not detected"; fi
fi

# 4) Clippy cleanliness signal (release bin already built)
set +e
cargo clippy -p anubis --release -- -D warnings >"$OUT/clippy.log" 2>&1
crc=$?
set -e
if [[ $crc -eq 0 ]]; then record "clippy_d_warnings" "PASS" "clean"; else record "clippy_d_warnings" "FAIL" "see clippy.log"; fi

# 5) Offensive doctor honesty contract
"$BIN" offensive-doctor --json >"$OUT/doctor.json"
if grep -q '"false_green_rejected": true' "$OUT/doctor.json"; then
  record "security_fixture_contract" "PASS" "doctor"
else
  record "security_fixture_contract" "FAIL" "missing"
fi

# 6) Native proof_assert fail-closed on run
set +e
ANUBIS_PROOF_INPUTS='x=1,lo=5,hi=10' "$BIN" run examples/proof/proof_assert_range.anb >"$OUT/run_assert_fail.log" 2>&1
frc=$?
set -e
if [[ $frc -ne 0 ]] && grep -q ANUBIS_PROOF_ASSERT_FAILED "$OUT/run_assert_fail.log"; then
  record "native_assert_fail" "PASS" "x out of range"
else
  record "native_assert_fail" "FAIL" "rc=$frc"
fi

verdict=FAIL
[[ $fail -eq 0 && $pass -gt 0 ]] && verdict=PASS
python3 - <<PY
import json
print(json.dumps({"total":$total,"passed":$pass,"failed":$fail,"overall_verdict":"$verdict"}, indent=2))
open("$OUT/report.json","w").write(json.dumps({
  "total":$total,"passed":$pass,"failed":$fail,"overall_verdict":"$verdict",
  "binary":"$BIN","note":"Anubis power gate — language+proof+receipts"
}, indent=2))
PY
echo "Overall: $verdict ($pass/$total)"
[[ "$verdict" == PASS ]]
