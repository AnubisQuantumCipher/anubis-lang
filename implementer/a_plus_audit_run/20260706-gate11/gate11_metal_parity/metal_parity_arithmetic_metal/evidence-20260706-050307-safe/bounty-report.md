# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(= y (_ bv42 32))=FAIL
- `source_hash`: PASS - a520473f792ab92523fe7aea9c5ba83ae59bb674c5892a443b3b3af2c64473ba
- `build_log_hash`: PASS - 46f1d66013e3eaea5150b584439dc8bb56895ec55dad18e4b0ca24f0d662d84b
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_guest.elf,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_generated_methods_rs_hash`: PASS - dd0397ca511c07c3fc5d254dff074374100ef005fdd71e7ee4889981d731096b
- `hybrid_risc0_guest_elf_hash`: PASS - 1cf5c59ae284bf33ea3bc31d470c59efb83bf04cfb58d6477252dd2217e9fe3c
- `hybrid_risc0_image_id_txt_hash`: PASS - 7517466e75e0ee16e870281e9bd08152291435ad39e59d98968380516b9d5ad4
- `hybrid_risc0_receipt_bin_hash`: PASS - 57fd526c93e69bcf347fef07f2ad2bf6cc4cf7965360da66703d708fbc29e5c1
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - 68cedecb2cacded1802bacfb592235ac2e91ab1c6a7ca5a656a5bfa7cedc1774
- `hybrid_risc0_receipt_verify_log_hash`: PASS - f2625b11c5f275ff43aee3c7702f9b5cf95aeb471d9c24f444a8d3e82760c26d
- `hybrid_risc0_prove_log_hash`: PASS - 32e478af9f8aeffb2536736de3d852b2f452e7489461645bc39bcfffc783a2e1
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - af76dac3218c7aaf0df7eba777f8972dc94e3af19333e3beb5a0c234aa251432
- `hybrid_risc0_receipt_hash`: PASS - e352bf9749a70b59115b1e1298ca4731bac4540139ffea75b9e75e4cd7346c9b
- `hybrid_risc0_metadata_json_hash`: PASS - 68cedecb2cacded1802bacfb592235ac2e91ab1c6a7ca5a656a5bfa7cedc1774
- `hybrid_risc0_receipt_rs_hash`: PASS - a39dc51c2d25eae3d35ef47a4fd1cdf4718455308d16de5462155ae61670fb12
- `risc0_receipt_verify`: PASS - verify_status=passed fresh_receipt_generated=true dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false
