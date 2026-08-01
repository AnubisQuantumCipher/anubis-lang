#!/usr/bin/env bash
# Poison-test the native-corpus inventory and pin-binding trust spine.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$ROOT"

# The hosted aggregate exports these controls for its real RISC0 build. This
# harness also exercises the production release publisher, whose contract
# correctly forbids every kernel/Metal bypass. Start the synthetic publisher
# cases from that clean release contract so their intended poison reaches the
# named check instead of all cases failing on an unrelated inherited bypass.
unset ANUBIS_SKIP_RISC0_METAL RISC0_SKIP_BUILD_KERNELS R0_DISABLE_METAL

pass=0
fail=0
TMP="$(mktemp -d "${TMPDIR:-/tmp}/anubis-corpus-binding.XXXXXX")"
TMP="$(cd "$TMP" && pwd -P)"
if [[ "${ANUBIS_TEST_KEEP_TMP:-0}" == "1" ]]; then
  echo "CORPUS_INVENTORY_BINDING_TMP=$TMP" >&2
else
  trap 'rm -rf "$TMP"' EXIT
fi

ok() { echo "PASS $1"; pass=$((pass + 1)); }
bad() { echo "FAIL $1"; fail=$((fail + 1)); }

policy_roots="$(python3 - <<'PY'
import json
with open("scripts/lib/pin_manifest_policy.json", encoding="utf-8") as handle:
    print("\n".join(json.load(handle)["roots"]))
PY
)"
policy_exact_exclusions="$(python3 - <<'PY'
import json
with open("scripts/lib/pin_manifest_policy.json", encoding="utf-8") as handle:
    print("\n".join(json.load(handle)["excluded_exact_directories"]))
PY
)"
policy_top_exclusions="$(python3 - <<'PY'
import json
with open("scripts/lib/pin_manifest_policy.json", encoding="utf-8") as handle:
    print("\n".join(json.load(handle)["excluded_top_level_entries"]))
PY
)"
if grep -Fxq examples <<<"$policy_roots" \
   && grep -Fxq scripts <<<"$policy_roots" \
   && grep -Fxq tests <<<"$policy_roots"; then
  ok "versioned pin policy binds examples, scripts, and tests"
else
  bad "versioned pin policy omits examples, scripts, or tests"
fi

if grep -Fxq .hermes <<<"$policy_top_exclusions" \
   && grep -Fxq adversary <<<"$policy_top_exclusions" \
   && grep -Fxq verify.log <<<"$policy_top_exclusions"; then
  ok "versioned pin policy classifies live operator-local top-level artifacts"
else
  bad "versioned pin policy omits an expected operator-local top-level artifact"
fi

if grep -Fxq tools <<<"$policy_roots" \
   && grep -Fxq vendor <<<"$policy_roots" \
   && grep -Fxq .github <<<"$policy_roots" \
   && grep -Fxq .cargo <<<"$policy_roots"; then
  ok "versioned pin policy binds CLI tests, vendor, CI, and build configuration"
else
  bad "versioned pin policy omits a release trust root"
fi

if grep -Fxq vm <<<"$policy_roots" \
   && ! grep -Eq '^vm/' <<<"$policy_roots" \
   && grep -Fxq vm/exports <<<"$policy_exact_exclusions" \
   && grep -Fxq vm/pins <<<"$policy_exact_exclusions"; then
  ok "versioned pin policy binds the whole vm source root except generated pins and exports"
else
  bad "versioned pin policy permits vm sibling source to escape"
fi

# The pristine file hash comes from `risc0-sys-1.5.0.crate` (crate SHA-256
# 960c8295fbb87e1e73e332f8f7de2fba0252377575042d9d3e9a4eb50a38e078).
# Reversing the checked-in patch must recover that exact upstream file, and
# forwarding it must recover the vendored bytes with no unrecorded divergence.
vendor_patch_fixture="$TMP/risc0-sys-build.rs"
cp vendor/risc0-sys/build.rs "$vendor_patch_fixture"
/usr/bin/patch -R -s "$vendor_patch_fixture" \
  < patches/risc0-sys-1.5.0-hosted-metal-placeholder.diff \
  >"$TMP/vendor-patch-reverse.out" 2>"$TMP/vendor-patch-reverse.err"
vendor_patch_reverse_rc=$?
vendor_upstream_sha="$(shasum -a 256 "$vendor_patch_fixture" | awk '{print $1}')"
/usr/bin/patch -s "$vendor_patch_fixture" \
  < patches/risc0-sys-1.5.0-hosted-metal-placeholder.diff \
  >"$TMP/vendor-patch-forward.out" 2>"$TMP/vendor-patch-forward.err"
vendor_patch_forward_rc=$?
cmp -s "$vendor_patch_fixture" vendor/risc0-sys/build.rs
vendor_patch_cmp_rc=$?
if [[ "$vendor_patch_reverse_rc" -eq 0 && "$vendor_patch_forward_rc" -eq 0 \
  && "$vendor_patch_cmp_rc" -eq 0 \
  && "$vendor_upstream_sha" == dfab9bfabc621ef471a4971802e0d0c9e927614de72118c4a87ff0c5455e9e69 ]]; then
  ok "vendored risc0-sys hosted divergence exactly matches its pinned upstream patch"
else
  bad "vendored risc0-sys hosted divergence escaped patch binding (reverse=$vendor_patch_reverse_rc forward=$vendor_patch_forward_rc cmp=$vendor_patch_cmp_rc upstream=$vendor_upstream_sha)"
fi

meta_publish_line="$(grep -n 'durable_publish_file "$scratch/metadata" "$meta" 0444' scripts/publish_pin.sh | cut -d: -f1)"
current_publish_line="$(grep -n 'durable_replace_pointer "$scratch/CURRENT" "$CURRENT"' scripts/publish_pin.sh | cut -d: -f1)"
if [[ -n "$meta_publish_line" && -n "$current_publish_line" && "$meta_publish_line" -lt "$current_publish_line" ]]; then
  ok "the complete immutable pair precedes atomic CURRENT selection"
else
  bad "atomic CURRENT selection can name an incomplete immutable pair"
fi

if [[ -f scripts/lib/native_corpus_inventory.py ]]; then
  ok "shared corpus inventory exists"
else
  bad "shared corpus inventory missing"
fi

python3 scripts/test_gate_run_freshness.py \
  >"$TMP/gate-run-freshness-tests.out" \
  2>"$TMP/gate-run-freshness-tests.err"
gate_freshness_tests_rc=$?
if [[ $gate_freshness_tests_rc -eq 0 ]]; then
  ok "run-local gate freshness ledger fault suite passes"
else
  bad "run-local gate freshness ledger fault suite failed (rc=$gate_freshness_tests_rc)"
fi

python3 scripts/test_gate_run_ledger_promote.py \
  >"$TMP/gate-run-ledger-promote-tests.out" \
  2>"$TMP/gate-run-ledger-promote-tests.err"
gate_ledger_promote_tests_rc=$?
if [[ $gate_ledger_promote_tests_rc -eq 0 ]]; then
  ok "gate-run ledger atomic no-replace promotion poison suite passes"
else
  bad "gate-run ledger promotion poison suite failed (rc=$gate_ledger_promote_tests_rc)"
fi

python3 scripts/test_seal_verdict_validate.py \
  >"$TMP/seal-verdict-validator-tests.out" \
  2>"$TMP/seal-verdict-validator-tests.err"
seal_verdict_validator_tests_rc=$?
if [[ $seal_verdict_validator_tests_rc -eq 0 ]]; then
  ok "seal verdict ledger and exact-HEAD binding suite passes"
else
  bad "seal verdict ledger and exact-HEAD binding suite failed (rc=$seal_verdict_validator_tests_rc)"
fi

INV="$TMP/inventory"
mkdir -p "$INV/scripts/lib" "$INV/examples/security" "$INV/examples/showcase" "$INV/tests/fixtures/language_core"
if [[ -f scripts/lib/native_corpus_inventory.py ]]; then
  cp scripts/lib/native_corpus_inventory.py "$INV/scripts/lib/"
  cp scripts/lib/pin_manifest.py "$INV/scripts/lib/"
  printf '%s\n' \
    '{' \
    '  "schema": "anubis.pin-manifest-policy.v2",' \
    '  "roots": ["examples", "scripts", "tests"],' \
    '  "files": [],' \
    '  "excluded_top_level_entries": {' \
    '    ".git": {"kind": "directory", "reason": "test Git metadata"},' \
    '    "outside.anb": {"kind": "file", "reason": "symlink escape target"}' \
    '  },' \
    '  "excluded_exact_directories": ["scripts/__pycache__"],' \
    '  "excluded_directory_names": [],' \
    '  "excluded_directory_names_under": {}' \
    '}' >"$INV/scripts/lib/pin_manifest_policy.json"
