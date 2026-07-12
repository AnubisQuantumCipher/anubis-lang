#!/usr/bin/env bash
# Phase 8+ — External Reproducibility Gate (macOS / unit 1). Fail-closed.
#
# The self-host binary fixpoint (run_selfhost_gate.sh) proves same-machine,
# same-toolchain binary identity. This gate upgrades that to a REPRODUCIBLE
# build: the byte-identical fixpoint source (stage2.rs) compiled under a pinned
# toolchain with ALL machine-identity paths remapped yields a binary that
# (a) is deterministic across independent build directories, and
# (b) contains ZERO host/user paths — so a third party on another machine can
# re-derive the exact bytes from the recorded manifest.
#
# Claim scope (honest): REPRODUCIBILITY under a pinned toolchain + normalized
# environment. NOT toolchain diversity / trusting-trust closure — a subverted
# rustc reproduces its own subversion here too. Different rustc VERSIONS emit
# different code and are NOT expected to match (measured: stable 1.94 vs
# nightly 1.97 differ). Full Thompson closure needs a second independent
# backend (see docs/language/SELFHOST_REPRO_PLAN.md + SELFHOST.md).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
SEAL_OUT="${1:-out/selfhost_gate}"
if [[ "$SEAL_OUT" != /* ]]; then SEAL_OUT="$ROOT/$SEAL_OUT"; fi
OUT="$ROOT/out/selfhost_repro_gate"
rm -rf "$OUT"; mkdir -p "$OUT"

pass=0; fail=0
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }
pass_one() { pass=$((pass+1)); note "$1: PASS"; }
fail_one() { fail=$((fail+1)); note "$1: FAIL"; }
: >"$OUT/summary.txt"

# Optional explicit toolchain pin: ANUBIS_REPRO_TOOLCHAIN=nightly-2026-05-10
RUSTC=(rustc)
if [[ -n "${ANUBIS_REPRO_TOOLCHAIN:-}" ]]; then RUSTC=(rustc "+${ANUBIS_REPRO_TOOLCHAIN}"); fi
TC_VER="$("${RUSTC[@]}" --version 2>/dev/null || echo unknown)"

# ---------------------------------------------------------------------------
# 0. Obtain the byte-identical fixpoint SOURCE. Prefer the seal's stage2.rs
#    (already proven == stage3.rs); otherwise run the seal to produce it.
# ---------------------------------------------------------------------------
echo "== fixpoint source =="
if [[ -f "$SEAL_OUT/stage2.rs" && -f "$SEAL_OUT/stage3.rs" ]] && cmp -s "$SEAL_OUT/stage2.rs" "$SEAL_OUT/stage3.rs"; then
  note "reusing established fixpoint source: $SEAL_OUT/stage2.rs"
else
  note "no established fixpoint source — running run_selfhost_gate.sh first"
  if ! bash "$ROOT/scripts/run_selfhost_gate.sh" "$SEAL_OUT" >"$OUT/seal.log" 2>&1; then
    fail_one "fixpoint_source_available"; echo "SELFHOST_REPRO_GATE: FAIL ($pass pass / $fail fail)"; exit 1
  fi
fi
if [[ -f "$SEAL_OUT/stage2.rs" ]]; then
  cp "$SEAL_OUT/stage2.rs" "$OUT/fixpoint.rs"
  SRC_SHA="$(shasum -a 256 "$OUT/fixpoint.rs" | awk '{print $1}')"
  pass_one "fixpoint_source_available"
else
  fail_one "fixpoint_source_available"; echo "SELFHOST_REPRO_GATE: FAIL ($pass pass / $fail fail)"; exit 1
fi

# ---------------------------------------------------------------------------
# Reproducible build: pinned flags + remap HOME (covers ~/.rustup, ~/.cargo,
# std source paths) and the build dir, both to fixed tokens. SOURCE_DATE_EPOCH
# pinned. Build from an identical canonical filename so rustc's embedded source
# path is constant. Normalize the only per-link nondeterministic Mach-O fields
# (ad-hoc signature + content-derived LC_UUID).
# ---------------------------------------------------------------------------
repro_build() {  # $1 = physical build dir, $2 = output binary path
  local bdir="$1" obin="$2"
  mkdir -p "$bdir"
  cp "$OUT/fixpoint.rs" "$bdir/canon.rs"
  SOURCE_DATE_EPOCH=0 "${RUSTC[@]}" -O -C codegen-units=1 -C debuginfo=0 \
    "--remap-path-prefix=$HOME=/anubis-home" \
    "--remap-path-prefix=$bdir=/anubis-build" \
    "$bdir/canon.rs" -o "$obin" 2>>"$OUT/rustc.err"
}
normalize() {  # $1 = binary (normalized in place, on a copy)
  command -v codesign >/dev/null 2>&1 && codesign --remove-signature "$1" 2>/dev/null || true
  python3 "$ROOT/scripts/macho_normalize.py" "$1" >/dev/null 2>&1 || true
}

echo "== reproducible build (pinned + remapped) =="
BUILD_OK=1
repro_build "$OUT/bdirA" "$OUT/reproA.bin" || BUILD_OK=0
# Independent SECOND build dir with a DIFFERENT absolute path — both remap to
# /anubis-build, so a deterministic build must produce identical bytes.
repro_build "$OUT/bdirB_different_path" "$OUT/reproB.bin" || BUILD_OK=0
if [[ $BUILD_OK -ne 1 ]]; then
  fail_one "repro_build"; tail -20 "$OUT/rustc.err" >>"$OUT/summary.txt" 2>/dev/null || true
  echo "SELFHOST_REPRO_GATE: FAIL ($pass pass / $fail fail)"; exit 1
fi
pass_one "repro_build"

# 1. Liveness — the reproducible binary is a real, runnable anubis-sh compiler.
echo "== liveness =="
if "$OUT/reproA.bin" version 2>/dev/null | grep -q "anubis-sh"; then
  pass_one "repro_binary_runnable"
else
  fail_one "repro_binary_runnable"
fi

# 2. Determinism across independent build directories (normalized).
echo "== determinism (independent build dirs) =="
cp "$OUT/reproA.bin" "$OUT/reproA.norm"; cp "$OUT/reproB.bin" "$OUT/reproB.norm"
normalize "$OUT/reproA.norm"; normalize "$OUT/reproB.norm"
REPRO_SHA="$(shasum -a 256 "$OUT/reproA.norm" | awk '{print $1}')"
if cmp -s "$OUT/reproA.norm" "$OUT/reproB.norm"; then
  note "reproducible sha256 (normalized): $REPRO_SHA"
  pass_one "repro_deterministic_cross_builddir"
else
  fail_one "repro_deterministic_cross_builddir"
fi

# 3. Machine-independence — ZERO host/user identity paths survive the remap.
echo "== machine-independence =="
HOME_HITS="$(strings "$OUT/reproA.norm" 2>/dev/null | grep -c "$HOME" || true)"
USERS_HITS="$(strings "$OUT/reproA.norm" 2>/dev/null | grep -c "/Users/" || true)"
if [[ "$HOME_HITS" -eq 0 && "$USERS_HITS" -eq 0 ]]; then
  note "0 host-identity paths (HOME + /Users/) in the reproducible binary"
  pass_one "repro_no_machine_paths"
else
  note "machine paths leaked: HOME=$HOME_HITS /Users/=$USERS_HITS"
  strings "$OUT/reproA.norm" 2>/dev/null | grep "$HOME" | head -3 >>"$OUT/summary.txt" || true
  fail_one "repro_no_machine_paths"
fi

# ---------------------------------------------------------------------------
# HERMETIC LANE (unit 2) — the externally-publishable claim. Build the fixpoint
# source inside a pinned rust container twice, in independent runs; require a
# bit-identical Linux ELF. Inside the image every path is fixed (toolchain,
# HOME, /work), so a third party with the same image DIGEST + source re-derives
# the exact bytes. Runs when Docker is available; REQUIRED when ANUBIS_REPRO_DOCKER=1.
# ---------------------------------------------------------------------------
LINUX_STATUS="skipped"; LINUX_SHA=""; IMG_DIGEST=""
REPRO_IMAGE="${ANUBIS_REPRO_IMAGE:-rust:1.83-slim-bookworm}"
echo "== hermetic Linux lane (Docker) =="
if docker info >/dev/null 2>&1; then
  docker pull -q "$REPRO_IMAGE" >/dev/null 2>&1 || true
  IMG_DIGEST="$(docker inspect --format '{{index .RepoDigests 0}}' "$REPRO_IMAGE" 2>/dev/null || true)"
  hermetic_sha() {
    docker run --rm -v "$OUT:/src:ro" "$REPRO_IMAGE" bash -c '
      set -e; mkdir -p /work; cp /src/fixpoint.rs /work/canon.rs; cd /work
      SOURCE_DATE_EPOCH=0 rustc -O -C codegen-units=1 -C debuginfo=0 \
        -C link-args=-Wl,--build-id=none canon.rs -o out.bin 2>/dev/null
      sha256sum out.bin | cut -d" " -f1' 2>/dev/null | tail -1
  }
  L1="$(hermetic_sha)"; L2="$(hermetic_sha)"
  if [[ -n "$L1" && "$L1" == "$L2" ]]; then
    LINUX_STATUS="reproducible"; LINUX_SHA="$L1"
    note "hermetic Linux ELF sha256: $L1 (image $IMG_DIGEST)"
    pass_one "repro_hermetic_linux"
  else
    LINUX_STATUS="differ"
    note "hermetic builds differ: $L1 vs $L2"
    fail_one "repro_hermetic_linux"
  fi
elif [[ "${ANUBIS_REPRO_DOCKER:-0}" == "1" ]]; then
  note "ANUBIS_REPRO_DOCKER=1 but Docker daemon is down"
  fail_one "repro_hermetic_linux"
else
  note "repro_hermetic_linux: SKIP (Docker unavailable; set ANUBIS_REPRO_DOCKER=1 to require)"
fi

# Reproducibility manifest — what a third party pins to re-derive the bytes.
python3 - "$OUT" "$TC_VER" "$SRC_SHA" "$REPRO_SHA" "$LINUX_STATUS" "$LINUX_SHA" "$IMG_DIGEST" "$REPRO_IMAGE" <<'PY' || true
import json, sys
out, tc, src_sha, repro_sha, lin_status, lin_sha, img_digest, img = sys.argv[1:9]
manifest = {
    "artifact": "anubis-sh self-host compiler (stage2 fixpoint source)",
    "source_sha256": src_sha,
    "macos_lane": {
        "toolchain": tc,
        "target": "aarch64-apple-darwin",
        "reproducible_sha256": repro_sha,
        "build": {
            "flags": ["-O", "-C codegen-units=1", "-C debuginfo=0"],
            "remap_path_prefix": ["$HOME=/anubis-home", "<builddir>=/anubis-build"],
            "SOURCE_DATE_EPOCH": "0",
            "canonical_source_name": "canon.rs",
            "normalize": ["codesign --remove-signature", "zero LC_UUID"],
        },
    },
    "hermetic_linux_lane": {
        "status": lin_status,
        "image": img,
        "image_digest": img_digest,
        "target": "aarch64-unknown-linux-gnu (or host arch in image)",
        "reproducible_sha256": lin_sha,
        "build": {
            "flags": ["-O", "-C codegen-units=1", "-C debuginfo=0",
                      "-C link-args=-Wl,--build-id=none"],
            "SOURCE_DATE_EPOCH": "0",
            "note": "all paths fixed by the container image; no remap needed",
        },
    },
    "claim": "Reproducible under the pinned toolchain/image + normalized "
             "environment. NOT toolchain-diversity / trusting-trust closure.",
}
open(f"{out}/repro_manifest.json", "w").write(json.dumps(manifest, indent=2) + "\n")
print("  wrote repro_manifest.json")
PY

{
  echo "selfhost_repro_gate pass=$pass fail=$fail"
  echo "toolchain: $TC_VER"
} | tee -a "$OUT/summary.txt"

if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_REPRO_GATE: FAIL ($pass pass / $fail fail)"
  exit 1
fi
echo "SELFHOST_REPRO_GATE: PASS ($pass/$pass)"
exit 0
