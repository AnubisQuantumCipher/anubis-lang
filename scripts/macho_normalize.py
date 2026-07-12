#!/usr/bin/env python3
# Zero the content-derived LC_UUID load command in a Mach-O binary, in place.
# Used by the self-host binary-fixpoint seal: LC_UUID + the ad-hoc code
# signature are the only per-link nondeterministic Mach-O fields and carry no
# program semantics. codesign --remove-signature strips the signature; this
# zeroes the UUID so two builds of byte-identical source compare byte-identical.
import struct
import sys

LC_UUID = 0x1B
MH_MAGIC_64 = 0xFEEDFACF
MH_CIGAM_64 = 0xCFFAEDFE
MH_MAGIC_32 = 0xFEEDFACE
MH_CIGAM_32 = 0xCEFAEDFE


def zero_uuid(path: str) -> int:
    data = bytearray(open(path, "rb").read())
    magic = struct.unpack("<I", data[0:4])[0]
    if magic in (MH_MAGIC_64, MH_CIGAM_64):
        endian = "<" if magic == MH_MAGIC_64 else ">"
        hdr = 32
    elif magic in (MH_MAGIC_32, MH_CIGAM_32):
        endian = "<" if magic == MH_MAGIC_32 else ">"
        hdr = 28
    else:
        raise SystemExit(f"not a thin Mach-O (magic={magic:#x})")
    ncmds = struct.unpack(endian + "I", data[16:20])[0]
    off = hdr
    zeroed = 0
    for _ in range(ncmds):
        cmd, cmdsize = struct.unpack(endian + "II", data[off:off + 8])
        if cmd == LC_UUID:
            for i in range(off + 8, off + 8 + 16):
                data[i] = 0
            zeroed += 1
        if cmdsize == 0:
            break
        off += cmdsize
    open(path, "wb").write(data)
    return zeroed


if __name__ == "__main__":
    total = 0
    for p in sys.argv[1:]:
        total += zero_uuid(p)
    print(f"zeroed {total} LC_UUID")