fi
(
  cd "$INV"
  git init -q
  git config user.email corpus-binding@example.invalid
  git config user.name corpus-binding-test
  printf 'fn main() {}\n' > examples/security/a.anb
  printf 'fn main() {}\n' > examples/showcase/b.anb
  printf 'fn main() {}\n' > tests/fixtures/language_core/c.anb
  git add .
  git commit -qm baseline
)

if [[ -f "$INV/scripts/lib/native_corpus_inventory.py" ]]; then
  clean_count="$(cd "$INV" && python3 scripts/lib/native_corpus_inventory.py --count 2>"$TMP/inventory-clean.err")"
  clean_rc=$?
  if [[ $clean_rc -eq 0 && "$clean_count" == "3" ]]; then
    ok "clean source-manifest inventory counts three files"
  else
    bad "clean manifest inventory expected count=3 rc=0 (got count=$clean_count rc=$clean_rc)"
  fi

  printf 'fn main() {}\n' > "$INV/examples/showcase/untracked_poison.anb"
  poison_count="$(cd "$INV" && python3 scripts/lib/native_corpus_inventory.py --count \
    2>"$TMP/inventory-poison.err")"
  poison_rc=$?
  if [[ $poison_rc -eq 0 && "$poison_count" == 4 ]]; then
    ok "on-disk corpus additions enter the source-manifest authority"
  else
    bad "on-disk manifest-bound corpus addition was omitted (count=$poison_count rc=$poison_rc)"
  fi

  (cd "$INV" && git add examples/showcase/untracked_poison.anb)
  staged_count="$(cd "$INV" && python3 scripts/lib/native_corpus_inventory.py --count 2>"$TMP/inventory-staged.err")"
  staged_rc=$?
  if [[ $staged_rc -eq 0 && "$staged_count" == "4" ]]; then
    ok "staged corpus poison becomes reproducible inventory"
  else
    bad "staged inventory expected count=4 rc=0 (got count=$staged_count rc=$staged_rc)"
  fi

  printf 'fn outside() {}\n' > "$INV/outside.anb"
  ln -s ../../../outside.anb "$INV/tests/fixtures/language_core/symlink_escape.anb"
  (cd "$INV" && git add tests/fixtures/language_core/symlink_escape.anb)
  (cd "$INV" && python3 scripts/lib/native_corpus_inventory.py --count >"$TMP/inventory-symlink.out" 2>"$TMP/inventory-symlink.err")
  symlink_rc=$?
  if [[ $symlink_rc -ne 0 ]] && grep -q 'regular non-symlink' "$TMP/inventory-symlink.err"; then
    ok "source-manifest corpus symlink fails closed"
  else
    bad "source-manifest corpus symlink did not fail with explicit diagnostic (rc=$symlink_rc)"
  fi
fi

PIN="$TMP/pin"
mkdir -p "$PIN/.gate_floors" "$PIN/scripts/floors" "$PIN/scripts/lib" "$PIN/target/release" \
  "$PIN/compiler/src" "$PIN/compiler/stdlib" "$PIN/solver/src" "$PIN/selfhost" "$PIN/formal" \
  "$PIN/examples/security" "$PIN/examples/showcase" "$PIN/tests/fixtures" \
  "$PIN/tools/anubis/src" "$PIN/tools/anubis/tests" "$PIN/docs" "$PIN/poc_kit"
cp scripts/publish_pin.sh "$PIN/scripts/"
cp scripts/lib/pin_manifest.py "$PIN/scripts/lib/"
cat > "$PIN/scripts/lib/pin_manifest_policy.json" <<'JSON'
{
  "schema": "anubis.pin-manifest-policy.v2",
  "roots": [
    ".gate_floors",
    "compiler",
    "docs",
    "examples",
    "formal",
    "poc_kit",
    "scripts",
    "selfhost",
    "solver",
    "tests",
    "tools",
    "vm"
  ],
  "files": [
    "Cargo.lock",
    "Cargo.toml"
  ],
  "excluded_top_level_entries": {
    ".git": {
      "kind": "file_or_directory",
      "reason": "Git administrative metadata or linked-worktree pointer"
    },
    "target": {
      "kind": "directory",
      "reason": "build output"
    }
  },
  "excluded_exact_directories": [
    "examples/security/out",
    "examples/showcase/seshat_planner/artifacts",
    "formal/.lake",
    "poc_kit/bin",
    "scripts/__pycache__",
    "scripts/lib/__pycache__",
    "vm/exports",
    "vm/pins"
  ],
  "excluded_directory_names": [],
  "excluded_directory_names_under": {}
}
JSON
(
  cd "$PIN"
  git init -q
  git config user.email corpus-binding@example.invalid
  git config user.name corpus-binding-test
  printf 'fn main() {}\n' > examples/security/a.anb
  printf 'fn main() {}\n' > examples/showcase/b.anb
  printf 'fn main() {}\n' > tests/fixtures/c.anb
  printf '#[test]\nfn evidence_binding() {}\n' > tools/anubis/tests/evidence_binding.rs
  printf '# Current claim\n' > docs/CLAIMS.md
  printf 'int main(void) { return 0; }\n' > poc_kit/vuln_local.c
  printf '#!/usr/bin/env bash\nexit 0\n' > poc_kit/build_vuln.sh
  chmod +x poc_kit/build_vuln.sh
  printf '1\n' > .gate_floors/synthetic.floor
  printf '1\n' > scripts/floors/synthetic.count_floor
  printf '[workspace]\nmembers = ["tools/anubis"]\nresolver = "2"\n' > Cargo.toml
  printf '[package]\nname="compiler"\n' > compiler/Cargo.toml
  printf '[package]\nname = "anubis"\nversion = "0.0.0"\nedition = "2021"\n' > tools/anubis/Cargo.toml
  printf 'fn main() { println!("synthetic-anubis"); }\n' > tools/anubis/src/main.rs
  printf '[package]\nname="solver"\n' > solver/Cargo.toml
  printf 'fn main() {}\n' > selfhost/stage.anb
  printf 'leanprover/lean4:nightly\n' > formal/lean-toolchain
  printf 'package Anubis\n' > formal/lakefile.toml
  printf '1\n' > examples/security/.fixture_count_floor
  printf '1\n' > docs/.docs_drift_coverage_floor
  cargo generate-lockfile -q
  git add .
  git commit -qm baseline
  printf '#!/usr/bin/env bash\nexit 0\n' > target/release/anubis
  chmod +x target/release/anubis
  touch target/release/anubis
  /bin/bash -p scripts/publish_pin.sh >"$TMP/publish.out" 2>"$TMP/publish.err"
)
publish_rc=$?
if [[ $publish_rc -eq 0 ]]; then
  ok "isolated pin publishes"
else
  bad "isolated pin failed to publish (rc=$publish_rc)"
fi

mkdir "$PIN/vm/pins/.publish.lock"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/publish-lock-a.out" 2>"$TMP/publish-lock-a.err")
publish_lock_a_rc=$?
lock_survived_a=0
[[ -d "$PIN/vm/pins/.publish.lock" ]] && lock_survived_a=1
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/publish-lock-b.out" 2>"$TMP/publish-lock-b.err")
publish_lock_b_rc=$?
lock_survived_b=0
[[ -d "$PIN/vm/pins/.publish.lock" ]] && lock_survived_b=1
rmdir "$PIN/vm/pins/.publish.lock"
if [[ $publish_lock_a_rc -ne 0 && $publish_lock_b_rc -ne 0 \
  && "$lock_survived_a" == 1 && "$lock_survived_b" == 1 ]] \
  && grep -q 'PIN_PUBLICATION_LOCKED' "$TMP/publish-lock-a.err" \
  && grep -q 'PIN_PUBLICATION_LOCKED' "$TMP/publish-lock-b.err"; then
  ok "publication losers cannot remove the winning global lock"
else
  bad "publication lock ownership was not preserved (a=$publish_lock_a_rc/$lock_survived_a b=$publish_lock_b_rc/$lock_survived_b)"
fi

ln -s "$TMP" "$TMP/root-component-link"
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$TMP/root-component-link/pin" \
  --field tree_sha256 \
  >"$TMP/intermediate-root-symlink.out" 2>"$TMP/intermediate-root-symlink.err"
intermediate_root_symlink_rc=$?
rm "$TMP/root-component-link"
if [[ $intermediate_root_symlink_rc -ne 0 ]] \
   && grep -q 'symlink intermediate path component' "$TMP/intermediate-root-symlink.err"; then
  ok "intermediate symlink in repository root path fails closed"
else
  bad "intermediate repository-root symlink was followed (rc=$intermediate_root_symlink_rc)"
fi

mv "$PIN/scripts/lib" "$PIN/scripts/lib.real"
ln -s lib.real "$PIN/scripts/lib"
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$PIN" \
  --policy scripts/lib/pin_manifest_policy.json \
  --field tree_sha256 \
  >"$TMP/intermediate-policy-symlink.out" 2>"$TMP/intermediate-policy-symlink.err"
