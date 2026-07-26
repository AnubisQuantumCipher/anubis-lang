# CI Runner Boundary Repair — Final Evidence

Generated: 2026-07-26T04:59:36Z

Branch: `a-plus-maturity/safe-mode-trust-spine-20260725`

## Failure retained as evidence

The first Safe Mode trust-spine commit ran the full seal on stock GitHub macOS
and honestly failed G9 because Tart was not installed:

- push run `30182775554`
- pull-request run `30182787531`
- diagnostic: `ANUBIS_POC_KIT_VZ_REQUIRED: tart is not installed`

This exposed a runner-boundary defect: hosted CI was being asked to make a full
VZ claim it could not support.

## Repair

`.github/workflows/ci.yml` now separates two claims:

- Push/PR `hosted-gate-witness` runs
  `scripts/audit_unified.sh --profile hosted`. It can report only
  `HOSTED_PASS`: G9 is `EXTERNAL` and G14 is pinned to its non-executing 5/5
  host isolation witness.
- Manual `sealed-vz-gate-suite` requires a self-hosted macOS ARM64 runner
  labeled `tart-vz`. It runs the unchanged `scripts/audit_a_plus.sh` front door,
  including G9 PoC execution and the full G14 34-check offensive battery.

The default full profile passes only at exactly 15/15 with zero failures, skips,
or external gates. A `FAIL` report exits nonzero.

## Native and guest determinism

Generated native projects first build against Cargo's audited local cache in
offline mode. With `CARGO_HTTP_PROXY=http://127.0.0.1:9` and
`CARGO_NET_RETRY=0`, the Turing-core fixture gate passed 13/13 at:

`out/safe_mode_trust_spine_20260725/turing_offline_cache_recheck`

G9 and G14 sync the host-safe, freshly gate-built Apple-silicon release binary
into each disposable guest, recompute SHA-256 in the guest, and refuse execution
on a mismatch. Research, crash, fuzz, exploit, and offensive execution remains
guest-only.

Verified binary SHA-256:

`758636722a6ed4c75f35220c6697c19542fa4ca825314a0127f3849b9f4c922e`

## Exact-tree hosted witness

Evidence:
`implementer/a_plus_audit_run/20260726T043000Z/hosted_ci_profile`

Verdict:
`HOSTED_PASS (14/15 passed, 0 failed, 0 skipped, 1 external)`

Key boundary results:

- Rust tests: 828 passed
- language fixtures: 244/244
- G9: `EXTERNAL`
- G14: PASS 5/5, `host-isolation-witness`
- dogfood feel gate: 8/8

No research-capable operation ran on the host. This witness is not a full
maturity seal.

## Exact-tree full VZ seal

Evidence:
`implementer/a_plus_audit_run/20260726T050000Z/full_vz_seal`

Verdict:
`PASS (15/15 passed, 0 failed, 0 skipped, 0 external)`

Key results:

- Rust tests: 828 passed
- language fixtures: 244/244
- Turing core: 13/13
- PCA: 13/13
- PoC kit: 4/4 in disposable guest `anubis-poc-kit-gate-98280`
- prove: 11/11
- offensive platform: 34/34 in disposable guest
  `anubis-offensive-gate-1853`
- dogfood feel gate: 8/8

Both final VZ guests recorded
`binary_transport: host-built-arm64-hash-verified` and the verified binary hash
above.

## Claim boundary

This is unsigned local audit evidence unless a configured signing lane adds an
external signature. It proves the recorded gates for this exact source tree; it
is not a proof of total compiler soundness.

Independent A15 hostile reproduction remains pending. The pull request must
remain draft until A15 reproduces the claimed improvement with independent
commands and artifacts.
