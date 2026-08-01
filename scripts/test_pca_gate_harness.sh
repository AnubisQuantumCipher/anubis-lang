#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/anubis-pca-harness.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
pass=0
fail=0
ok() { pass=$((pass + 1)); printf 'PASS %s\n' "$*"; }
bad() { fail=$((fail + 1)); printf 'FAIL %s\n' "$*"; }

# shellcheck source=scripts/lib/pca_gate_harness.sh
source scripts/lib/pca_gate_harness.sh

FAKE="$TMP/fake-anubis"
cat > "$FAKE" <<'SH'
#!/usr/bin/env bash
out="${@: -1}"
mkdir -p "$out"
count="${FAKE_BUNDLES:-1}"
i=1
while [[ "$i" -le "$count" ]]; do
  bundle="$out/evidence-$i"
  mkdir -p "$bundle"
  printf '{}\n' > "$bundle/pca.json"
  printf 'source\n' > "$bundle/source.anubis"
  printf '{}\n' > "$bundle/evidence.json"
  printf 'manifest\n' > "$bundle/MANIFEST.sha256"
  i=$((i + 1))
done
if [[ "${FAKE_SYMLINK_PCA:-0}" == "1" ]]; then
  printf '{}\n' > "$out/outside.json"
  rm "$out/evidence-1/pca.json"
  ln -s "$out/outside.json" "$out/evidence-1/pca.json"
fi
exit "${FAKE_RC:-0}"
SH
chmod +x "$FAKE"
PROG="$TMP/prog.anb"
printf 'fn main() {}\n' > "$PROG"

FAKE_RC=0 FAKE_BUNDLES=1 pca_generate_evidence_bundle "$FAKE" "$PROG" "$TMP/clean" >"$TMP/clean.out" 2>"$TMP/clean.err"
clean_rc=$?
if [[ $clean_rc -eq 0 && "$(cat "$TMP/clean.out")" == "$TMP/clean/evidence-1" ]]; then
  ok "zero-exit command with one complete regular bundle is admitted"
else
  bad "clean PCA setup was not admitted exactly (rc=$clean_rc)"
fi

FAKE_RC=7 FAKE_BUNDLES=1 pca_generate_evidence_bundle "$FAKE" "$PROG" "$TMP/nonzero" >"$TMP/nonzero.out" 2>"$TMP/nonzero.err"
nonzero_rc=$?
if [[ $nonzero_rc -ne 0 ]] && grep -q 'PCA_GATE_SETUP_ERROR: evidence command exited 7' "$TMP/nonzero.err"; then
  ok "nonzero evidence command is rejected even when it leaves a complete bundle"
else
  bad "nonzero evidence command was not rejected explicitly (rc=$nonzero_rc)"
fi

FAKE_RC=0 FAKE_BUNDLES=2 pca_generate_evidence_bundle "$FAKE" "$PROG" "$TMP/multiple" >"$TMP/multiple.out" 2>"$TMP/multiple.err"
multiple_rc=$?
if [[ $multiple_rc -ne 0 ]] && grep -q 'PCA_GATE_SETUP_ERROR' "$TMP/multiple.err"; then
  ok "multiple evidence bundles fail closed"
else
  bad "multiple evidence bundles were admitted (rc=$multiple_rc)"
fi

FAKE_RC=0 FAKE_BUNDLES=0 pca_generate_evidence_bundle "$FAKE" "$PROG" "$TMP/none" >"$TMP/none.out" 2>"$TMP/none.err"
none_rc=$?
if [[ $none_rc -ne 0 ]] && grep -q 'PCA_GATE_SETUP_ERROR' "$TMP/none.err"; then
  ok "missing evidence bundle fails closed"
else
  bad "missing evidence bundle was admitted (rc=$none_rc)"
fi

FAKE_RC=0 FAKE_BUNDLES=1 FAKE_SYMLINK_PCA=1 pca_generate_evidence_bundle "$FAKE" "$PROG" "$TMP/symlink" >"$TMP/symlink.out" 2>"$TMP/symlink.err"
symlink_rc=$?
if [[ $symlink_rc -ne 0 ]] && grep -q 'PCA_GATE_SETUP_ERROR' "$TMP/symlink.err"; then
  ok "symlink required bundle member fails closed"
else
  bad "symlink required bundle member was admitted (rc=$symlink_rc)"
fi

if grep -q 'source "$ROOT/scripts/lib/pca_gate_harness.sh"' scripts/run_pca_gate.sh \
  && ! grep -qE 'find .*evidence-.*\|[[:space:]]*head' scripts/run_pca_gate.sh; then
  ok "PCA gate uses the strict setup harness"
else
  bad "PCA gate bypasses the strict setup harness"
fi

if grep -q 'ANUBIS_BIN' scripts/run_pca_gate.sh \
  && grep -q 'using supplied pinned binary' scripts/run_pca_gate.sh; then
  ok "PCA gate can consume a frozen supplied pin instead of forcing a rebuild"
else
  bad "PCA gate cannot be bound to a supplied immutable pin"
fi

if grep -q 'pca_version = 1' scripts/run_pca_gate.sh \
  && grep -q 'unknown_phase1_probe' scripts/run_pca_gate.sh; then
  ok "PCA gate separates v1-only and arbitrary unknown-field poison cases"
else
  bad "PCA gate lacks independent v1-only/unknown-field poison cases"
fi

echo "PCA_GATE_HARNESS: $pass passed, $fail failed"
[[ $fail -eq 0 ]]
