# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(= y (_ bv42 32))=FAIL
- `source_hash`: PASS - a520473f792ab92523fe7aea9c5ba83ae59bb674c5892a443b3b3af2c64473ba
- `build_log_hash`: PASS - 58ba9d14c58048aba62c55b197f9982a1e3bef067ba540c80922567f93802e53
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_guest.elf,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_generated_methods_rs_hash`: PASS - adcacf3a81f97246f04d0e2e18ffc03d12b3f61d26a291da8a0dd305ad894626
- `hybrid_risc0_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_risc0_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_risc0_receipt_bin_hash`: PASS - 367131163c8eb3f6c129c87bc936e3df24cc909d2e1565c42d0b771425f79cb7
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - 00f07b63ab23997d02c75e3bb3b60ec7efbe16f61ace3079010183483dc32ee3
- `hybrid_risc0_receipt_verify_log_hash`: PASS - 8c589a8cc2dc6c5041c82a0c1314bb329ff8f3926293e67427ebdd94bc4ba09f
- `hybrid_risc0_prove_log_hash`: PASS - b6b7ca8a3a74f5aa1c28bdb4f83928d3998e4ef57aab68a5640616aacec49f95
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - af76dac3218c7aaf0df7eba777f8972dc94e3af19333e3beb5a0c234aa251432
- `hybrid_risc0_receipt_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_risc0_metadata_json_hash`: PASS - 00f07b63ab23997d02c75e3bb3b60ec7efbe16f61ace3079010183483dc32ee3
- `hybrid_risc0_receipt_rs_hash`: PASS - a39dc51c2d25eae3d35ef47a4fd1cdf4718455308d16de5462155ae61670fb12
- `risc0_receipt_verify`: FAIL - verify_status=failed fresh_receipt_generated=false dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false