intermediate_policy_symlink_rc=$?
rm "$PIN/scripts/lib"
mv "$PIN/scripts/lib.real" "$PIN/scripts/lib"
if [[ $intermediate_policy_symlink_rc -ne 0 ]] \
   && grep -q 'symlink intermediate path component' "$TMP/intermediate-policy-symlink.err"; then
  ok "intermediate symlink in manifest policy path fails closed"
else
  bad "intermediate manifest-policy symlink was followed (rc=$intermediate_policy_symlink_rc)"
fi

mv "$PIN/tests/fixtures" "$PIN/tests/fixtures.real"
ln -s fixtures.real "$PIN/tests/fixtures"
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$PIN" \
  --field tree_sha256 \
  >"$TMP/intermediate-source-symlink.out" 2>"$TMP/intermediate-source-symlink.err"
intermediate_source_symlink_rc=$?
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$PIN" \
  --print-rsync-excludes \
  >"$TMP/rsync-excludes-source-symlink.out" \
  2>"$TMP/rsync-excludes-source-symlink.err"
rsync_excludes_source_symlink_rc=$?
rm "$PIN/tests/fixtures"
mv "$PIN/tests/fixtures.real" "$PIN/tests/fixtures"
if [[ $intermediate_source_symlink_rc -ne 0 ]] \
   && grep -Eq 'symlink (directory in source trust universe|intermediate path component)' \
      "$TMP/intermediate-source-symlink.err"; then
  ok "intermediate symlink in a source file path fails closed"
else
  bad "intermediate source-file symlink was followed (rc=$intermediate_source_symlink_rc)"
fi
if [[ $rsync_excludes_source_symlink_rc -ne 0 ]] \
   && grep -q 'symlink directory in source trust universe' \
      "$TMP/rsync-excludes-source-symlink.err"; then
  ok "rsync exclusion export validates the complete source universe first"
else
  bad "rsync exclusion export bypassed source validation (rc=$rsync_excludes_source_symlink_rc)"
fi

printf 'unknown top-level source\n' > "$PIN/unknown-top-level-file.bin"
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$PIN" \
  --field tree_sha256 \
  >"$TMP/unknown-top-level-file.out" 2>"$TMP/unknown-top-level-file.err"
unknown_top_level_file_rc=$?
rm "$PIN/unknown-top-level-file.bin"
if [[ $unknown_top_level_file_rc -ne 0 ]] \
   && grep -q 'unclassified top-level repository entry: unknown-top-level-file.bin' \
      "$TMP/unknown-top-level-file.err"; then
  ok "unknown top-level file fails closed"
else
  bad "unknown top-level file escaped policy coverage (rc=$unknown_top_level_file_rc)"
fi

mkdir "$PIN/unknown-top-level-directory"
python3 "$ROOT/scripts/lib/pin_manifest.py" \
  --root "$PIN" \
  --field tree_sha256 \
  >"$TMP/unknown-top-level-directory.out" 2>"$TMP/unknown-top-level-directory.err"
unknown_top_level_directory_rc=$?
rmdir "$PIN/unknown-top-level-directory"
if [[ $unknown_top_level_directory_rc -ne 0 ]] \
   && grep -q 'unclassified top-level repository entry: unknown-top-level-directory' \
      "$TMP/unknown-top-level-directory.err"; then
  ok "unknown top-level directory fails closed"
else
  bad "unknown top-level directory escaped policy coverage (rc=$unknown_top_level_directory_rc)"
fi

cp "$PIN/scripts/lib/pin_manifest_policy.json" "$TMP/nested-root-policy.backup"
python3 - "$PIN/scripts/lib/pin_manifest_policy.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    policy = json.load(handle)
policy["roots"] = ["vm/kernels" if root == "vm" else root for root in policy["roots"]]
with open(path, "w", encoding="utf-8") as handle:
    json.dump(policy, handle, indent=2)
    handle.write("\n")
PY
python3 "$ROOT/scripts/lib/pin_manifest.py" --root "$PIN" --field tree_sha256 \
  >"$TMP/nested-root-policy.out" 2>"$TMP/nested-root-policy.err"
nested_root_policy_rc=$?
mv "$TMP/nested-root-policy.backup" "$PIN/scripts/lib/pin_manifest_policy.json"
if [[ $nested_root_policy_rc -ne 0 ]] \
  && grep -q 'roots must be complete top-level directories: vm/kernels' \
    "$TMP/nested-root-policy.err"; then
  ok "nested policy roots cannot launder unbound top-level siblings"
else
  bad "nested policy root escaped the complete-root model (rc=$nested_root_policy_rc)"
fi

python3 - "$ROOT/scripts/lib/pin_manifest.py" "$PIN/tests/fixtures/c.anb" <<'PY'
import importlib.util
import os
import shutil
import sys
from pathlib import Path

