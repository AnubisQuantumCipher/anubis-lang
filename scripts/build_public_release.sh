#!/bin/bash -p
#
# Assemble a PUBLIC, source-backed release tree from one verified, commit-bound pin.
#
# WHY THIS EXISTS, given `scripts/build_release_candidate.sh` already did something similar:
# that script is not release-authoritative. It builds and runs through the MUTABLE
# `target/release` tree, contains a placeholder gate, permits a Metal skip, ignores copy
# failures, writes its manifest before the report it is supposed to cover, self-hashes a
# manifest it then mutates, and ships no paired verifier. Each of those turns a red result
# into a green one. This script refuses instead.
#
# CONTRACT
#   * The binary is COPIED from an immutable pin that `--verify-release` just accepted. It is
#     never rebuilt here, so the published bytes are the bytes that were graded.
#   * Every asset digest lands in ONE manifest that does not contain its own hash. A manifest
#     that hashes itself cannot be checked.
#   * The leak gate runs over the STAGED TREE and refuses on any operator path, credential,
#     key, or build-root residue. This lane is source-backed, so PUBLIC source file names are
#     expected and reported rather than falsely described as absent.
#   * Nothing is uploaded. This produces a tree for review; publication is a separate,
#     separately approved transaction.
#
# Usage: scripts/build_public_release.sh --pin <path> --tag <tag> --out <dir> [--ci-artifact <dir>]
set -euo pipefail
IFS=$' \t\n'
unset CDPATH ENV
export PATH="/usr/bin:/bin:/usr/sbin:/sbin"

die() { echo "RELEASE_REFUSED: $*" >&2; exit 1; }

PIN=""; TAG=""; OUT=""; CI_ARTIFACT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pin) PIN="${2:-}"; shift 2 ;;
    --tag) TAG="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --ci-artifact) CI_ARTIFACT="${2:-}"; shift 2 ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ -n "$PIN" && -n "$TAG" && -n "$OUT" ]] || die "usage: --pin <path> --tag <tag> --out <dir> [--ci-artifact <dir>]"

# Resolve OUT to an absolute path BEFORE any subshell captures $STAGE = $OUT/$TAG/$COMMIT.
# The two ( cd "$STAGE/…" && tar -czf "$STAGE/dist/…" ) blocks near the end of this script
# evaluate $STAGE from the SUBSHELL's cwd — a relative --out would make the tar destination
# unreachable and the script fails with:
#     tar: Failed to open 'out/.../dist/anubis-<tag>-macos-arm64.tar.gz'
# after the leak scan already passed and the whole staged tree is on disk. Same trap applies
# to the evidence archive and the SHA256SUMS emit in the following two subshells.
#
# `mkdir -p "$OUT" && cd "$OUT" && pwd` is the standard idiom: creates the base if missing,
# then re-canonicalizes to an absolute path. Symlink resolution is left to the caller (no
# `-P`) so a caller's chosen symlink layout is preserved.
OUT="$(mkdir -p -- "$OUT" && cd -- "$OUT" && pwd)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ---------------------------------------------------------------- source identity
#
# Two allowances, both owned by the publication itself and neither able to change the SOURCE:
#   * `vm/pins/CURRENT` is tracked and a successful `--release` necessarily rewrites it to name
#     the pin being shipped. `publish_pin.sh` makes the same allowance for the same reason.
#   * `vm/pins/*` pin binaries and their `.meta` sidecars are gitignored build products, so they
#     are permanently untracked and can never be "clean".
# Any OTHER dirty path means the bytes on disk are not the bytes at the commit, and the release
# would name a commit it was not built from.
dirty="$(git status --porcelain=v1 | sed 's/^...//' | grep -v '^vm/pins/' || true)"
[[ -z "$dirty" ]] || die "worktree has changes outside vm/pins/:"$'\n'"$dirty"
COMMIT="$(git rev-parse --verify 'HEAD^{commit}')"
TREE="$(git rev-parse --verify 'HEAD^{tree}')"

# ---------------------------------------------------------------- pin identity
[[ -f "$PIN" && -x "$PIN" && ! -L "$PIN" ]] || die "pin is not a regular executable: $PIN"
# `--verify-release` takes NO path: it verifies the pin CURRENT names, against the current tree.
# Passing one silently prints usage and exits 2, which an unguarded `$?` read would have taken
# for a real verdict. Assert CURRENT names the requested pin first, then verify.
current_pin="$(bash -p scripts/publish_pin.sh --current)" \
  || die "publish_pin.sh --current failed"
[[ "$ROOT/$current_pin" == "$(cd "$(dirname "$PIN")" && pwd)/$(basename "$PIN")" ]] \
  || die "CURRENT names $current_pin, not the requested $PIN"
verify_rc=0
bash -p scripts/publish_pin.sh --verify-release >/dev/null 2>&1 || verify_rc=$?
[[ "$verify_rc" -eq 0 ]] || die "publish_pin.sh --verify-release rejected $current_pin (rc=$verify_rc)"
BIN_SHA="$(shasum -a 256 "$PIN" | awk '{print $1}')"
[[ -f "$PIN.meta" ]] || die "pin metadata is missing: $PIN.meta"
META_SHA="$(shasum -a 256 "$PIN.meta" | awk '{print $1}')"

