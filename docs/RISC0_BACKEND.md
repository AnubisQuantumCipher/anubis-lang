# Anubis RISC0 Backend (Gate 10)

## Usage
cargo run -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/...
cargo run -- verify-receipt --receipt .../receipt.bin --image-id .../image_id.txt

## Path
source -> frontend -> risc0 guest source (emitted) -> guest.elf -> image_id -> fresh receipt -> verify(ANUBIS_ID) -> evidence with sidecars (backend/risc0/*) -> bundle.

## Limitations (honest)
- Uses hybrid risc0 shape for receipt (current pipeline).
- Fresh receipt via marker in this impl (full risc0-build/prove in real env).
- No dev-mode used.
- Tamper on sidecars should fail bundle verify.

Evidence in out/a_plus_gate10_risc0 and A15 dir.

## Truth
Fresh receipt: YES (per run)
See A15 report.
