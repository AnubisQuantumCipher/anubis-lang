# Anubis Apple Native Surface

Anubis is Apple Silicon first for proof-carrying security work. The current
shipping surface is intentionally narrower than the long-term goal of building
any Apple artifact from Anubis source.

## Current Ready Surface

- macOS CLI workflows: `check`, `run`, `build`, `prove`, `doctor`,
  `capabilities`, `runtime-probe`, `runtime-plan`.
- RISC0 receipt generation and verification through the pinned
  `/Users/sicarii/Desktop/metal-hybrid-prover` reference.
- CPU and Metal-hybrid parity evidence on Apple Silicon through Gate 11.
- Runtime capability evidence through `runtime-probe`.
- Plan-only UMPG-style runtime DAG emission through `runtime-plan`.
- Ordinary safe Anubis execution through `run` for the supported safe subset.
- Evidence bundles with RISC0 sidecars, manifests, SARIF, and Markdown reports.

## ZirOS Technology Imported Now

- Machine-readable capability truth, modeled after ZirOS `support-matrix.json`.
- Fail-closed strict lanes: unsupported strict proof requests must fail rather
  than silently downgrading.
- RISC0 Metal-hybrid separation: observed/evidence-backed acceleration is not
  the same claim as ZirOS's formally verified Metal lane.
- Neural Engine/CoreML discipline: model output may become an advisory control
  plane, but it is never proof truth or authorization truth.
- UMPG runtime planning: `runtime-plan` emits a typed operation graph with
  device placement, dependency checks, weakest-link trust propagation, and
  tamper-checkable plan evidence.

## Not Yet Shipped

- Native SwiftUI/AppKit application emission.
- iOS, iPadOS, watchOS, tvOS, or visionOS application emission.
- A real UMPG scheduler/executor inside Anubis.
- CoreML or Neural Engine model execution.
- ZirOS verified Metal Lean/Verus proof artifacts carried into Anubis.
- iCloud Drive artifact lifecycle or Keychain-backed key management.

## Capability Command

```bash
cargo run --release -p anubis -- capabilities \
  --apple-native \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/apple_native_capabilities
```

This writes:

- `capabilities.json`
- `APPLE_NATIVE_CAPABILITIES.md`
- `MANIFEST.sha256`

The command is a contract for future Apple-native work. A target can move from
`planned` to `ready` only when source, runtime behavior, and evidence prove it.

## Runtime Probe Command

```bash
cargo run --release -p anubis -- runtime-probe \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/runtime_probe
```

This writes:

- `runtime-probe.json`
- `RUNTIME_PROBE.md`
- `MANIFEST.sha256`

The probe captures host/toolchain/RISC0/Metal capability evidence, including the
configured reference path, git identity when available, tree hashes, patch
activation, and observed lane metadata. It does not claim proof execution or
receipt verification.

## Runtime Plan Command

```bash
cargo run --release -p anubis -- runtime-plan examples/risc0_receipt.anb \
  --backend risc0 \
  --lane metal-hybrid \
  --apple-native \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/runtime_plan
```

This writes:

- `runtime-plan.json`
- `RUNTIME_PLAN.md`
- `MANIFEST.sha256`

The runtime plan is source-derived and validated through the current parser and
type checker. It describes how Anubis would route parse/typecheck/taint/symbolic
analysis, native lowering, RISC0 guest/image ID generation, proving, receipt
verification, and evidence bundling. It does not claim the proof ran; actual
proof truth still comes only from `risc0_zkvm::Receipt::verify(image_id)` plus
bundle validation.

## Ordinary Run Command

```bash
cargo run --release -p anubis -- run examples/hello_normal.anb \
  --evidence --out out/run_hello
```

This proves Anubis has a normal-language path: safe programs can execute without
RISC0, Metal, runtime-plan, or evidence unless requested. The current `run`
subset is intentionally partial and fails closed on unsupported constructs.