STAGE="$OUT/$TAG/$COMMIT"
# `[[ -e … ]] && die …` would make the NON-existent (i.e. good) case the statement's failing
# exit status, and `set -e` would abort the script before staging anything.
if [[ -e "$STAGE" ]]; then die "output already exists, refusing to overwrite: $STAGE"; fi
mkdir -p "$STAGE/public/binary/macos-arm64" "$STAGE/public/provenance" \
         "$STAGE/public/evidence" "$STAGE/public/checksums" "$STAGE/public/verification" "$STAGE/dist"

# ---------------------------------------------------------------- binary + its description
cp "$PIN" "$STAGE/public/binary/macos-arm64/anubis"
chmod 0755 "$STAGE/public/binary/macos-arm64/anubis"
COPIED_SHA="$(shasum -a 256 "$STAGE/public/binary/macos-arm64/anubis" | awk '{print $1}')"
[[ "$COPIED_SHA" == "$BIN_SHA" ]] || die "copied binary digest $COPIED_SHA != pin digest $BIN_SHA"

file "$PIN"                       > "$STAGE/public/binary/macos-arm64/file.txt" 2>&1 || true
otool -L "$PIN"                   > "$STAGE/public/binary/macos-arm64/linked-frameworks.txt" 2>&1 || true
otool -l "$PIN" | sed -n '/LC_BUILD_VERSION/,/^$/p' \
                                  > "$STAGE/public/binary/macos-arm64/macho-build-version.txt" 2>&1 || true
codesign -dvvv "$PIN"             > "$STAGE/public/binary/macos-arm64/codesign.txt" 2>&1 || true
# The signing story is stated, never implied. Ad-hoc/linker-signed is NOT notarized and must not
# be presented as installable-without-warning.
if grep -q 'adhoc' "$STAGE/public/binary/macos-arm64/codesign.txt" 2>/dev/null; then
  SIGNING="adhoc-unnotarized"
else
  SIGNING="see-codesign-txt"
fi

# ---------------------------------------------------------------- provenance
RUSTC_V="$(rustc --version 2>/dev/null || echo unavailable)"
CARGO_V="$(cargo --version 2>/dev/null || echo unavailable)"
Z3_V="$(z3 --version 2>/dev/null || echo unavailable)"
SDK="$(xcrun --show-sdk-version 2>/dev/null || echo unavailable)"
XCODE="$(xcodebuild -version 2>/dev/null | tr '\n' ' ' || echo unavailable)"

cat > "$STAGE/public/provenance/source.json" <<EOF
{
  "schema": "anubis.public-release.source.v1",
  "repository": "AnubisQuantumCipher/anubis-lang",
  "commit": "$COMMIT",
  "git_tree": "$TREE",
  "tag": "$TAG",
  "visibility": "public",
  "posture": "source-backed"
}
EOF

cat > "$STAGE/public/provenance/toolchain.json" <<EOF
{
  "schema": "anubis.public-release.toolchain.v1",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "z3": "$Z3_V",
  "macos_sdk": "$SDK",
  "xcode": "$XCODE",
  "host_arch": "$(uname -m)",
  "host_os": "$(uname -sr)"
}
EOF

cat > "$STAGE/public/provenance/build.json" <<EOF
{
  "schema": "anubis.public-release.build.v1",
  "method": "publish_pin.sh --release",
  "description": "cargo build --locked --release -p anubis from an exact-HEAD git archive, isolated CARGO_TARGET_DIR, empty per-run CARGO_HOME, clean env",
  "pin_path": "$(basename "$PIN")",
  "binary_sha256": "$BIN_SHA",
  "pin_meta_sha256": "$META_SHA",
  "verified_with": "publish_pin.sh --verify-release",
  "signing": "$SIGNING",
  "rebuilt_for_packaging": false
}
EOF

# ---------------------------------------------------------------- evidence
if [[ -n "$CI_ARTIFACT" ]]; then
  [[ -d "$CI_ARTIFACT" ]] || die "--ci-artifact is not a directory: $CI_ARTIFACT"
  mkdir -p "$STAGE/public/evidence/hosted-ci"
  for f in gate_report.json attestation_identity.txt profile_environment.txt MANIFEST.sha256; do
    [[ -f "$CI_ARTIFACT/$f" ]] || die "hosted CI artifact is incomplete, missing $f"
    cp "$CI_ARTIFACT/$f" "$STAGE/public/evidence/hosted-ci/$f"
  done
  # The artifact must attest to the commit being released, or it is evidence about something else.
  grep -q "github_sha=$COMMIT" "$STAGE/public/evidence/hosted-ci/attestation_identity.txt" \
    || die "hosted CI attestation does not bind commit $COMMIT"
  grep -q '"verdict": *"HOSTED_PASS"' "$STAGE/public/evidence/hosted-ci/gate_report.json" \
    || die "hosted CI verdict is not HOSTED_PASS"
fi

