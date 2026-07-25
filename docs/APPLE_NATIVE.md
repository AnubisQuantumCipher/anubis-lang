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
- Keychain / Secure Enclave **hardware** binding of linear capability tokens (static
  non-exportable mint/export is shipped; SE isolation is residual).

## Effect-derived entitlement profile

Parallel to `anubis vz confine` (hypervisor grants from the proven effect set), the language
also derives an **OS-facing** App Sandbox / entitlement profile:

```bash
anubis entitlements examples/hello.anb --out out/entitlement_profile.json \
  --plist out/program.entitlements
```

What is claimed:

- Pure function of source → deterministic `anubis.entitlements.v1` JSON.
- Sealed into every `anubis build --evidence` bundle as `entitlement_profile.json` (+
  `program.entitlements` plist).
- Re-derived on `anubis verify`; forged permissive keys fail closed (`ANUBIS_ENTITLEMENT_DRIFT`).
- Net-free programs do **not** enable `com.apple.security.network.client`; open effect sets
  use restrictive defaults.

What is **not** claimed (residual honesty):

- **Derived profile, not enforced until signed.** Every key has
  `apple_enforced_claim: false`. Host enforcement requires codesign with the generated
  entitlements and App Sandbox enablement (`needs_human`).
- Toolchain VZ entitlement (`com.apple.security.virtualization`) is intentionally **not**
  mixed into the language-derived app profile.
- Path-level sandbox allow-lists are residual.

## Non-exportable linear capabilities (static)

Language-level dual of “secrets do not leave” for **authority tokens**:

```text
let s = cap_acquire_nonexportable("fs.write");  // may authorize by causal spend
send("h", 80, "payload");                         // OK if kind matches (token not an arg)
print(s);                                         // ANUBIS_CAPABILITY_EXPORT
let e = cap_export(s, "audit reason");            // peel (string-literal reason)
print(e);                                         // OK after export
```

Claimed: static non-exportable mint, export sinks, well-formed peel.  
**Not claimed:** Keychain item create/lookup, Secure Enclave hardware isolation, interprocedural sealedness.

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

## Code-signing & the "do I need an Apple Developer account?" question

**Short answer: no. No user — building, installing, or downloading Anubis — needs an
Apple ID or an Apple Developer account.**

The entire core — `check`, `run`, `build`, `prove`, contracts, secrets, taint, and the
**tart** VM lane (`anubis vz create` / `exec` / `snapshot` / …) — needs **no
code-signing at all**.

Only the **native VZ backend** (`anubis vz native-preflight`, the direct
Virtualization.framework air-gap) needs the binary signed with **one** entitlement,
`com.apple.security.virtualization` — and that entitlement is **unrestricted**:

| Path | What it takes | Apple account needed? |
|---|---|---|
| Core language + the tart VM lane | just build and run | **none** |
| Native VZ lane, built from source | a **local ad-hoc signature** — `scripts/build_signed_anubis.sh` runs `codesign --sign -` with the entitlement, no portal, no provisioning profile | **none** |
| Distributing a **pre-built** binary to others | the *publisher* notarizes once with a Developer ID so Gatekeeper is happy on download | only the **publisher, once** — never the downloader |

Details that make this a non-issue:

- `com.apple.security.virtualization` is **not** a developer-portal entitlement. An
  ad-hoc signature (`codesign --sign -`) applies it and it works on your own Mac. The
  *restricted* VM entitlement, `com.apple.vm.networking` (which would need Apple's
  approval), is **not used** anywhere in Anubis.
- Building with a plain `cargo build` and then trying the native lane **fails closed**
  with a precise message telling you to run `scripts/build_signed_anubis.sh` — it never
  silently degrades. This was verified firsthand: the same binary is rejected without
  the entitlement and instantiates a VM with it.
- Signing with a real "Apple Development" identity (`--identity "Apple Development: …"`)
  also works, but that is a *convenience*, not a requirement.

So: clone-and-build users run `scripts/build_signed_anubis.sh` (ad-hoc, no account) and
the native lane works; download-a-release users just run the binary you notarized once.
Neither ever links their own Apple ID.
