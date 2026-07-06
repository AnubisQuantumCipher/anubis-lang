# Gate 10 RISC0 - Real Receipt Achieved (2026-07-06)

**Status: Stronger, with real cryptographic receipt possible**

When using release binary (or `cargo run --release`), with the patch from the complete reference implementation at `/Users/sicarii/Desktop/metal-hybrid-prover`, and the child isolation + clean spawn:

- Real Image ID derived from guest ELF: YES
- Real receipt.bin produced ( ~209k )
- `receipt.verify(ANUBIS_ID)` PASSES with the real RISC0 API
- `risc0_receipt_verify` check in evidence: PASS
- `fresh_receipt_generated: true`
- `methods_build_success: true`
- No placeholder

`verify-receipt` standalone succeeds.

`verify_bundle.sh` may still report overall FAIL because the minimal fixture does not satisfy all other evidence checks (taint/solver/etc.), but the RISC0-specific part is now a real PASS.

The previous "always SIGBUS in prove runs" was largely due to using debug binary or unclean spawn. With release + improvements, a real receipt is achievable.

**Still note the user's point:** The overall Gate 10 for a full "fresh local RISC0 proof that makes the entire bundle PASS" may have additional requirements, and Metal work should wait for unambiguous cryptographic PASS on the RISC0 path.

Artifacts in this dir and proof_bundle/.
