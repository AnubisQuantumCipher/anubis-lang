#!/usr/bin/env bash
set -euo pipefail
# Fetch and prepare an aarch64 Linux kernel that VZLinuxBootLoader can actually boot.
#
# THE TRAP THIS SCRIPT EXISTS FOR: Alpine's `vmlinuz-virt` is an EFI ZBOOT image — the real
# kernel gzipped inside a PE/COFF wrapper (`zimg` magic at 0x4). `file` reports it as a
# perfectly good "PE32+ executable (EFI application) Aarch64" and it is useless to VZ, which
# needs a RAW arm64 `Image`. Handing the zboot file to `native-boot` produces only:
#
#   ANUBIS_VZNATIVE_START_FAILED: Internal Virtualization error.
#
# — no mention of the kernel, the format, or what to do. The check below is the difference
# between a two-minute fix and an afternoon.
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
BASE="${ANUBIS_ALPINE_BASE:-https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/aarch64/netboot}"
OUT="vm/kernels"; mkdir -p "$OUT"

for f in vmlinuz-virt initramfs-virt config-6.12.81-0-virt; do
  [[ -f "$OUT/$f" ]] && continue
  echo "[fetch] $f"
  curl -fsS -o "$OUT/$f" "$BASE/$f"
done
[[ -f "$OUT/config-virt" ]] || cp "$OUT/config-6.12.81-0-virt" "$OUT/config-virt" 2>/dev/null || true

python3 - "$OUT" <<'PY'
import struct, sys, gzip, os
out = sys.argv[1]
src = os.path.join(out, 'vmlinuz-virt')
dst = os.path.join(out, 'Image-virt')
d = open(src, 'rb').read()

def arm64_ok(b):
    return len(b) > 0x3c and b[0x38:0x3c] == bytes.fromhex('41524d64')

if arm64_ok(d):
    open(dst, 'wb').write(d)
    print("[ok] vmlinuz-virt is already a raw arm64 Image")
    raise SystemExit(0)

i = d.find(b'zimg')
if i < 0:
    raise SystemExit("[FAIL] not a raw arm64 Image and no zimg header — unknown kernel format, refusing to guess")

payload_offset, payload_size = struct.unpack_from('<II', d, i + 4)
comp = d[i+20:i+52].split(b'\x00')[0].decode(errors='replace')
print(f"[zboot] payload_offset={hex(payload_offset)} size={payload_size} compression={comp}")
blob = d[payload_offset:payload_offset+payload_size]
if comp != 'gzip':
    raise SystemExit(f"[FAIL] payload compression is {comp!r}; only gzip is handled here")

raw = gzip.decompress(blob)
if not arm64_ok(raw):
    raise SystemExit("[FAIL] decompressed payload has no arm64 Image magic at 0x38 — refusing to publish it")
open(dst, 'wb').write(raw)
print(f"[ok] extracted raw arm64 Image ({len(raw)} bytes) -> {dst}")
PY

echo
echo "boot it:"
echo "  ./target/release/anubis vz native-boot <program.anb> \\"
echo "      --kernel vm/kernels/Image-virt --initrd vm/kernels/initramfs-virt"