module_path = Path(sys.argv[1])
victim = Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("pin_manifest_under_test", module_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = module
spec.loader.exec_module(module)

original = victim.with_name(victim.name + ".opened")
real_fstat = module.os.fstat
calls = 0

def racing_fstat(fd):
    global calls
    calls += 1
    result = real_fstat(fd)
    if calls == 2:
        before = victim.stat()
        victim.rename(original)
        shutil.copyfile(original, victim)
        os.chmod(victim, before.st_mode)
        os.utime(victim, ns=(before.st_atime_ns, before.st_mtime_ns))
    return result

module.os.fstat = racing_fstat
try:
    module.file_receipt(victim)
except SystemExit:
    outcome = 0
else:
    outcome = 1
finally:
    victim.unlink(missing_ok=True)
    original.rename(victim)
raise SystemExit(outcome)
PY
manifest_path_replacement_rc=$?
if [[ $manifest_path_replacement_rc -eq 0 ]]; then
  ok "manifest file receipt rejects closing pathname replacement"
else
  bad "manifest file receipt accepted a replaced closing pathname"
fi

for manifest_race in content late_path; do
  python3 - "$ROOT/scripts/lib/pin_manifest.py" "$PIN" "$manifest_race" <<'PY'
import importlib.util
import sys
from pathlib import Path

module_path = Path(sys.argv[1])
root = Path(sys.argv[2])
mode = sys.argv[3]
spec = importlib.util.spec_from_file_location("pin_manifest_race_test", module_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = module
spec.loader.exec_module(module)

victim = root / "tests/fixtures/c.anb"
late = root / "tests/fixtures/LATE_MANIFEST_RACE.anb"
original = victim.read_bytes()
real_build = module.build_manifest
calls = 0

def racing_build(*args, **kwargs):
    global calls
    calls += 1
    result = real_build(*args, **kwargs)
    if calls == 2:
        if mode == "content":
            victim.write_bytes(b"fn changed_after_second_pass() {}\n")
        else:
            late.write_bytes(b"fn late_after_second_pass() {}\n")
    return result

module.build_manifest = racing_build
try:
    module.stable_manifest(root)
except SystemExit:
    outcome = 0
else:
    outcome = 1
finally:
    victim.write_bytes(original)
    late.unlink(missing_ok=True)
raise SystemExit(outcome)
PY
  manifest_race_rc=$?
  if [[ $manifest_race_rc -eq 0 ]]; then
    ok "manifest closing pass rejects ${manifest_race} mutation"
  else
    bad "manifest closing pass accepted ${manifest_race} mutation"
  fi
done

isolated_pin_rel="$(tr -d '\n' < "$PIN/vm/pins/CURRENT")"
isolated_meta="$PIN/$isolated_pin_rel.meta"
if [[ "$(grep -Fc 'manifest_schema: anubis.pin-source-manifest.v2' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Fc 'pin_schema: anubis.binary-pin.v2' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Ec '^head: [0-9a-f]{40}$' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Ec '^head_tree: ([0-9a-f]{40}|[0-9a-f]{64})$' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Fc 'commit_bound: false' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Fc 'build_mode: technical-existing-target' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Ec '^policy_sha256: [0-9a-f]{64}$' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Ec '^src_count: [1-9][0-9]*$' "$isolated_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Ec '^src_list_sha256: [0-9a-f]{64}$' "$isolated_meta" 2>/dev/null || true)" == "1" ]]; then
  ok "technical pin metadata records immutable schema, full Git identity, and source manifest"
else
  bad "technical pin metadata omits immutable schema, full Git identity, or source manifest"
fi

technical_current="$isolated_pin_rel"
technical_meta_sha_before="$(shasum -a 256 "$isolated_meta" | awk '{print $1}')"
if grep -Fq 'release_cargo_home="$scratch/release-cargo-home"' \
     "$PIN/scripts/publish_pin.sh" \
   && grep -Fq 'mkdir "$scratch/build-target" "$release_cargo_home"' \
     "$PIN/scripts/publish_pin.sh" \
   && grep -Fq 'CARGO_HOME="$release_cargo_home"' "$PIN/scripts/publish_pin.sh" \
   && ! grep -Fq 'CARGO_HOME="$CANONICAL_CARGO_HOME"' "$PIN/scripts/publish_pin.sh"; then
  ok "release build excludes unbound canonical Cargo configuration"
else
  bad "release build can load unbound canonical Cargo configuration"
fi
parent_config_guard_count="$(grep -Fc 'assert_no_parent_cargo_configuration "$release_source" || exit 1' \
  "$PIN/scripts/publish_pin.sh")"
parent_config_guard_first="$(grep -nF 'assert_no_parent_cargo_configuration "$release_source" || exit 1' \
  "$PIN/scripts/publish_pin.sh" | head -1 | cut -d: -f1)"
parent_config_guard_last="$(grep -nF 'assert_no_parent_cargo_configuration "$release_source" || exit 1' \
  "$PIN/scripts/publish_pin.sh" | tail -1 | cut -d: -f1)"
release_cargo_call_line="$(grep -nF '"$CARGO_BIN" build --locked --release -p anubis' \
  "$PIN/scripts/publish_pin.sh" | cut -d: -f1)"
release_weakening_guard_line="$(grep -nF \
  'for weakening_var in ANUBIS_SKIP_RISC0_METAL RISC0_SKIP_BUILD_KERNELS R0_DISABLE_METAL; do' \
  "$PIN/scripts/publish_pin.sh" | cut -d: -f1)"
if [[ "$parent_config_guard_count" == "2" \
   && "$parent_config_guard_first" -lt "$release_cargo_call_line" \
   && "$release_cargo_call_line" -lt "$parent_config_guard_last" ]]; then
  ok "release build rejects unbound ancestor Cargo configuration before and after Cargo"
else
  bad "release build does not bound Cargo's ancestor configuration walk"
fi
if [[ -n "$release_weakening_guard_line" \
   && "$release_weakening_guard_line" -lt "$release_cargo_call_line" ]]; then
  ok "release-mode weakening controls are denied before Cargo"
else
  bad "release-mode weakening controls are not denied before Cargo"
fi
raw_python_bin_uses="$(grep -Fc '"$PYTHON_BIN"' "$PIN/scripts/publish_pin.sh")"
if [[ "$raw_python_bin_uses" == "2" ]] \
   && grep -Fq '"$PYTHON_BIN" -I -B "$@"' "$PIN/scripts/publish_pin.sh"; then
  ok "all pin-provenance Python calls route through isolated no-bytecode mode"
else
  bad "a pin-provenance Python call bypasses isolated no-bytecode mode"
fi

orphan_pin="$PIN/$isolated_pin_rel"
cp "$orphan_pin" "$TMP/orphan-pin.backup"
cp "$isolated_meta" "$TMP/orphan-meta.backup"
rm "$isolated_meta"
chmod 0500 "$orphan_pin"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/orphan-mode.out" 2>"$TMP/orphan-mode.err")
orphan_mode_rc=$?
if [[ $orphan_mode_rc -ne 0 && ! -e "$isolated_meta" ]] \
   && grep -q 'PIN_COLLISION: malformed or conflicting binary-only orphan' \
      "$TMP/orphan-mode.err"; then
  ok "publication rejects an incorrectly-moded binary-only orphan"
else
  bad "publication recovered an incorrectly-moded binary-only orphan (rc=$orphan_mode_rc)"
fi

chmod 0755 "$orphan_pin"
printf '# conflicting orphan bytes\n' >> "$orphan_pin"
chmod 0555 "$orphan_pin"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/orphan-bytes.out" 2>"$TMP/orphan-bytes.err")
orphan_bytes_rc=$?
if [[ $orphan_bytes_rc -ne 0 && ! -e "$isolated_meta" ]] \
   && grep -q 'PIN_COLLISION: malformed or conflicting binary-only orphan' \
      "$TMP/orphan-bytes.err"; then
  ok "publication rejects a conflicting binary-only orphan"
else
  bad "publication recovered a conflicting binary-only orphan (rc=$orphan_bytes_rc)"
fi

chmod 0755 "$orphan_pin"
cp "$TMP/orphan-pin.backup" "$orphan_pin"
chmod 0555 "$orphan_pin"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/orphan-recover.out" 2>"$TMP/orphan-recover.err")
orphan_recover_rc=$?
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify \
  >"$TMP/orphan-recover-verify.out" 2>"$TMP/orphan-recover-verify.err")
orphan_recover_verify_rc=$?
if [[ $orphan_recover_rc -eq 0 && $orphan_recover_verify_rc -eq 0 \
   && -f "$isolated_meta" ]] \
   && cmp -s "$isolated_meta" "$TMP/orphan-meta.backup" \
   && grep -q 'PIN_ORPHAN_RECOVERED: completed exact binary/metadata pair' \
      "$TMP/orphan-recover.err"; then
  ok "publication retry recovers an exact stable binary-only orphan"
else
  bad "publication retry did not recover an exact binary-only orphan (publish=$orphan_recover_rc verify=$orphan_recover_verify_rc)"
fi

rm "$orphan_pin"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/orphan-metadata-only.out" 2>"$TMP/orphan-metadata-only.err")
orphan_metadata_only_rc=$?
if [[ $orphan_metadata_only_rc -ne 0 ]] \
   && grep -q 'PIN_COLLISION: metadata-only orphan cannot be recovered' \
      "$TMP/orphan-metadata-only.err"; then
  ok "publication rejects a metadata-only orphan"
else
  bad "publication recovered a metadata-only orphan (rc=$orphan_metadata_only_rc)"
fi
cp "$TMP/orphan-pin.backup" "$orphan_pin"
chmod 0555 "$orphan_pin"
(
  cd "$PIN"
  git add -f vm/pins/CURRENT
  git commit -qm 'bind tracked CURRENT fixture before release publication'
)
touch "$PIN/target/release/anubis"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --release >"$TMP/release-publish.out" 2>"$TMP/release-publish.err")
release_publish_rc=$?
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify-release >"$TMP/release-verify.out" 2>"$TMP/release-verify.err")
release_verify_rc=$?
release_pin_rel="$(tr -d '\n' < "$PIN/vm/pins/CURRENT")"
release_meta="$PIN/$release_pin_rel.meta"
if [[ $release_publish_rc -eq 0 && $release_verify_rc -eq 0 \
   && "$release_pin_rel" =~ ^vm/pins/anubis-[0-9a-f]{12}-src-[0-9a-f]{12}-release$ \
   && "$(grep -Fc 'commit_bound: true' "$release_meta" 2>/dev/null || true)" == "1" \
   && "$(grep -Fc 'build_mode: cargo-build-locked-release-exact-head-archive-clean-target' "$release_meta" 2>/dev/null || true)" == "1" ]]; then
  ok "release mode publishes and verifies an exact clean commit-bound identity"
else
  bad "release mode did not produce a verified clean commit-bound identity (publish=$release_publish_rc verify=$release_verify_rc)"
fi

for release_weakening_var in \
  ANUBIS_SKIP_RISC0_METAL RISC0_SKIP_BUILD_KERNELS R0_DISABLE_METAL; do
  release_weakening_current_before="$(shasum -a 256 "$PIN/vm/pins/CURRENT" | awk '{print $1}')"
  (
    cd "$PIN"
    /usr/bin/env "$release_weakening_var=1" \
      /bin/bash -p scripts/publish_pin.sh --release \
      >"$TMP/release-$release_weakening_var.out" \
      2>"$TMP/release-$release_weakening_var.err"
  )
  release_weakening_rc=$?
  release_weakening_current_after="$(shasum -a 256 "$PIN/vm/pins/CURRENT" | awk '{print $1}')"
  if [[ "$release_weakening_rc" -ne 0 \
     && "$release_weakening_current_before" == "$release_weakening_current_after" ]] \
     && grep -Fq \
       "PIN_RELEASE_BUILD_ENV_DENIED: $release_weakening_var must be unset or 0" \
       "$TMP/release-$release_weakening_var.err"; then
    ok "release mode rejects caller $release_weakening_var before CURRENT mutation"
  else
    bad "release mode did not fail closed on caller $release_weakening_var (rc=$release_weakening_rc current_unchanged=$([[ \"$release_weakening_current_before\" == \"$release_weakening_current_after\" ]] && echo yes || echo no))"
  fi
done

