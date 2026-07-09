# Gate 11 Metal parity — verification summary (2026-07-09)

## Goal
Prove CPU vs Metal-hybrid RISC0 parity on **distinct** lanes with program-derived guests,
without same-dir false-green or ignored sealer exit.

## What changed (honesty)
- Fixtures commit real program outputs (`return 42` / `x*6` / intermediate `y`) — not a leftover fixed circuit comment.
- `scripts/check_metal_parity.sh`: distinct `*_cpu`/`*_metal` outs; same-path check; sealer exit not ignored under `--require-metal`.
- `gate11-metal-parity` sealer: requires `paths_distinct` for journal match / PASS.
- `scripts/gate11_a15_reproduce.sh`: full checker, exits nonzero on fail (no `|| true` on sealer).

## Evidence run
```text
bash scripts/check_metal_parity.sh --require-metal --out out/a_plus_gate11_parity_continue
# overall_verdict=PASS  seal_rc=0
# host: macos/aarch64, tier2_metal_available=true
```

| Fixture | CPU lane | Metal lane | Journal | ImageID match | paths_distinct | Verdict |
|---------|----------|------------|---------|---------------|----------------|---------|
| metal_parity_hello | cpu | metal-hybrid | `2a000000` (42) | yes | yes | PASS |
| metal_parity_arithmetic | cpu | metal-hybrid | `2a000000` (42) | yes | yes | PASS |
| metal_parity_symbolic_safe | cpu | metal-hybrid | `2a000000` (42) | yes | yes | PASS |

- Shared journal sha256: `e8a4b2ee7ede79a3afb332b5b6cc3d952a65fd8cffb897f5d18016577c33d7cc`
- Three **distinct** ImageIDs across fixtures (program-bound guests).
- Artifacts: this directory + `out/a_plus_gate11_parity_continue/parity_report.json`

## Claims
- **REAL (local Apple Silicon Tier-2):** full Gate 11 PASS with observed metal-hybrid.
- **NOT CLAIMED:** hosted CI Metal, third-party reproduction, performance superiority.

## Non-claims / residual
- Does not claim Metal is faster.
- Hosted runners without Metal must use CPU lane only.
