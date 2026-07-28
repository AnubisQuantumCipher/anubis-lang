#!/usr/bin/env bash
# Source audit: a new corpus/fixture scorer must either use gate_common.sh or be
# listed here with a narrow reason. Unknown non-users fail the release seal.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

exception_reason() {
  case "$1" in
    scripts/audit_unified.sh) echo "aggregate gate; scores child gate verdicts" ;;
    scripts/check_capset_registry_parity.sh) echo "source-registry comparison; no fixture verdicts" ;;
    scripts/check_metal_parity.sh) echo "hardware proof matrix finalized by gate11-metal-parity" ;;
    scripts/gate10_a15_reproduce.sh) echo "historical A15 reproduction driver" ;;
    scripts/publish_pin.sh) echo "publishes a content-addressed binary pin; its stale-source guard globs *.anb and says 'fail', which trips the candidate heuristic — it scores no fixtures" ;;
    scripts/run_author_diversity_gate.sh) echo "source authorship audit, not an Anubis fixture corpus" ;;
    scripts/run_check_confine_run_gate.sh) echo "fixed end-to-end confinement scenario" ;;
    scripts/run_dx_gate.sh) echo "fixed DX assertions; EXPECT text is test-runner input" ;;
    scripts/run_enum_match_gate.sh) echo "fixed feature scenario" ;;
    scripts/run_essence_spine_gate.sh) echo "aggregate gate; scores child gate verdicts" ;;
    scripts/run_for_in_gate.sh) echo "fixed feature scenario" ;;
    scripts/run_lang_trio_gate.sh) echo "fixed feature scenario" ;;
    scripts/run_named_journal_gate.sh) echo "fixed proof-journal scenarios" ;;
    scripts/run_nexus_gate.sh) echo "aggregate gate; scores child gate verdicts" ;;
    scripts/run_package_gate.sh) echo "fixed package scenarios" ;;
    scripts/run_poc_kit_gate.sh) echo "fixed PoC-kit scenarios" ;;
    scripts/run_power_gate.sh) echo "aggregate gate; scores child gate verdicts" ;;
    scripts/run_proof_binding_gate.sh) echo "fixed proof-binding scenarios" ;;
    scripts/run_prove_gate.sh) echo "fixed prove scenarios" ;;
    scripts/run_seal_checklist.sh) echo "independent root-of-trust accounting avoids common-mode failure" ;;
    scripts/run_docs_drift_gate.sh) echo "scores DOCUMENTATION stamps against re-derived inventory, not .anb fixtures — gate_common's parse_expectation/score_fixture model does not apply" ;;
    scripts/run_selfhost_ddc_gate.sh) echo "compiler-stage DDC protocol, not fixture expectations" ;;
    scripts/run_selfhost_dogfood_gate.sh) echo "compiler-stage dogfood protocol" ;;
    scripts/run_selfhost_gate.sh) echo "compiler bootstrap stage protocol" ;;
    scripts/run_stdlib_gate.sh) echo "fixed stdlib integration scenarios" ;;
    *) return 1 ;;
  esac
}

total=0
migrated=0
excepted=0
failed=0

for script in scripts/*.sh; do
  if ! grep -q '\.anb' "$script" \
    || ! grep -Eq 'for .* in |find .*\.anb|fixtures=\(' "$script" \
    || ! grep -Eqi 'pass|fail|verdict|agree|mismatch|EXPECT' "$script"; then
    continue
  fi

  total=$((total + 1))
  if grep -qF 'scripts/lib/gate_common.sh' "$script" \
    && grep -Eq '^[[:space:]]*(if ! )?(parse_expectation|score_fixture|require_nonempty_corpus|finalize)[[:space:]]' "$script"; then
    migrated=$((migrated + 1))
    printf 'MIGRATED  %s\n' "$script"
    continue
  fi

  if reason="$(exception_reason "$script")"; then
    excepted=$((excepted + 1))
    printf 'EXCEPTION %s - %s\n' "$script" "$reason"
  else
    failed=$((failed + 1))
    printf 'ANOMALY   %s - fixture/corpus scorer neither uses gate_common nor has an exception\n' "$script"
  fi
done

set +e
finalize "$total" "$((migrated + excepted))" "$failed" 0
final_rc=$?
set -e
if [[ "$final_rc" -ne 0 ]]; then
  echo "GATE_COMMON_ADOPTION_GATE: FAIL (candidates=$total migrated=$migrated exceptions=$excepted anomalies=$failed; $GATE_FINAL_REASON)"
  exit 1
fi

echo "GATE_COMMON_ADOPTION_GATE: PASS (candidates=$total migrated=$migrated exceptions=$excepted anomalies=0)"
