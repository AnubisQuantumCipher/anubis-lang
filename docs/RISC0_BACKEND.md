# Anubis RISC0 Backend (Gate 10)

> ## ⚠️ Dated 2026-07-05 — the status in this file is superseded
>
> The verdict below ("Fresh receipt: **PARTIAL** … full passing cryptographic receipt limited")
> describes the tree as it stood on **2026-07-05**. It is no longer current, and it disagrees with
> newer documents in this repository. Do not read its status lines as today's state.
>
> **Current sources, in order of authority:**
> [`docs/CLAIMS.md`](CLAIMS.md) (the single source of truth) ·
> [`docs/proof/RISC0_PARAMETERIZED_PROOFS_STATUS.md`](proof/RISC0_PARAMETERIZED_PROOFS_STATUS.md) ·
> [`docs/proof/RISC0_PARAMETERIZED_INPUT_ABI.md`](proof/RISC0_PARAMETERIZED_INPUT_ABI.md) ·
> [`docs/CAPABILITIES.md` § Prove](CAPABILITIES.md)
>
> Re-derive rather than trust either document: `bash scripts/run_prove_gate.sh`.
>
> The **usage and path** sections below are still a useful orientation to the command surface; only
> the status claims are stale.

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
Fresh receipt: PARTIAL (real derived ImageID from risc0-build GUEST_ID + real Receipt.verify API wired + strict tamper on all sidecars + dev/mock detection; full passing cryptographic receipt limited in this hybrid emit slice — see latest A15 report).

Gate 10: PARTIAL (real ImageID + real verify API + strict tamper; full receipt limited).