(cd "$PIN" && RUSTC_WRAPPER=/usr/bin/false /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-rustc-wrapper.out" 2>"$TMP/release-rustc-wrapper.err")
release_rustc_wrapper_rc=$?
if [[ $release_rustc_wrapper_rc -ne 0 ]] \
   && grep -q 'PIN_RELEASE_BUILD_ENV_DENIED: RUSTC_WRAPPER' \
      "$TMP/release-rustc-wrapper.err"; then
  ok "release mode rejects caller RUSTC_WRAPPER"
else
  bad "release mode did not explicitly reject caller RUSTC_WRAPPER (rc=$release_rustc_wrapper_rc)"
fi

(cd "$PIN" && RUSTFLAGS='--cfg anubis_release_env_poison' \
  /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-rustflags.out" 2>"$TMP/release-rustflags.err")
release_rustflags_rc=$?
if [[ $release_rustflags_rc -ne 0 ]] \
   && grep -q 'PIN_RELEASE_BUILD_ENV_DENIED: RUSTFLAGS' "$TMP/release-rustflags.err"; then
  ok "release mode rejects caller RUSTFLAGS"
else
  bad "release mode did not explicitly reject caller RUSTFLAGS (rc=$release_rustflags_rc)"
fi

(cd "$PIN" && CARGO_BUILD_RUSTC=/usr/bin/false \
  /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-cargo-build.out" 2>"$TMP/release-cargo-build.err")
release_cargo_build_rc=$?
if [[ $release_cargo_build_rc -ne 0 ]] \
   && grep -q 'PIN_RELEASE_BUILD_ENV_DENIED: CARGO_BUILD_RUSTC' \
      "$TMP/release-cargo-build.err"; then
  ok "release mode rejects caller CARGO_BUILD overrides"
else
  bad "release mode did not explicitly reject caller CARGO_BUILD override (rc=$release_cargo_build_rc)"
fi

(cd "$PIN" && CARGO_PROFILE_RELEASE_OPT_LEVEL=0 \
  /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-cargo-profile.out" 2>"$TMP/release-cargo-profile.err")
release_cargo_profile_rc=$?
if [[ $release_cargo_profile_rc -ne 0 ]] \
   && grep -q 'PIN_RELEASE_BUILD_ENV_DENIED: CARGO_PROFILE_RELEASE_OPT_LEVEL' \
      "$TMP/release-cargo-profile.err"; then
  ok "release mode rejects caller CARGO_PROFILE overrides"
else
  bad "release mode did not explicitly reject caller CARGO_PROFILE override (rc=$release_cargo_profile_rc)"
fi

(cd "$PIN" && CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS='--cfg anubis_target_poison' \
  /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-cargo-target.out" 2>"$TMP/release-cargo-target.err")
release_cargo_target_rc=$?
if [[ $release_cargo_target_rc -ne 0 ]] \
   && grep -q 'PIN_RELEASE_BUILD_ENV_DENIED: CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS' \
      "$TMP/release-cargo-target.err"; then
  ok "release mode rejects caller CARGO_TARGET overrides"
else
  bad "release mode did not explicitly reject caller CARGO_TARGET override (rc=$release_cargo_target_rc)"
fi

spoof_home="$TMP/release-spoof-home"
spoof_home_marker="$TMP/release-spoof-home-wrapper.marker"
mkdir -p "$spoof_home/.cargo"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  "printf poison >'$spoof_home_marker'" \
  'exit 91' \
  > "$spoof_home/.cargo/poison-rustc-wrapper"
chmod +x "$spoof_home/.cargo/poison-rustc-wrapper"
printf '[build]\nrustc-wrapper = "%s"\n' "$spoof_home/.cargo/poison-rustc-wrapper" \
  > "$spoof_home/.cargo/config.toml"
/usr/bin/env -u CARGO_HOME HOME="$spoof_home" RUSTUP_HOME="$HOME/.rustup" \
  /bin/bash -c 'cd "$1" && /bin/bash -p scripts/publish_pin.sh --release' _ "$PIN" \
  >"$TMP/release-spoof-home.out" 2>"$TMP/release-spoof-home.err"
release_spoof_home_rc=$?
if [[ $release_spoof_home_rc -eq 0 && ! -e "$spoof_home_marker" ]]; then
  ok "release mode ignores caller HOME Cargo configuration"
else
  bad "release mode trusted caller HOME Cargo configuration (rc=$release_spoof_home_rc marker=$([[ -e "$spoof_home_marker" ]] && echo present || echo absent))"
fi

rm -f "$spoof_home_marker"
HOME="$HOME" CARGO_HOME="$spoof_home/.cargo" RUSTUP_HOME="$HOME/.rustup" \
  /bin/bash -c 'cd "$1" && /bin/bash -p scripts/publish_pin.sh --release' _ "$PIN" \
  >"$TMP/release-spoof-cargo-home.out" 2>"$TMP/release-spoof-cargo-home.err"
release_spoof_cargo_home_rc=$?
if [[ $release_spoof_cargo_home_rc -eq 0 && ! -e "$spoof_home_marker" ]]; then
  ok "release mode ignores caller CARGO_HOME configuration"
else
  bad "release mode trusted caller CARGO_HOME configuration (rc=$release_spoof_cargo_home_rc marker=$([[ -e "$spoof_home_marker" ]] && echo present || echo absent))"
fi

python_poison_dir="$TMP/release-python-poison"
python_poison_marker="$TMP/release-python-sitecustomize.marker"
mkdir "$python_poison_dir"
printf '%s\n' \
  'import os' \
  'with open(os.environ["ANUBIS_PYTHON_MARKER"], "w", encoding="utf-8") as handle:' \
  '    handle.write("poison")' \
  > "$python_poison_dir/sitecustomize.py"
(cd "$PIN" && PYTHONPATH="$python_poison_dir" \
  ANUBIS_PYTHON_MARKER="$python_poison_marker" \
  /bin/bash -p scripts/publish_pin.sh --verify-release \
  >"$TMP/release-python-poison.out" 2>"$TMP/release-python-poison.err")
release_python_poison_rc=$?
if [[ $release_python_poison_rc -eq 0 && ! -e "$python_poison_marker" \
   && ! -e "$python_poison_dir/__pycache__" ]]; then
  ok "pin publication Python ignores caller PYTHONPATH and sitecustomize"
else
  bad "pin publication Python loaded caller sitecustomize (rc=$release_python_poison_rc marker=$([[ -e "$python_poison_marker" ]] && echo present || echo absent))"
fi

bash_env_poison="$TMP/release-bash-env-poison.sh"
bash_env_marker="$TMP/release-bash-env.marker"
bash_function_marker="$TMP/release-bash-function.marker"
printf '%s\n' \
  '/bin/echo poison >"$ANUBIS_BASH_ENV_MARKER"' \
  'mkdir() { /bin/echo function >"$ANUBIS_BASH_FUNCTION_MARKER"; return 97; }' \
  'export -f mkdir' \
  > "$bash_env_poison"
(
  mkdir() { /bin/echo function >"$ANUBIS_BASH_FUNCTION_MARKER"; return 97; }
  export -f mkdir
  cd "$PIN"
  BASH_ENV="$bash_env_poison" \
    ANUBIS_BASH_ENV_MARKER="$bash_env_marker" \
    ANUBIS_BASH_FUNCTION_MARKER="$bash_function_marker" \
    ./scripts/publish_pin.sh --verify-release \
    >"$TMP/release-bash-env-direct.out" 2>"$TMP/release-bash-env-direct.err"
)
release_bash_env_direct_rc=$?
if [[ $release_bash_env_direct_rc -eq 0 && ! -e "$bash_env_marker" \
   && ! -e "$bash_function_marker" ]]; then
  ok "direct pin-script launch ignores BASH_ENV and exported function overrides"
else
  bad "direct pin-script launch executed caller shell startup state (rc=$release_bash_env_direct_rc)"
fi

(
  mkdir() { /bin/echo function >"$ANUBIS_BASH_FUNCTION_MARKER"; return 97; }
  export -f mkdir
  cd "$PIN"
  ANUBIS_BASH_FUNCTION_MARKER="$bash_function_marker" \
    /usr/bin/env -u BASH_ENV /bin/bash scripts/publish_pin.sh --verify-release \
    >"$TMP/release-bash-reexec.out" 2>"$TMP/release-bash-reexec.err"
)
release_bash_reexec_rc=$?
if [[ $release_bash_reexec_rc -eq 0 && ! -e "$bash_function_marker" ]]; then
  ok "legacy clean Bash launch self-reexecutes without imported functions"
else
  bad "legacy clean Bash launch retained an imported function (rc=$release_bash_reexec_rc)"
fi

cp "$PIN/vm/pins/CURRENT" "$TMP/unprivileged-publish-current.before"
(cd "$PIN" && /usr/bin/env -u BASH_ENV /bin/bash scripts/publish_pin.sh \
  >"$TMP/unprivileged-publish.out" 2>"$TMP/unprivileged-publish.err")
unprivileged_publish_rc=$?
if [[ $unprivileged_publish_rc -ne 0 ]] \
   && cmp -s "$PIN/vm/pins/CURRENT" "$TMP/unprivileged-publish-current.before" \
   && grep -q 'PIN_SHELL_UNTRUSTED: publication requires direct execution or /bin/bash -p' \
      "$TMP/unprivileged-publish.err"; then
  ok "unprivileged Bash cannot enter pin publication"
else
  bad "unprivileged Bash entered pin publication (rc=$unprivileged_publish_rc)"
fi

cp "$PIN/vm/pins/CURRENT" "$TMP/bash-env-current.before"
(cd "$PIN" && BASH_ENV="$bash_env_poison" \
  ANUBIS_BASH_ENV_MARKER="$bash_env_marker" \
  ANUBIS_BASH_FUNCTION_MARKER="$bash_function_marker" \
  /bin/bash scripts/publish_pin.sh --verify-release \
  >"$TMP/release-bash-env-explicit.out" 2>"$TMP/release-bash-env-explicit.err")
release_bash_env_explicit_rc=$?
if [[ $release_bash_env_explicit_rc -ne 0 && -e "$bash_env_marker" \
   && ! -e "$bash_function_marker" ]] \
   && cmp -s "$PIN/vm/pins/CURRENT" "$TMP/bash-env-current.before" \
   && grep -q 'PIN_SHELL_UNTRUSTED: BASH_ENV requires direct execution or /bin/bash -p' \
      "$TMP/release-bash-env-explicit.err"; then
  ok "unprivileged BASH_ENV launch is refused before pin logic"
else
  bad "unprivileged BASH_ENV launch reached pin logic (rc=$release_bash_env_explicit_rc)"
fi

mkdir -p "$TMP/publish-path-shims"
printf '%s\n' '#!/usr/bin/env bash' 'printf git >"$ANUBIS_SHIM_MARKER"' 'exit 97' \
  >"$TMP/publish-path-shims/git"
printf '%s\n' '#!/usr/bin/env bash' 'printf tar >"$ANUBIS_SHIM_MARKER"' 'exit 98' \
  >"$TMP/publish-path-shims/tar"
printf '%s\n' '#!/usr/bin/env bash' 'printf dirname >"$ANUBIS_SHIM_MARKER"' 'exit 99' \
  >"$TMP/publish-path-shims/dirname"
chmod +x "$TMP/publish-path-shims/git" "$TMP/publish-path-shims/tar" \
  "$TMP/publish-path-shims/dirname"
rm -f "$TMP/publish-tool-shim.marker"
(cd "$PIN" && PATH="$TMP/publish-path-shims:$PATH" \
  ANUBIS_SHIM_MARKER="$TMP/publish-tool-shim.marker" \
  /bin/bash -p scripts/publish_pin.sh --verify-release \
  >"$TMP/publish-tool-shim.out" 2>"$TMP/publish-tool-shim.err")
publish_tool_shim_rc=$?
if [[ $publish_tool_shim_rc -eq 0 && ! -e "$TMP/publish-tool-shim.marker" ]]; then
  ok "release verification ignores caller PATH git, tar, and dirname shims"
else
  bad "release verification trusted a caller PATH shim (rc=$publish_tool_shim_rc)"
fi

replace_target="$(cd "$PIN" && git rev-parse HEAD^)"
(cd "$PIN" && git replace HEAD "$replace_target")
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify-release \
  >"$TMP/publish-replace-ref.out" 2>"$TMP/publish-replace-ref.err")
publish_replace_ref_rc=$?
(cd "$PIN" && git replace -d HEAD >/dev/null)
if [[ $publish_replace_ref_rc -eq 0 ]]; then
  ok "release verification ignores repository-local replace refs"
else
  bad "release verification honored a repository-local replace ref (rc=$publish_replace_ref_rc)"
fi

printf '# dirty release probe\n' >> "$PIN/docs/CLAIMS.md"
(mkdir -p "$TMP/redirected-git" && cd "$TMP/redirected-git" && git init -q)
(cd "$PIN" && GIT_DIR="$TMP/redirected-git/.git" GIT_WORK_TREE="$TMP/redirected-git" \
  GIT_INDEX_FILE="$TMP/redirected-git/.git/index" \
  /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-redirected-git.out" 2>"$TMP/release-redirected-git.err")
release_redirected_git_rc=$?
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-dirty.out" 2>"$TMP/release-dirty.err")
release_dirty_rc=$?
printf '# Current claim\n' > "$PIN/docs/CLAIMS.md"
if [[ $release_dirty_rc -ne 0 && $release_redirected_git_rc -ne 0 ]] \
  && grep -q 'PIN_RELEASE_DIRTY' "$TMP/release-dirty.err" \
  && grep -q 'PIN_RELEASE_DIRTY' "$TMP/release-redirected-git.err"; then
  ok "release mode rejects tracked drift despite redirected Git environment"
else
  bad "release mode accepted tracked drift (plain=$release_dirty_rc redirected=$release_redirected_git_rc)"
fi

printf 'ignored eligible input\n' > "$PIN/examples/security/ignored_release_poison.txt"
printf 'examples/security/ignored_release_poison.txt\n' >> "$PIN/.git/info/exclude"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --release \
  >"$TMP/release-ignored.out" 2>"$TMP/release-ignored.err")
release_ignored_rc=$?
rm "$PIN/examples/security/ignored_release_poison.txt"
if [[ $release_ignored_rc -ne 0 ]] && grep -q 'PIN_RELEASE_UNBOUND' "$TMP/release-ignored.err"; then
  ok "release mode rejects ignored eligible files absent from the commit"
else
  bad "release mode accepted an ignored eligible file outside the commit (rc=$release_ignored_rc)"
fi

printf '# rebound source epoch\n' >> "$PIN/docs/CLAIMS.md"
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh \
  >"$TMP/rebind-publish.out" 2>"$TMP/rebind-publish.err")
rebind_publish_rc=$?
rebound_pin_rel="$(tr -d '\n' < "$PIN/vm/pins/CURRENT")"
printf '# Current claim\n' > "$PIN/docs/CLAIMS.md"
if [[ $rebind_publish_rc -eq 0 && "$rebound_pin_rel" != "$technical_current" \
   && "$(shasum -a 256 "$isolated_meta" | awk '{print $1}')" == "$technical_meta_sha_before" ]]; then
  ok "a new source epoch gets a new pin identity without rebinding old metadata"
else
  bad "source-epoch publication rebound an immutable pin (rc=$rebind_publish_rc)"
fi
printf '%s\n' "$technical_current" > "$PIN/vm/pins/CURRENT"
touch "$PIN/target/release/anubis"

mkdir -p "$PIN/examples/security/out" "$PIN/formal/.lake/build/ir"
printf '{"generated":true}\n' > "$PIN/examples/security/out/check-summary.json"
printf 'int generated(void) { return 0; }\n' > "$PIN/formal/.lake/build/ir/Generated.c"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/ignored-products.out" 2>"$TMP/ignored-products.err")
ignored_products_rc=$?
if [[ $ignored_products_rc -eq 0 ]] && grep -q '^pin matches tree:' "$TMP/ignored-products.out"; then
  ok "policy-excluded build products do not perturb the source manifest"
else
  bad "policy-excluded build products changed or broke the source manifest (rc=$ignored_products_rc)"
fi

(cd "$PIN" && python3 scripts/lib/pin_manifest.py \
  --root "$PIN" \
  --newer-than target/release/anubis \
  >"$TMP/excluded-stale.out" 2>"$TMP/excluded-stale.err")
excluded_stale_rc=$?
if [[ $excluded_stale_rc -eq 0 ]]; then
  ok "newer excluded build products do not trigger stale-source refusal"
else
  bad "excluded build product entered staleness check (rc=$excluded_stale_rc)"
fi

mv "$PIN/.git" "$TMP/pin.git"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/no-git.out" 2>"$TMP/no-git.err")
no_git_rc=$?
mv "$TMP/pin.git" "$PIN/.git"
if [[ $no_git_rc -eq 0 ]] && grep -q '^pin matches tree:' "$TMP/no-git.out"; then
  ok "pin verification is independent of Git metadata"
else
  bad "pin verification required Git metadata (rc=$no_git_rc)"
fi

printf 'ignored-by-local-git\n' > "$PIN/examples/security/local_ignore_poison.txt"
printf 'examples/security/local_ignore_poison.txt\n' >> "$PIN/.git/info/exclude"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/local-ignore.out" 2>"$TMP/local-ignore.err")
local_ignore_rc=$?
if [[ $local_ignore_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/local-ignore.err"; then
  ok "repository-local Git ignore state cannot hide a manifest source"
else
  bad "repository-local Git ignore state changed manifest authority (rc=$local_ignore_rc)"
fi
rm "$PIN/examples/security/local_ignore_poison.txt"

mkdir -p "$PIN/examples/security/output"
printf '{"near_miss":true}\n' > "$PIN/examples/security/output/report.json"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/output-near-miss.out" 2>"$TMP/output-near-miss.err")
output_near_miss_rc=$?
if [[ $output_near_miss_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/output-near-miss.err"; then
  ok "near-miss output directory remains source-bound"
else
  bad "near-miss output directory was over-excluded (rc=$output_near_miss_rc)"
fi
rm -rf "$PIN/examples/security/output"

printf '{"source":true}\n' > "$PIN/formal/source.json"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/formal-near-miss.out" 2>"$TMP/formal-near-miss.err")
formal_near_miss_rc=$?
if [[ $formal_near_miss_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/formal-near-miss.err"; then
  ok "formal near-miss source remains source-bound"
else
  bad "formal near-miss source was over-excluded (rc=$formal_near_miss_rc)"
fi
rm "$PIN/formal/source.json"

mkdir -p "$PIN/examples/new/out"
printf 'unlisted generated-looking input\n' > "$PIN/examples/new/out/source.bin"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/unlisted-out.out" 2>"$TMP/unlisted-out.err")
unlisted_out_rc=$?
if [[ $unlisted_out_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/unlisted-out.err"; then
  ok "unlisted example out directory remains source-bound"
else
  bad "unlisted example out directory was over-excluded (rc=$unlisted_out_rc)"
fi
rm -rf "$PIN/examples/new"

printf 'extensionless\n' > "$PIN/tests/fixtures/extensionless"
printf '\x00\x01binary\n' > "$PIN/tests/fixtures/payload.bin"
printf 'export default 1;\n' > "$PIN/tests/fixtures/module.mjs"
printf 'hidden\n' > "$PIN/tests/fixtures/.hidden-input"
chmod +x "$PIN/tests/fixtures/extensionless"
python3 "$PIN/scripts/lib/pin_manifest.py" --root "$PIN" --field json > "$TMP/all-files.json"
all_files_manifest_rc=$?
python3 - "$TMP/all-files.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as handle:
    rows = {row["path"]: row for row in json.load(handle)["rows"]}
required = {
    "tests/fixtures/extensionless",
    "tests/fixtures/payload.bin",
    "tests/fixtures/module.mjs",
    "tests/fixtures/.hidden-input",
}
raise SystemExit(0 if required <= rows.keys() and rows["tests/fixtures/extensionless"]["executable"] is True else 1)
PY
all_files_rows_rc=$?
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/all-files-verify.out" 2>"$TMP/all-files-verify.err")
all_files_verify_rc=$?
if [[ $all_files_manifest_rc -eq 0 && $all_files_rows_rc -eq 0 \
   && $all_files_verify_rc -ne 0 ]] \
   && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/all-files-verify.err"; then
  ok "extensionless, binary, module, hidden, and executable inputs are source-bound"
else
  bad "all-regular-file binding regression (manifest=$all_files_manifest_rc rows=$all_files_rows_rc verify=$all_files_verify_rc)"
fi
rm "$PIN/tests/fixtures/extensionless" "$PIN/tests/fixtures/payload.bin" \
  "$PIN/tests/fixtures/module.mjs" "$PIN/tests/fixtures/.hidden-input"

chmod -x "$PIN/poc_kit/build_vuln.sh"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/mode-poison.out" 2>"$TMP/mode-poison.err")
mode_poison_rc=$?
if [[ $mode_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/mode-poison.err"; then
  ok "executable-mode mutation invalidates the pin"
else
  bad "executable-mode mutation did not invalidate the pin (rc=$mode_poison_rc)"
fi
chmod +x "$PIN/poc_kit/build_vuln.sh"

ln -s a.anb "$PIN/examples/security/nonexcluded-link.anb"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/nonexcluded-symlink.out" 2>"$TMP/nonexcluded-symlink.err")
nonexcluded_symlink_rc=$?
if [[ $nonexcluded_symlink_rc -ne 0 ]] && grep -q 'regular non-symlink file' "$TMP/nonexcluded-symlink.err"; then
  ok "nonexcluded source symlink fails closed"
else
  bad "nonexcluded source symlink did not fail closed (rc=$nonexcluded_symlink_rc)"
fi
rm "$PIN/examples/security/nonexcluded-link.anb"

rm -rf "$PIN/formal/.lake"
mkdir -p "$TMP/excluded-symlink-target"
ln -s "$TMP/excluded-symlink-target" "$PIN/formal/.lake"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/excluded-symlink.out" 2>"$TMP/excluded-symlink.err")
excluded_symlink_rc=$?
if [[ $excluded_symlink_rc -ne 0 ]] && grep -q 'exact excluded directory must be a real directory' "$TMP/excluded-symlink.err"; then
  ok "excluded directory names do not authorize symlink traversal"
else
  bad "excluded symlink directory did not fail closed (rc=$excluded_symlink_rc)"
fi
rm "$PIN/formal/.lake"
printf 'not a directory\n' > "$PIN/formal/.lake"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/excluded-file.out" 2>"$TMP/excluded-file.err")
excluded_file_rc=$?
if [[ $excluded_file_rc -ne 0 ]] && grep -q 'exact excluded directory must be a real directory' "$TMP/excluded-file.err"; then
  ok "excluded directory replaced by a file fails closed"
else
  bad "excluded directory file substitution did not fail closed (rc=$excluded_file_rc)"
fi
rm "$PIN/formal/.lake"
mkdir -p "$PIN/formal/.lake/build"

(cd "$PIN" && python3 scripts/lib/pin_manifest.py --root "$PIN" --dir examples \
  >"$TMP/caller-mismatch.out" 2>"$TMP/caller-mismatch.err")
caller_mismatch_rc=$?
if [[ $caller_mismatch_rc -ne 0 ]] && grep -q -- '--dir values must exactly match' "$TMP/caller-mismatch.err"; then
  ok "caller root-list drift fails closed"
else
  bad "caller root-list drift was accepted (rc=$caller_mismatch_rc)"
fi

cp "$PIN/scripts/lib/pin_manifest_policy.json" "$TMP/policy.backup"
python3 - "$PIN/scripts/lib/pin_manifest_policy.json" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    policy = json.load(handle)
policy["excluded_directory_names"].append("zz_cache")
with open(path, "w", encoding="utf-8") as handle:
    json.dump(policy, handle, indent=2)
    handle.write("\n")
PY
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/policy-poison.out" 2>"$TMP/policy-poison.err")
policy_poison_rc=$?
if [[ $policy_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/policy-poison.err"; then
  ok "manifest policy mutation invalidates the pin"
else
  bad "manifest policy mutation did not invalidate the pin (rc=$policy_poison_rc)"
fi
cp "$TMP/policy.backup" "$PIN/scripts/lib/pin_manifest_policy.json"

python3 - "$PIN/scripts/floors/synthetic.count_floor" "$PIN/target/release/anubis" <<'PY'
import os, sys
source, binary = sys.argv[1:]
binary_mtime = os.stat(binary).st_mtime_ns
os.utime(source, ns=(binary_mtime + 5_000_000_000, binary_mtime + 5_000_000_000))
PY
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh >"$TMP/stale-count-floor.out" 2>"$TMP/stale-count-floor.err")
stale_count_floor_rc=$?
if [[ $stale_count_floor_rc -ne 0 ]] && grep -q 'REFUSING to publish a stale pin' "$TMP/stale-count-floor.err"; then
  ok "newer count-floor source refuses stale publication"
else
  bad "newer count-floor source did not refuse stale publication (rc=$stale_count_floor_rc)"
fi
python3 - "$PIN/scripts/floors/synthetic.count_floor" "$PIN/target/release/anubis" <<'PY'
import os, sys
source, binary = sys.argv[1:]
source_mtime = os.stat(source).st_mtime_ns
os.utime(binary, ns=(source_mtime + 1_000_000_000, source_mtime + 1_000_000_000))
PY

printf '2\n' > "$PIN/scripts/floors/synthetic.count_floor"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/floor-poison.out" 2>"$TMP/floor-poison.err")
floor_poison_rc=$?
if [[ $floor_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/floor-poison.err"; then
  ok "coverage-floor mutation invalidates pin"
else
  bad "coverage-floor mutation did not invalidate pin (rc=$floor_poison_rc)"
fi
printf '1\n' > "$PIN/scripts/floors/synthetic.count_floor"

printf '2\n' > "$PIN/.gate_floors/synthetic.floor"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/root-floor-poison.out" 2>"$TMP/root-floor-poison.err")
root_floor_poison_rc=$?
if [[ $root_floor_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/root-floor-poison.err"; then
  ok "root gate-floor mutation invalidates pin"
else
  bad "root gate-floor mutation did not invalidate pin (rc=$root_floor_poison_rc)"
fi
printf '1\n' > "$PIN/.gate_floors/synthetic.floor"

printf '#[test]\nfn evidence_binding_changed() {}\n' > "$PIN/tools/anubis/tests/evidence_binding.rs"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/cli-test-poison.out" 2>"$TMP/cli-test-poison.err")
cli_test_poison_rc=$?
if [[ $cli_test_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/cli-test-poison.err"; then
  ok "CLI integration-test mutation invalidates pin"
else
  bad "CLI integration-test mutation did not invalidate pin (rc=$cli_test_poison_rc)"
fi
printf '#[test]\nfn evidence_binding() {}\n' > "$PIN/tools/anubis/tests/evidence_binding.rs"

printf '# Changed current claim\n' > "$PIN/docs/CLAIMS.md"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/docs-poison.out" 2>"$TMP/docs-poison.err")
docs_poison_rc=$?
if [[ $docs_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/docs-poison.err"; then
  ok "claim-document mutation invalidates pin"
else
  bad "claim-document mutation did not invalidate pin (rc=$docs_poison_rc)"
fi
printf '# Current claim\n' > "$PIN/docs/CLAIMS.md"

printf 'vm sibling source\n' > "$PIN/vm/guest_policy.txt"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/vm-sibling-poison.out" 2>"$TMP/vm-sibling-poison.err")
vm_sibling_poison_rc=$?
if [[ $vm_sibling_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/vm-sibling-poison.err"; then
  ok "vm sibling source mutation invalidates pin"
else
  bad "vm sibling source escaped the manifest (rc=$vm_sibling_poison_rc)"
fi
rm "$PIN/vm/guest_policy.txt"

printf 'int main(void) { return 7; }\n' > "$PIN/poc_kit/vuln_local.c"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/poc-kit-poison.out" 2>"$TMP/poc-kit-poison.err")
poc_kit_poison_rc=$?
if [[ $poc_kit_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/poc-kit-poison.err"; then
  ok "offensive target source mutation invalidates pin"
else
  bad "offensive target source mutation did not invalidate pin (rc=$poc_kit_poison_rc)"
fi
printf 'int main(void) { return 0; }\n' > "$PIN/poc_kit/vuln_local.c"

published_pin_rel="$(tr -d '\n' < "$PIN/vm/pins/CURRENT")"
published_pin="$PIN/$published_pin_rel"
cp "$published_pin" "$TMP/published-pin.backup"
chmod u+w "$published_pin"
printf '# pin-byte-poison\n' >> "$published_pin"
chmod a-w "$published_pin"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/pin-byte-poison.out" 2>"$TMP/pin-byte-poison.err")
pin_byte_poison_rc=$?
if [[ $pin_byte_poison_rc -ne 0 ]] && grep -q 'PIN_BYTES_MISMATCH' "$TMP/pin-byte-poison.err"; then
  ok "pin byte mutation fails strict verification"
else
  bad "pin byte mutation did not fail with explicit diagnostic (rc=$pin_byte_poison_rc)"
fi
(cd "$PIN" && ANUBIS_PIN_ALLOW_STALE=1 /bin/bash -p scripts/publish_pin.sh >"$TMP/pin-collision.out" 2>"$TMP/pin-collision.err")
pin_collision_rc=$?
if [[ $pin_collision_rc -ne 0 ]] && grep -q 'PIN_COLLISION' "$TMP/pin-collision.err"; then
  ok "publication refuses a mismatched existing content-addressed pin"
else
  bad "publication reused a mismatched existing content-addressed pin (rc=$pin_collision_rc)"
fi
chmod u+w "$published_pin"
cp "$TMP/published-pin.backup" "$published_pin"
chmod a-w "$published_pin"

cp "$PIN/vm/pins/CURRENT" "$TMP/current.backup"
rm "$PIN/vm/pins/CURRENT"
ln -s "$TMP/current.backup" "$PIN/vm/pins/CURRENT"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --current >"$TMP/current-symlink-current.out" 2>"$TMP/current-symlink-current.err")
current_symlink_current_rc=$?
if [[ $current_symlink_current_rc -ne 0 ]] && grep -q 'PIN_CURRENT_INVALID' "$TMP/current-symlink-current.err"; then
  ok "--current rejects a symlink CURRENT"
else
  bad "--current followed a symlink CURRENT (rc=$current_symlink_current_rc)"
fi
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/current-symlink.out" 2>"$TMP/current-symlink.err")
current_symlink_rc=$?
if [[ $current_symlink_rc -ne 0 ]] && grep -q 'PIN_CURRENT_INVALID' "$TMP/current-symlink.err"; then
  ok "symlink CURRENT fails strict verification"
else
  bad "symlink CURRENT did not fail with explicit diagnostic (rc=$current_symlink_rc)"
fi
rm "$PIN/vm/pins/CURRENT"
cp "$TMP/current.backup" "$PIN/vm/pins/CURRENT"
printf '../../target/release/anubis\n' > "$PIN/vm/pins/CURRENT"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/current-traversal.out" 2>"$TMP/current-traversal.err")
current_traversal_rc=$?
if [[ $current_traversal_rc -ne 0 ]] && grep -q 'PIN_CURRENT_INVALID' "$TMP/current-traversal.err"; then
  ok "CURRENT traversal path fails strict verification"
else
  bad "CURRENT traversal path did not fail with explicit diagnostic (rc=$current_traversal_rc)"
fi
cp "$TMP/current.backup" "$PIN/vm/pins/CURRENT"

published_meta="$published_pin.meta"
cp "$published_meta" "$TMP/published-meta.backup"
# Published metadata is intentionally read-only.  BSD rm prompts before removing an
# unwritable regular file when stdin is a terminal, which can hang this poison test
# instead of producing a verdict.  The target is the exact scratch-published path;
# force only these controlled removals so interactive and non-interactive runs agree.
rm -f -- "$published_meta"
ln -s "$TMP/published-meta.backup" "$published_meta"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/meta-symlink.out" 2>"$TMP/meta-symlink.err")
meta_symlink_rc=$?
if [[ $meta_symlink_rc -ne 0 ]] && grep -q 'PIN_META_INVALID' "$TMP/meta-symlink.err"; then
  ok "symlink pin metadata fails strict verification"
else
  bad "symlink pin metadata did not fail with explicit diagnostic (rc=$meta_symlink_rc)"
fi
rm -f -- "$published_meta"
cp "$TMP/published-meta.backup" "$published_meta"
chmod u+w "$published_meta"
cp "$published_meta" "$TMP/published-meta-duplicate.backup"
meta_sha="$(awk -F': ' '$1 == "sha256" { print $2 }' "$published_meta")"
printf 'sha256: %s\n' "$meta_sha" >> "$published_meta"
chmod a-w "$published_meta"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/meta-duplicate.out" 2>"$TMP/meta-duplicate.err")
meta_duplicate_rc=$?
if [[ $meta_duplicate_rc -ne 0 ]] && grep -q 'PIN_META_INVALID' "$TMP/meta-duplicate.err"; then
  ok "duplicate metadata field fails strict verification"
else
  bad "duplicate metadata field did not fail with explicit diagnostic (rc=$meta_duplicate_rc)"
fi
chmod u+w "$published_meta"
cp "$TMP/published-meta-duplicate.backup" "$published_meta"

cp "$published_meta" "$TMP/published-meta-count.backup"
python3 - "$published_meta" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
lines = p.read_text().splitlines()
p.write_text("\n".join("src_count: 999999" if line.startswith("src_count:") else line for line in lines) + "\n")
PY
chmod a-w "$published_meta"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/meta-count-poison.out" 2>"$TMP/meta-count-poison.err")
meta_count_poison_rc=$?
if [[ $meta_count_poison_rc -ne 0 ]] && grep -q 'PIN_MANIFEST_MISMATCH' "$TMP/meta-count-poison.err"; then
  ok "metadata source-count mutation fails strict verification"
else
  bad "metadata source-count mutation did not fail with explicit diagnostic (rc=$meta_count_poison_rc)"
fi
chmod u+w "$published_meta"
cp "$TMP/published-meta-count.backup" "$published_meta"
chmod a-w "$published_meta"

chmod u+w "$published_meta"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/meta-writable.out" 2>"$TMP/meta-writable.err")
meta_writable_rc=$?
if [[ $meta_writable_rc -ne 0 ]] && grep -q 'PIN_META_INVALID' "$TMP/meta-writable.err"; then
  ok "writable pin metadata fails strict verification"
else
  bad "writable pin metadata did not fail with explicit diagnostic (rc=$meta_writable_rc)"
fi
chmod a-w "$published_meta"

chmod u+w "$published_pin"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/pin-writable.out" 2>"$TMP/pin-writable.err")
pin_writable_rc=$?
if [[ $pin_writable_rc -ne 0 ]] && grep -q 'PIN_FILE_INVALID' "$TMP/pin-writable.err"; then
  ok "writable content-addressed pin fails strict verification"
else
  bad "writable content-addressed pin did not fail with explicit diagnostic (rc=$pin_writable_rc)"
fi
chmod a-w "$published_pin"

printf 'fn main() {}\n' > "$PIN/examples/showcase/untracked_after_publish.anb"
(cd "$PIN" && /bin/bash -p scripts/publish_pin.sh --verify >"$TMP/pin-poison.out" 2>"$TMP/pin-poison.err")
pin_poison_rc=$?
if [[ $pin_poison_rc -ne 0 ]] && grep -q 'PIN DOES NOT MATCH THE TREE' "$TMP/pin-poison.err"; then
  ok "untracked showcase fixture invalidates pin"
else
  bad "untracked showcase fixture did not invalidate pin (rc=$pin_poison_rc)"
fi

echo "CORPUS_INVENTORY_BINDING: $pass passed, $fail failed"
[[ $fail -eq 0 ]]
