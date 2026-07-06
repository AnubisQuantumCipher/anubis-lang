# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1
- `symbolic`: PASS - constraints=1
- `solver`: PASS - solver:no-obligations=PASS
- `source_hash`: PASS - 10b7e09d15366daadeb23803b074630da64a636d7f2b81c15369295ad751dace
- `build_log_hash`: PASS - 58ba9d14c58048aba62c55b197f9982a1e3bef067ba540c80922567f93802e53
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_guest.elf,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - deef7e372b548795c62c793b516d683fdbd0803c15a284d06307f39d6319d4dd
- `hybrid_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_generated_methods_rs_hash`: PASS - 6ffb3eb0e6cc53f6d76267364bfd2cc347f06eab49f5025ec5137811a64d93ad
- `hybrid_risc0_guest_elf_hash`: PASS - deef7e372b548795c62c793b516d683fdbd0803c15a284d06307f39d6319d4dd
- `hybrid_risc0_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_risc0_receipt_bin_hash`: PASS - fc20d057c53735f7a3c41d510a32384dadf2f49f99987d68b35bb00a15c2d434
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - ec2f45146e5e2835bcd0645d565f31783a9940c22756add3572b59cee440d023
- `hybrid_risc0_receipt_verify_log_hash`: PASS - 915eeecdd4403c5ab4c918f4e8b412e2269ba7afc1ff5094b3334b01dca97117
- `hybrid_risc0_prove_log_hash`: PASS - 904c7d0c20fa3a3235f765da07dd3518b11a21b1d9fb095d3aaf0dd9df7b8d81
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - af76dac3218c7aaf0df7eba777f8972dc94e3af19333e3beb5a0c234aa251432
- `hybrid_risc0_receipt_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_risc0_metadata_json_hash`: PASS - ec2f45146e5e2835bcd0645d565f31783a9940c22756add3572b59cee440d023
- `hybrid_risc0_receipt_rs_hash`: PASS - a39dc51c2d25eae3d35ef47a4fd1cdf4718455308d16de5462155ae61670fb12
- `risc0_receipt_verify`: PASS - verify_status=passed fresh_receipt_generated=true dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false patch_crates_io_active=true methods_patch_crates_io_active=true prover_patch_crates_io_active=true reference_ok=true vendor_ok=true
