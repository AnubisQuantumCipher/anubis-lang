# Anubis Bounty Evidence Report

- mode: safe
- lane: risc0-risc0

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=2
- `symbolic`: PASS - constraints=1
- `solver`: PASS - solver:no-obligations=PASS
- `source_hash`: PASS - 99f113aa5f6536809f16b22619696aae8a0dc0822f8168af9f3a7bb89e515de3
- `build_log_hash`: PASS - 2be3879d90b98ae8c7e9470d3688d7f405f5e90550f17d9475c96aa008183685
- `artifact`: PASS - native emitted
- `artifact_hash`: PASS - 741ef1a85c4be5864a022d9f6c7b2d99a88bffa139cf03c17d5fb1f5d9235606
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs,risc0_guest.elf,risc0_image_id.txt,risc0_receipt.bin,risc0_risc0_metadata.json,risc0_receipt.verify.log,risc0_prove.log,risc0_guest_src_main.rs,risc0_receipt,risc0_metadata.json,risc0_receipt.rs
- `hybrid_guest_elf_hash`: PASS - 246575e6ab618980d7a8158a94ae89669eb1dffd6d569da708df1d39eb3ecc27
- `hybrid_image_id_txt_hash`: PASS - a096eb35bee0ce51dbc236f151295a7f0b7bdc6f281fc8b894b9ef39eabf3842
- `hybrid_generated_methods_rs_hash`: PASS - 2cc3dd0b3c2f3a87fca984fad9360ca2670d8108731d40bd483d1b1fc9c291b0
- `hybrid_risc0_guest_elf_hash`: PASS - 246575e6ab618980d7a8158a94ae89669eb1dffd6d569da708df1d39eb3ecc27
- `hybrid_risc0_image_id_txt_hash`: PASS - a096eb35bee0ce51dbc236f151295a7f0b7bdc6f281fc8b894b9ef39eabf3842
- `hybrid_risc0_receipt_bin_hash`: PASS - 8adb7bbc288970db5e3766a2c7e3e938ca6aa1ba14525997afda91ca433c492a
- `hybrid_risc0_risc0_metadata_json_hash`: PASS - f04873b4f9063b4bbba060a70858e4ccb32ec94821f0e08a9ab115732c5db9a3
- `hybrid_risc0_receipt_verify_log_hash`: PASS - 536c99c89a6ebf9185d371f9faf23242b9b976fff5389d9562a4d43849d6da5d
- `hybrid_risc0_prove_log_hash`: PASS - 32e478af9f8aeffb2536736de3d852b2f452e7489461645bc39bcfffc783a2e1
- `hybrid_risc0_guest_src_main_rs_hash`: PASS - 4fa9d3f5972a67bedcb6a6354ed2b52422dc32bf217f889854e70cf98e91d634
- `hybrid_risc0_receipt_hash`: PASS - 741ef1a85c4be5864a022d9f6c7b2d99a88bffa139cf03c17d5fb1f5d9235606
- `hybrid_risc0_metadata_json_hash`: PASS - f04873b4f9063b4bbba060a70858e4ccb32ec94821f0e08a9ab115732c5db9a3
- `hybrid_risc0_receipt_rs_hash`: PASS - e0288511e1350c63d5ea77123241553d43c43e5a75c1847117a2c317bc0fd8cf
- `risc0_receipt_verify`: PASS - verify_status=passed fresh_receipt_generated=true dev_mode=false mock_prover=false cache_used=false placeholder_image_id=false patch_crates_io_active=true methods_patch_crates_io_active=true prover_patch_crates_io_active=true reference_ok=true vendor_ok=true
