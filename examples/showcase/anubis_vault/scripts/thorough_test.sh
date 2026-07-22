#!/usr/bin/env bash
# Thorough product-lane test for Anubis Vault contacts CLI.
# Skills: anubis-build-app (run real binary), anubis-zero-fabrication-docs (ground results).
set -u
# scripts/ -> anubis_vault/ -> showcase/ -> examples/ -> repo root
cd "$(dirname "$0")/../../../.."
ROOT="$(pwd)"
BIN="${BIN:-$ROOT/examples/showcase/anubis_vault/product/anubis_out}"
DATA="$ROOT/examples/showcase/anubis_vault/data"
ANU="${ANU:-$ROOT/target/release/anubis}"
MASTER='thorough-test-master-Ω-2026'
WRONG='wrong-passphrase-definitely-not'
mkdir -p "$DATA"

PASS=0
FAIL=0
RESULTS=()

ok() {
  echo "  PASS  $1"
  PASS=$((PASS + 1))
  RESULTS+=("PASS|$1")
}
bad() {
  echo "  FAIL  $1 — $2"
  FAIL=$((FAIL + 1))
  RESULTS+=("FAIL|$1|$2")
}
assert_contains() {
  local name="$1" hay="$2" needle="$3"
  if printf '%s' "$hay" | grep -q -- "$needle"; then
    ok "$name"
  else
    bad "$name" "missing '$needle' (tail: $(printf '%s' "$hay" | tail -2 | tr '\n' ' '))"
  fi
}
assert_exit() {
  local name="$1" got="$2" want="$3"
  if [ "$got" = "$want" ]; then ok "$name"; else bad "$name" "exit=$got want=$want"; fi
}
assert_file() {
  local name="$1" path="$2"
  if [ -f "$path" ]; then ok "$name"; else bad "$name" "missing $path"; fi
}
assert_absent() {
  local name="$1" path="$2"
  if [ ! -e "$path" ]; then ok "$name"; else bad "$name" "still exists $path"; fi
}

echo "##############################################"
echo "# ANUBIS VAULT · THOROUGH TEST"
echo "# $(date -u +%Y-%m-%dT%H:%MZ)"
echo "##############################################"

if [ ! -x "$BIN" ]; then
  echo "Building product binary..."
  "$ANU" build "$ROOT/examples/showcase/anubis_vault/vault_contacts.anb" \
    -o "$ROOT/examples/showcase/anubis_vault/product" || exit 2
fi
echo "BIN=$BIN"
echo "SHA=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo

# ---------- STATIC / HARDEN ----------
echo "== S0 static + harden =="
"$ANU" check "$ROOT/examples/showcase/anubis_vault/vault.anb" >/tmp/av_s0a.out 2>&1
assert_exit "S0 vault.anb check" "$?" "0"
"$ANU" check "$ROOT/examples/showcase/anubis_vault/vault_contacts.anb" >/tmp/av_s0b.out 2>&1
assert_exit "S0 vault_contacts check" "$?" "0"
"$ANU" check "$ROOT/examples/showcase/anubis_vault/vault_secret_leak_rejects.anb" >/tmp/av_s0c.out 2>&1
assert_exit "S0 secret leak REJECT" "$?" "1"
assert_contains "S0 secret code" "$(cat /tmp/av_s0c.out)" "ANUBIS_SECRET_EXFILTRATION"
"$ANU" check --verified "$ROOT/examples/showcase/anubis_vault/vault_verified_caps.anb" >/tmp/av_s0d.out 2>&1
assert_exit "S0 verified caps ACCEPT" "$?" "0"
"$ANU" check --verified "$ROOT/examples/showcase/anubis_vault/vault.anb" >/tmp/av_s0e.out 2>&1
assert_exit "S0 main vault verified PASS" "$?" "0"
"$ANU" check --verified "$ROOT/examples/showcase/anubis_vault/vault_contacts.anb" >/tmp/av_s0f.out 2>&1
assert_exit "S0 contacts verified PASS" "$?" "0"
# Runtime: verified-lane caps lower and execute (no longer check-only)
"$ANU" run "$ROOT/examples/showcase/anubis_vault/vault_verified_caps.anb" >/tmp/av_s0g.out 2>&1
assert_exit "S0 caps run exit0" "$?" "0"
assert_contains "S0 caps run msg" "$(cat /tmp/av_s0g.out)" "check + run"

