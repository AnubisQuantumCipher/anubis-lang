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
- `hybrid_receipt_artifacts`: PASS - guest.elf,image_id.txt,generated-methods.rs
- `hybrid_guest_elf_hash`: PASS - 7888038e2eea92a7f2360efedb0c8b581f81bd97f581e471e3a669f98abbb0b4
- `hybrid_image_id_txt_hash`: PASS - ed1de968dd8ab8d2f3d6659fb56c2e1cf98286621acc105d6ebe0e6a2703b430
- `hybrid_generated_methods_rs_hash`: PASS - d8ebd11d9446d94ec93bc9f32acd8e95aa4b05080c9426e43ee8f17b04074822
