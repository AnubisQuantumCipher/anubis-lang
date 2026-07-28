# Guest kernel provenance — aarch64 Linux for the native zero-NIC lane

Fetched 2026-07-28 for `anubis vz native-boot`. `VZLinuxBootLoader` requires an aarch64
kernel; Apple Silicon VZ does not emulate x86.

| file | bytes | sha256 |
|---|---|---|
| `vmlinuz-virt` | 9626112 | 749eb77d8c0a887868166c220e36411400b9bed5df6443b201c96950faf0f8ac |
| `initramfs-virt` | 8762564 | 6f48e46367737f1f223f2be3968945e4aeb0e7089f87386aee9da967c46d6269 |
| `config-virt` | 158288 | 2fd0d5014cea0560b215c36072b060d2c0695932789eabd6f0c8013806c8f126 |

Source: `https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/aarch64/netboot/`
Kernel: Alpine `6.12.81-0-virt` (the VM-optimized flavor, not `lts`).

## The checksums above are TRUST-ON-FIRST-USE, and that is a real limitation

Alpine publishes `.sha256` files for its ISOs and minirootfs tarballs. It publishes **none for
the netboot kernel or initramfs** — verified by listing the directory, not assumed. The hashes
here were computed locally from what the TLS fetch returned; they pin the artifact against
future substitution but do **not** attest that it is what Alpine built.

An early attempt to verify printed `MISMATCH pub=<html><title>404` — the comparison had
consumed an error page as if it were a checksum. That is the same shape as every other defect
this repo chases: a consumer accepting whatever a producer handed it without checking that it
was the KIND of thing expected. Any future re-fetch script must reject a non-hex "checksum"
rather than compare it.

## Verified properties (from the PUBLISHED config, not from assertion)

```
CONFIG_VIRTIO_FS=m      module — NOT built in
CONFIG_FUSE_FS=m        module — NOT built in
CONFIG_VIRTIO_PCI=y     built in
CONFIG_VIRTIO_CONSOLE=y built in
```

The round-25 plan asserted `CONFIG_VIRTIO_FS=y`. It is `=m`, and the difference is
load-bearing: Alpine's normal module delivery is `modloop`, fetched **over the network**,
which is exactly what a zero-NIC guest cannot do. Had the modules not been present locally the
staging design would have died on a network dependency discovered at boot.

They are present. `lib/modules/6.12.81-0-virt/kernel/fs/fuse/virtiofs.ko` and `fuse.ko` ship
inside `initramfs-virt` (130 `.ko` total), so `modprobe virtiofs` works with no network. The
conclusion held; the stated reason did not.

## What this unblocks, and what it does not

`CONFIG_VIRTIO_CONSOLE=y` is built in, so a guest can write to a serial console with **no
modules and no VirtioFS at all**. The two goals are therefore separable:

- **prove absence of a NIC from inside the guest** — needs console only
- **stage a PoC into the guest** — needs VirtioFS, hence the module load

The first does not depend on the second. A zero-NIC proof should not wait on file staging.


## The kernel Alpine ships cannot be booted by VZ, and the error says nothing

`vmlinuz-virt` is an **EFI zboot** image: the real kernel gzipped inside a PE/COFF wrapper,
`zimg` magic at offset 0x4. `file` calls it "PE32+ executable (EFI application) Aarch64", which
reads like success. `VZLinuxBootLoader` needs a RAW arm64 `Image` and rejects it with:

```
ANUBIS_VZNATIVE_START_FAILED: Internal Virtualization error. The virtual machine failed to start.
```

No mention of the kernel, the format, or the fix. The tell is the arm64 boot header magic at
offset 0x38, which must be `41524d64` (`ARM\x64`) and in the zboot file is not.

`scripts/vm/fetch_guest_kernel.sh` fetches, detects zboot, extracts the payload and verifies the
magic BEFORE publishing `Image-virt` — it refuses rather than writing a file that will fail the
same opaque way later. Measured: payload_offset `0xcbb8`, size 9564917, compression `gzip`,
decompressed 34668544 bytes with valid magic.

## Boot confirmed 2026-07-28

`Image-virt` + `initramfs-virt` boot to a shell under `VZLinuxBootLoader` with
`networkDevices=0`. Guest console reached `Run /bin/sh as init process` and a prompt. The probe
commands did not execute — stdin was pre-buffered and closed before `/bin/sh` opened it, so the
zero-NIC evidence is not yet captured from inside the guest. The BOOT is proven; the PROOF is
not, and those are recorded separately on purpose.