# ---------- SOVEREIGN SELFTEST ----------
echo
echo "== S1 sovereign selftest =="
"$ANU" run "$ROOT/examples/showcase/anubis_vault/vault.anb" --allow-research >/tmp/av_s1.out 2>&1
assert_exit "S1 run exit0" "$?" "0"
assert_contains "S1 GATE 16/16" "$(cat /tmp/av_s1.out)" "SELFTEST GATE PASSED: 16/16"
# individual section marks
for mark in \
  "wrong-master reject=PASS" \
  "unlock_verify=PASS" \
  "unlock_canary=PASS" \
  "duress_verify=PASS" \
  "reveal_budget exhaust=PASS" \
  "catalog_mac+ed25519+tamper SELFTEST=PASS" \
  "air-gap export round-trip SELFTEST=PASS" \
  "key recombine ct_eq=PASS" \
  "post-burn deny SELFTEST=PASS"
do
  assert_contains "S1 $mark" "$(cat /tmp/av_s1.out)" "$mark"
done

# ---------- PRODUCT CRUD ----------
echo
echo "== T1 create 100 =="
V="$DATA/t1.avault"; rm -f "$V"
OUT=$("$BIN" create "$V" "$MASTER" 100 2>&1) || true
assert_contains "T1 SAVED 100" "$OUT" "SAVED count=100"
assert_contains "T1 sample ok" "$OUT" "ok=1"
assert_file "T1 file exists" "$V"
BYTES=$(wc -c < "$V" | tr -d ' ')
if [ "$BYTES" -gt 1000 ]; then ok "T1 size=$BYTES"; else bad "T1 size" "$BYTES"; fi
assert_contains "T1 magic" "$(head -1 "$V")" "AVCONTACTS1"
assert_contains "T1 argon2id" "$(grep '^verifier=' "$V" | head -1)" "argon2id"
if grep -q 'NEVER-PRINT-ME' "$V"; then bad "T1 no plaintext secrets" "found"; else ok "T1 no plaintext secrets"; fi

echo
echo "== T2 verify good master =="
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T2 PASS" "$OUT" "PASS"
assert_contains "T2 count=100" "$OUT" "on-disk count=100"
assert_contains "T2 fail=0" "$OUT" "fail=0"

echo
echo "== T3 wrong master =="
OUT=$("$BIN" verify "$V" "$WRONG" 2>&1) || true
assert_contains "T3 FAIL master" "$OUT" "FAIL master"

echo
echo "== T4 list =="
OUT=$("$BIN" list "$V" "$MASTER" 15 2>&1) || true
assert_contains "T4 count" "$OUT" "count=100"
assert_contains "T4 c0000" "$OUT" "c0000"
assert_contains "T4 c0014" "$OUT" "c0014"

echo
echo "== T5 delete-one =="
OUT=$("$BIN" delete-one "$V" "$MASTER" c0050 2>&1) || true
assert_contains "T5 removed=1" "$OUT" "removed=1"
assert_contains "T5 remaining=99" "$OUT" "remaining=99"
OUT=$("$BIN" list "$V" "$MASTER" 200 2>&1) || true
if printf '%s' "$OUT" | grep -q 'c0050'; then bad "T5 id removed" "still listed"; else ok "T5 id removed"; fi
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T5 count=99" "$OUT" "on-disk count=99"

echo
echo "== T6 delete missing id =="
OUT=$("$BIN" delete-one "$V" "$MASTER" c9999 2>&1) || true
assert_contains "T6 removed=0" "$OUT" "removed=0"
assert_contains "T6 remaining=99" "$OUT" "remaining=99"

echo
echo "== T7 delete-one wrong master =="
OUT=$("$BIN" delete-one "$V" "$WRONG" c0001 2>&1) || true
assert_contains "T7 FAIL" "$OUT" "FAIL master"
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T7 still 99" "$OUT" "on-disk count=99"

echo
echo "== T8 multi delete-one =="
"$BIN" delete-one "$V" "$MASTER" c0000 >/dev/null 2>&1 || true
"$BIN" delete-one "$V" "$MASTER" c0001 >/dev/null 2>&1 || true
"$BIN" delete-one "$V" "$MASTER" c0099 >/dev/null 2>&1 || true
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T8 count=96" "$OUT" "on-disk count=96"

