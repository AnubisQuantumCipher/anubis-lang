# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(= y (_ bv42 32))=FAIL
- `source_hash`: PASS - 77388d8485630e8701da513eda90d04df0db0cd8af53a7abce57538f2b8da3bf
- `build_log_hash`: PASS - 44fdc914e9fd7cefc4ba431a43209c31f8f428bba5072963d83b0dffdd1bbea2
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_guest.elf,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_generated_methods_rs_hash`: PASS - 618cf8571f68a84ea7a39330052abd896617840c4e788ce54125ebd03dc63b1e
- `hybrid_risc0_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_risc0_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_risc0_receipt_bin_hash`: PASS - 5a1e3b27d9a029a973762abca821ff308acc8dc411d7ca92bf381ec12725410d
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - 35767490adc0f61be359c76d8958b30841b9f927f9bc2918f56627bfe7873f15
- `hybrid_risc0_receipt_verify_log_hash`: PASS - 5ef0ff7e2a365f73c723ed438606a978c475ff224d4617a8dd9932cc3d7bec66
- `hybrid_risc0_prove_log_hash`: PASS - 32e478af9f8aeffb2536736de3d852b2f452e7489461645bc39bcfffc783a2e1
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - af76dac3218c7aaf0df7eba777f8972dc94e3af19333e3beb5a0c234aa251432
- `hybrid_risc0_receipt_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_risc0_metadata_json_hash`: PASS - 35767490adc0f61be359c76d8958b30841b9f927f9bc2918f56627bfe7873f15
- `hybrid_risc0_receipt_rs_hash`: PASS - a39dc51c2d25eae3d35ef47a4fd1cdf4718455308d16de5462155ae61670fb12
- `risc0_receipt_verify`: PASS - verify_status=passed fresh_receipt_generated=true dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false