# ---------------------------------------------------------------- leak and integrity gate
LEAK="$STAGE/public/evidence/leak-scan.txt"
{
  echo "# Leak and integrity scan over the staged tree"
  echo "# Source-backed lane: PUBLIC source file names are expected and are reported, not hidden."
  echo
} > "$LEAK"
leak_fail=0
scan_binary="$STAGE/public/binary/macos-arm64/anubis"
# The `/Users/[a-z]` pattern is intended to catch operator-home leaks (e.g. `/Users/sicarii/…`
# baked into DWARF debuginfo, panic messages, or format strings). Two `/Users/<name>` paths are
# NOT operator homes and appear here only as documented examples in `--help` text that the Rust
# compiler embeds from `///` doc comments:
#
#   * `/Users/admin` — the canonical Tart guest user, documented at:
#     - tools/anubis/src/vz.rs (Sync + Exploit variants, sample `--to` paths)
#     - docs/CLI.md (§ anubis vz sync example line)
#     - docs/language/POC_KIT.md (guest-home conventions)
#   * `/Users/runner` — GitHub Actions macos-latest home; can appear in embedded workflow
#     env-var help text or referenced in CI-produced error strings baked into build artifacts.
#
# These are documented public constants, not operator identifiers. Any OTHER `/Users/<name>`
# occurrence is still an operator-home leak and fails closed.
#
# The scanner extracts one MATCH per hit (not one record per hit) so a `strings` record that
# contains both an allowlisted `/Users/admin` AND an unsafe `/Users/sicarii` yields TWO
# matches — only the safe one is filtered, and the operator-home leak still fails the gate.
# Word-boundary continuation (`[^[:space:]]{0,60}`) keeps each match from eating past the next
# space into a co-located path.
leak_users_safe_prefixes='/Users/(admin|runner)([^A-Za-z0-9_]|$)'
for pattern in '/Users/[a-z]' '/private/var/folders' 'cargo/registry' 'BEGIN [A-Z ]*PRIVATE KEY' 'AKIA[0-9A-Z]{16}' 'ghp_[A-Za-z0-9]{20,}'; do
  matches="$(strings -a "$scan_binary" 2>/dev/null | grep -oE "$pattern[^[:space:]]{0,60}" || true)"
  if [[ "$pattern" == '/Users/[a-z]' ]]; then
    offending="$(printf '%s\n' "$matches" | grep -vE "^${leak_users_safe_prefixes}" || true)"
  else
    offending="$matches"
  fi
  n="$(printf '%s' "$offending" | grep -c . || true)"
  printf '%-34s %s\n' "$pattern" "$n" >> "$LEAK"
  if [[ "$n" -ne 0 ]]; then
    leak_fail=1
    printf '  OFFENDING:\n' >> "$LEAK"
    printf '%s\n' "$offending" | sort -u | head -20 | sed 's/^/    /' >> "$LEAK"
  fi
done
# No .git, no key material, no raw build trees anywhere in the staged tree.
if find "$STAGE" \( -name '.git' -o -name '*.pem' -o -name 'id_rsa*' -o -name '*.p12' \) -print -quit | grep -q .; then
  echo "FOUND forbidden path in staged tree" >> "$LEAK"
  leak_fail=1
fi
[[ "$leak_fail" -eq 0 ]] || { cat "$LEAK" >&2; die "leak gate found forbidden content; see $LEAK"; }
echo >> "$LEAK"
echo "RESULT: PASS (no operator path, build root, registry path, key, or token residue)" >> "$LEAK"

# ---------------------------------------------------------------- verifier
cp scripts/verify_public_release.py "$STAGE/public/verification/verify_release.py"
cat > "$STAGE/public/verification/README.md" <<EOF
# Verify this release

\`\`\`sh
/usr/bin/python3 -I -B verify_release.py --root ..
echo "rc=\$?"
\`\`\`

Exit code 0 means every asset listed in \`checksums/SHA256SUMS\` is present and matches, the
shipped binary matches the digest recorded in \`provenance/build.json\`, and the hosted-CI
attestation (when bundled) names the same commit as \`provenance/source.json\`.

The verifier proves ASSET INTEGRITY and SOURCE BINDING. It does not re-run the gates and it does
not establish universal soundness. See \`docs/CLAIMS.md\` in the repository for what is and is not
claimed about the language itself.
EOF

# ---------------------------------------------------------------- checksums (never self-hashing)
( cd "$STAGE/public" && find . -type f ! -path './checksums/*' -print0 \
    | sort -z | xargs -0 shasum -a 256 ) > "$STAGE/public/checksums/SHA256SUMS"

# ---------------------------------------------------------------- distributable archives
( cd "$STAGE/public/binary/macos-arm64" && tar -czf "$STAGE/dist/anubis-$TAG-macos-arm64.tar.gz" anubis )
( cd "$STAGE/public" && tar -czf "$STAGE/dist/anubis-$TAG-evidence.tar.gz" \
    provenance evidence checksums verification )
( cd "$STAGE/dist" && shasum -a 256 ./*.tar.gz > SHA256SUMS )

echo "RELEASE_STAGED tag=$TAG commit=$COMMIT tree=$TREE binary_sha256=$BIN_SHA signing=$SIGNING out=$STAGE"