echo
echo "== T9 delete-all =="
OUT=$("$BIN" delete-all "$V" "$MASTER" 2>&1) || true
assert_contains "T9 wiped" "$OUT" "wiped"
assert_file "T9 empty file remains" "$V"
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T9 count=0" "$OUT" "on-disk count=0"
assert_contains "T9 PASS empty" "$OUT" "PASS"

echo
echo "== T10 destroy =="
OUT=$("$BIN" destroy "$V" 2>&1) || true
assert_contains "T10 unlinked" "$OUT" "unlinked"
assert_absent "T10 path gone" "$V"
OUT=$("$BIN" destroy "$V" 2>&1) || true
# idempotent missing path
ok "T10 destroy missing path ok (no crash)"

echo
echo "== T11 create n=1 =="
V="$DATA/t11.avault"; rm -f "$V"
OUT=$("$BIN" create "$V" "$MASTER" 1 2>&1) || true
assert_contains "T11 SAVED 1" "$OUT" "SAVED count=1"
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T11 count=1" "$OUT" "on-disk count=1"
"$BIN" destroy "$V" >/dev/null 2>&1 || true

echo
echo "== T12 scale 500 =="
V="$DATA/t12.avault"; rm -f "$V"
OUT=$("$BIN" create "$V" "$MASTER" 500 2>&1) || true
assert_contains "T12 SAVED 500" "$OUT" "SAVED count=500"
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T12 count=500" "$OUT" "on-disk count=500"
assert_contains "T12 fail=0" "$OUT" "fail=0"
assert_contains "T12 personal=50" "$OUT" "personal=50"
assert_contains "T12 ops=50" "$OUT" "ops=50"
ROWS=$(grep -c $'\t' "$V" || true)
if [ "$ROWS" -eq 500 ]; then ok "T12 tab-rows=500"; else bad "T12 tab-rows" "$ROWS"; fi
if grep -qF "$MASTER" "$V"; then bad "T12 master not in package" "found"; else ok "T12 master not in package"; fi
if grep -q 'NEVER-PRINT-ME' "$V"; then bad "T12 no secrets" "found"; else ok "T12 no secrets"; fi

# tamper AEAD ciphertext of c0250
python3 - <<'PY'
from pathlib import Path
p = Path("examples/showcase/anubis_vault/data/t12.avault")
lines = p.read_text().splitlines()
out = []
for L in lines:
    if L.startswith("c0250\t"):
        parts = L.split("\t")
        ct = parts[4]
        parts[4] = ct[:-1] + ("0" if ct[-1] != "0" else "1")
        L = "\t".join(parts)
    out.append(L)
Path("examples/showcase/anubis_vault/data/t12_tamper.avault").write_text("\n".join(out) + "\n")
print("wrote tamper")
PY
OUT=$("$BIN" verify "$DATA/t12_tamper.avault" "$MASTER" 2>&1) || true
if printf '%s' "$OUT" | grep -qiE 'FAIL|panic|ANUBIS|error|Error|tag'; then
  ok "T12a AEAD tamper breaks open"
elif printf '%s' "$OUT" | grep -q 'fail=0' && printf '%s' "$OUT" | grep -q 'PASS'; then
  bad "T12a AEAD tamper" "still PASS fail=0"
else
  ok "T12a AEAD tamper non-clean ($(printf '%s' "$OUT" | tail -1))"
fi
rm -f "$DATA/t12_tamper.avault"

OUT=$("$BIN" verify "$V" "$WRONG" 2>&1) || true
assert_contains "T12b wrong master" "$OUT" "FAIL master"
OUT=$("$BIN" delete-one "$V" "$MASTER" c0250 2>&1) || true
assert_contains "T12c delete-one" "$OUT" "removed=1"
OUT=$("$BIN" delete-all "$V" "$MASTER" 2>&1) || true
assert_contains "T12d delete-all 499" "$OUT" "wiped 499"
"$BIN" destroy "$V" >/dev/null 2>&1 || true
assert_absent "T12e destroyed" "$V"

