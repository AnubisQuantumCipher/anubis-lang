# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1
- `symbolic`: PASS - constraints=1
- `solver`: PASS - solver:no-obligations=PASS
- `source_hash`: PASS - 10b7e09d15366daadeb23803b074630da64a636d7f2b81c15369295ad751dace
- `build_log_hash`: PASS - bc194972c5418b02c53af84a026367ef79ccc4fdf4bec1b6afd0a894d03ebe30
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
- `hybrid_image_id_txt_hash`: PASS - ed1de968dd8ab8d2f3d6659fb56c2e1cf98286621acc105d6ebe0e6a2703b430
- `hybrid_generated_methods_rs_hash`: PASS - e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
- `hybrid_risc0_image_id_txt_hash`: PASS - ed1de968dd8ab8d2f3d6659fb56c2e1cf98286621acc105d6ebe0e6a2703b430
- `hybrid_risc0_receipt_bin_hash`: PASS - 367131163c8eb3f6c129c87bc936e3df24cc909d2e1565c42d0b771425f79cb7
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - 0fb1b6525430325c0c08f582c951e35093680c649b4dd624ad511e37032cb60d
- `hybrid_risc0_receipt_verify_log_hash`: PASS - 85b73c45ed0fce484eb65d8ad1bd3564dc8b57ed74c7d8e3ef4025b0c97488bc
- `hybrid_risc0_prove_log_hash`: PASS - 45c9b3d4a3e0c936c3ed999422d5192bb4464610096fd04818ba6fd97f3f74d7
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - af76dac3218c7aaf0df7eba777f8972dc94e3af19333e3beb5a0c234aa251432
- `hybrid_risc0_receipt_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_risc0_metadata_json_hash`: PASS - 0fb1b6525430325c0c08f582c951e35093680c649b4dd624ad511e37032cb60d
- `hybrid_risc0_receipt_rs_hash`: PASS - a39dc51c2d25eae3d35ef47a4fd1cdf4718455308d16de5462155ae61670fb12
- `risc0_receipt_verify`: FAIL - verify_status=failed fresh_receipt_generated=false dev_mode=false mock_prover=false cache_used=false placeholder_image_id=true patch_crates_io_active=true methods_patch_crates_io_active=true prover_patch_crates_io_active=true reference_ok=true vendor_ok=true
