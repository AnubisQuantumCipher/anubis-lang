# Anubis Metal Backend (RISC0 Hybrid Lane)

**Gate 11 scope**: Prove that the same Anubis source using `--backend risc0` can safely target either the CPU lane (`R0_DISABLE_METAL=1`) or the Metal-hybrid lane (Tier-2 Apple Silicon) and produce:

- Identical ImageID (real, from guest ELF via risc0-build)
- Identical journal / committed outputs
- Receipts that both pass real `risc0_zkvm::Receipt::verify(IMAGE_ID)`
- Evidence that records `lane_observed` from logs/runtime (not host inference)
- Hashed sidecars + mechanical tamper detection on the parity bundle

## Non-Claims (honest boundaries)
- This is **not** a claim that "Metal is faster".
- Performance numbers belong to the reference repo only.
- No third-party reproduction claim.
- Hosted CI cannot validate the Metal lane (no Apple Silicon Tier-2); CI uses CPU lane only.
- Local Apple Silicon validation (this A15 run) is the only "REAL" Metal observation for Gate 11.

## How to Drive Lanes
```bash
# CPU (comparison)
R0_DISABLE_METAL=1 cargo run --release -p anubis -- prove examples/metal_parity_hello.anb \
  --backend risc0 --lane cpu --evidence --out out/cpu_hello

# Metal-hybrid (when Tier-2 present)
cargo run --release -p anubis -- prove examples/metal_parity_hello.anb \
  --backend risc0 --lane metal-hybrid --evidence --out out/metal_hello
```

The `--lane` flag (TASK 4) sets/clears `R0_DISABLE_METAL` before the in-process prove child. The child and post-processing emit `lane_observed` into `receipt.verify.log` and `risc0_metadata.json`.

## Observation Rule (strict)
`lane_observed` is written as:
- "cpu" only when `R0_DISABLE_METAL` was set or logs explicitly say so.
- "metal-hybrid" **only if logs prove it** (e.g. "lane_observed=metal-hybrid", "metal-hybrid lane selected", Tier-2 probe success in the HAL).
- "unknown" otherwise → Gate 11 PARTIAL.

Never infer from `uname -m` or "this is a Mac".

## Evidence + Tamper
`scripts/check_metal_parity.sh` produces the report + bundles. The top-level evidence-*/ contains:
- CPU and metal-hybrid sidecars (guest.elf, receipt.bin, image_id.txt, risc0_metadata.json, logs)
- parity_report.json + metal_parity_report.md
- manifest with hashes

`verify_bundle.sh` + schema checks + 5-pattern tamper (receipt, id, metadata, parity report, ...) must all FAIL mechanically on modified trees.

## Version Envelope (locked for Gate 11)
risc0-zkvm = 3.0.5
risc0-zkp = 3.0.4
risc0-circuit-rv32im = 4.0.4 (via patch from /Users/sicarii/Desktop/metal-hybrid-prover/vendor/...)

In-process only (`get_prover_server` + `ProverOpts` + real verify). No external r0vm.

## Reference
See docs/METAL_BACKEND_PIPELINE_MAP.md for the full contract, file locations, CI vs local limits, and limitations (Tier-2 only, circuit kernels, no full GPU port, etc.).

See docs/RISC0_METAL_HYBRID_REFERENCE.md for the pinned source repo and validation steps.

## Maturity Claim
- CPU lane (R0 path): REAL
- Metal-hybrid lane: REAL **only when observed** on Tier-2 Apple Silicon local run
- Receipt verification (both): REAL only when both pass real verify
- Output parity: REAL only when hashes match
- All other claims (speed, third-party, CI Metal): NOT CLAIMED

This document + the A15 report + parity_report.json + tamper logs are the durable evidence for Gate 11.