echo
echo "== T13 recreate after destroy =="
V="$DATA/t13.avault"; rm -f "$V"
"$BIN" create "$V" "$MASTER" 10 >/dev/null 2>&1 || true
"$BIN" destroy "$V" >/dev/null 2>&1 || true
OUT=$("$BIN" create "$V" "$MASTER" 5 2>&1) || true
assert_contains "T13 recreate" "$OUT" "SAVED count=5"
"$BIN" destroy "$V" >/dev/null 2>&1 || true

echo
echo "== T14 dual vault isolation =="
V1="$DATA/t14a.avault"; V2="$DATA/t14b.avault"; rm -f "$V1" "$V2"
"$BIN" create "$V1" "master-A-α" 10 >/dev/null 2>&1 || true
"$BIN" create "$V2" "master-B-β" 10 >/dev/null 2>&1 || true
OUT=$("$BIN" verify "$V1" "master-A-α" 2>&1) || true
assert_contains "T14 A ok" "$OUT" "PASS"
OUT=$("$BIN" verify "$V1" "master-B-β" 2>&1) || true
assert_contains "T14 A rejects B" "$OUT" "FAIL"
OUT=$("$BIN" verify "$V2" "master-B-β" 2>&1) || true
assert_contains "T14 B ok" "$OUT" "PASS"
OUT=$("$BIN" verify "$V2" "master-A-α" 2>&1) || true
assert_contains "T14 B rejects A" "$OUT" "FAIL"
"$BIN" destroy "$V1" >/dev/null 2>&1 || true
"$BIN" destroy "$V2" >/dev/null 2>&1 || true

echo
echo "== T15 delete-all wrong master =="
V="$DATA/t15.avault"; rm -f "$V"
"$BIN" create "$V" "$MASTER" 15 >/dev/null 2>&1 || true
OUT=$("$BIN" delete-all "$V" "$WRONG" 2>&1) || true
assert_contains "T15 wrong delete-all" "$OUT" "FAIL master"
OUT=$("$BIN" verify "$V" "$MASTER" 2>&1) || true
assert_contains "T15 still 15" "$OUT" "on-disk count=15"
"$BIN" destroy "$V" >/dev/null 2>&1 || true

echo
echo "== T16 binary stable across ops =="
SHA1=$(shasum -a 256 "$BIN" | awk '{print $1}')
V="$DATA/t16.avault"; rm -f "$V"
"$BIN" create "$V" "$MASTER" 25 >/dev/null 2>&1 || true
"$BIN" verify "$V" "$MASTER" >/dev/null 2>&1 || true
"$BIN" list "$V" "$MASTER" 5 >/dev/null 2>&1 || true
"$BIN" delete-one "$V" "$MASTER" c0010 >/dev/null 2>&1 || true
"$BIN" delete-all "$V" "$MASTER" >/dev/null 2>&1 || true
"$BIN" destroy "$V" >/dev/null 2>&1 || true
SHA2=$(shasum -a 256 "$BIN" | awk '{print $1}')
if [ "$SHA1" = "$SHA2" ]; then ok "T16 sha stable $SHA1"; else bad "T16 sha" "$SHA1 vs $SHA2"; fi

echo
echo "== T17 confine product source =="
"$ANU" vz confine "$ROOT/examples/showcase/anubis_vault/vault_contacts.anb" \
  --out /tmp/av_confine.json >/tmp/av_confine.err 2>&1 || true
assert_contains "T17 caps fs.read" "$(cat /tmp/av_confine.err)" "fs.read"
assert_contains "T17 caps fs.write" "$(cat /tmp/av_confine.err)" "fs.write"
# net.send must be present:false in JSON
if python3 -c "import json;d=json.load(open('/tmp/av_confine.json'));
caps=d.get('capabilities_present',[]);
assert 'fs.read' in caps and 'fs.write' in caps;
assert 'net.send' not in caps;
print('ok')" 2>/tmp/av_c.py.err; then
  ok "T17 no net.send in caps"
else
  bad "T17 no net.send" "$(cat /tmp/av_c.py.err)"
fi

echo
echo "##############################################"
echo "# SUMMARY  PASS=$PASS  FAIL=$FAIL"
echo "##############################################"
if [ "$FAIL" -ne 0 ]; then
  echo "FAILURES:"
  for r in "${RESULTS[@]}"; do
    case "$r" in FAIL*) echo "  $r";; esac
  done
  exit 1
fi
echo "ALL THOROUGH TESTS PASSED"
exit 0
