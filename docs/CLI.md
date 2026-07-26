# Anubis CLI (Ordinary Language Workflows)

Dev form (from repo root):

cargo run -- check <file.anb> [--evidence] [--emit ast,hir,mir] [--out DIR]
cargo run -- build <file>
cargo run -- run <file.anb> [--evidence --out DIR]
cargo run --release -p anubis -- prove <file> --backend risc0 --lane cpu|metal-hybrid \
  [--metal-reference /path/to/metal-hybrid-prover] [--evidence]
cargo run -- verify <bundle>            # (alias: validate) re-derive + tamper/signature check
cargo run -- verify-receipt --receipt <path> --image-id <path>
cargo run -- doctor
cargo run -- capabilities --apple-native --json [--evidence --out DIR]
cargo run -- entitlements <file.anb> [--out profile.json] [--plist program.entitlements]
cargo run -- runtime-probe --json [--evidence --out DIR] \
  [--metal-reference /path/to/metal-hybrid-prover]
cargo run -- runtime-plan <file.anb> --backend risc0 --lane cpu|metal-hybrid \
  --apple-native --metal-reference /path/to/metal-hybrid-prover --json [--evidence --out DIR]

Installed form (after cargo install or path):

anubis check ...
anubis prove ... --backend risc0
anubis doctor

Behavior:
- `check`: type/taint/policy/solver only; never emits a native artifact. A rejected check emits a
  timestamped `FAIL` evidence bundle automatically, even without `--evidence`; `--evidence` also
  requests a bundle for successful checks. Rejection PCAs use tier `rejected`, verdict `FAIL`, and
  carry the diagnostic—never a proof claim.
- `build`: verifies contracts by default, then emits a native artifact and optional evidence.
  `build --evidence` failures emit an artifact-free `FAIL` rejection bundle. `--no-verify` is an
  explicit escape hatch and emits only clearly marked `UNVERIFIED` evidence.
- `run`: verifies the same contracts as `check`/default `build` before native lowering, then executes
  ordinary Safe Anubis programs. Unsupported constructs fail with
  `ANUBIS_UNSUPPORTED_NATIVE_LOWERING`.
- `prove --backend risc0`: RISC0 receipt path (fresh, journal via verify-receipt).
- `doctor`: reports binary version, git, rustc, RISC0 versions, patched `risc0-circuit-rv32im` path + existence + Metal HAL, `R0_DISABLE_METAL` status, Apple Silicon, Tier-2, smoke checks, evidence scripts/schemas. Supports `--require-risc0`, `--require-metal`, `--metal-reference`, `--evidence`, `--json`.
- `capabilities --apple-native`: emits the machine-readable Apple-native capability matrix. It separates ready RISC0/Metal proof lanes from the plan-emitter-ready UMPG surface and planned CoreML/Neural Engine control-plane lanes.
- `entitlements <file.anb>`: derives a macOS App Sandbox / entitlement **profile** from the program's proven effect set (same spine as `vz confine`). Sealed into evidence as `entitlement_profile.json` + optional `program.entitlements` plist; re-derived on `verify` (forged permissive profiles fail closed). **Derived profile, not enforced until signed** — every key has `apple_enforced_claim: false`; codesign is `needs_human`.
- `runtime-probe`: emits capability evidence for host/toolchain/RISC0/Metal readiness. It does not claim proof execution or receipt verification.
- `runtime-plan`: parses and typechecks source, then emits a plan-only UMPG-style DAG with typed operations, dependencies, device placement, weakest-link trust policy, and the exact Metal reference path/config source. It is not receipt execution evidence.
- All commands accept `--metal-reference` (or env `ANUBIS_RISC0_METAL_REFERENCE`) for the risc0 metal-hybrid reference tree. Evidence records the config source.
- All support --evidence --out where applicable.
- Errors are human + include remediation where possible.

## Phase 7 — Developer experience

```bash
anubis doc <path> [--format md|json] [--private] [--out FILE]
anubis repl [--exact] [--allow-research] [--eval 'expr']
anubis lsp   # stdio Language Server (diagnostics + contract hovers)
```

- **doc:** verification-first API docs; **Contracts** section from source `requires`/`ensures`.
- **repl:** always typechecks (and discharges obligations) before eval; default = fast AST interpreter; `--exact` lowers via the same path as `anubis run`.
- **lsp:** `publishDiagnostics` from parse/typecheck/obligations; hover shows signature + contracts.
- Gate: `bash scripts/run_dx_gate.sh`
- Editors: `editors/vscode-anubis`, `editors/tree-sitter-anubis`

## Phase 6 — Packages + proof-carrying dependencies

```bash
anubis package lock --root .
anubis package verify --root .
anubis package publish --root . --key ./keys/signing.key
anubis trust add-signer <hex-pk> --name alice
anubis trust list
anubis keygen --out ./keys
anubis sign <evidence-dir> --key ./keys/signing.key
```

- `[dependencies]` in `Anubis.toml`: SemVer (local `~/.anubis/registry`), `{ path = ... }`, or
  `{ git = ..., rev = ... }` (rev required).
- `Anubis.lock` pins version + content Merkle hash. Cache: `~/.anubis/cache/<name>-<ver>-<sha>/`.
- Every dependency must present signed `evidence/`; signer must be in `~/.anubis/trust/signers.toml`.
- Unsigned deps only with **both** `--allow-unsigned-deps` and `ANUBIS_ALLOW_UNSIGNED_DEPS=1`.
- `check` / `run` / `build` automatically resolve and proof-check deps when declared.
- Full docs: `docs/language/PACKAGES.md`. Gate: `bash scripts/run_package_gate.sh`.

See also: scripts/ for runners and gate verifiers.

## Virtualization — `anubis vz` (Apple Virtualization.framework)

The full VM lifecycle on Apple Silicon, behind one CLI — stand up an isolated, reproducible guest to
run and seal code without leaving the `anubis` tool.

```bash
anubis vz status                                  # backend + Apple Silicon readiness + running VMs
anubis vz list [--json]                           # list VMs
anubis vz create dev --from ghcr.io/cirruslabs/macos-sonoma-base:latest --cpu 8 --memory 8192
anubis vz run dev --detach                        # boot headless in the background
anubis vz ip dev                                  # guest IP once booted
anubis vz exec dev --user admin -- uname -a       # run a command in the guest over SSH
anubis vz snapshot dev dev-clean                  # CoW clone as a snapshot
anubis vz sync dev --from ./workspace --to /Users/admin/workspace
anubis vz stop dev
anubis vz delete dev --force
```

**Disposable offensive lifecycle** — run dangerous code where its blast radius is a throwaway VM, never
the host. Gated behind `--allow-research`, like every dangerous Anubis operation; the guest is cloned
CoW, booted, fed the code, and discarded (unless `--keep`):

```bash
anubis vz exploit poc.anb --allow-research           # clone → boot → sync → `anubis run --allow-research` → discard
anubis vz fuzz poc_kit/bin/vuln_local --iterations 100000 --allow-research   # TARGET is a BINARY, not a .anb
```

- Backend: **tart** (Cirrus Labs' Virtualization.framework wrapper) — the same VZ layer the repo's
  `scripts/vm/run-slice.sh` seal battery uses. Install: `brew install cirruslabs/cli/tart`.
- Requires Apple Silicon macOS. A missing backend fails closed with `ANUBIS_VZ_BACKEND_MISSING`.
- A native `objc2-virtualization` FFI backend (no `tart`) is the documented next step; it needs the
  `com.apple.security.virtualization` entitlement + a signing identity (a human step).

## Gate 15 + Bounty-Grade PoC Kit

- `anubis vz exploit <poc.anb> --allow-research` executes packing + `target_run` in a disposable
  guest (see `docs/language/POC_KIT.md`); host `anubis run --allow-research` is refused.
- `anubis vz fuzz <local-binary> --iterations N --allow-research` runs the mutation process in a
  disposable guest; host `anubis fuzz` is refused. Guest evidence includes `fuzz_report.json` and
  `crashes/*.bin`; network targets remain forbidden.
- `anubis bounty-report <bundle> --out DIR` or `anubis report <bundle>` : Bounty evidence report from a bundle.
- PoC kit gate: `bash scripts/run_poc_kit_gate.sh --out out/poc_kit`
- Gold lab target: `bash poc_kit/build_vuln.sh` → `poc_kit/bin/vuln_local`

## Offensive Platform (AOP) — T1–T7

- `anubis engage-init / engage-status` — engagement + PSK + RBAC + mTLS certs
- `anubis listen --engage DIR` — HTTP+DNS+UDS C2, aop-2 encrypt, console at `http://127.0.0.1:4444/`
- `anubis agent-generate` — encrypted beacon agent (cargo-built)
- `anubis task-queue --module whoami` — queue agent task
- `anubis persist-launchagent --agent PATH` — LaunchAgent plist (T2)
- `anubis inject-plan --pid N --shellcode PATH` — inject plan only (T2)
- `anubis lateral-ssh --host H --cmd C` — scoped lateral (T4)
- `anubis pattern-create / pattern-offset / gadget-search / browser-harness` — ROP/browser (T5)
- `anubis pack-xor --input FILE` — lab packer (T6)
- `anubis exploit-new / exploit-run` — exploit modules
- `anubis module-list` / `offensive-doctor --json`
- Gate: `bash scripts/run_offensive_platform_gate.sh` (20 checks; host entrypoint runs in a disposable tart guest by default)

See `docs/language/OFFENSIVE_PLATFORM.md`.

All require proper `@` attributes with authorization for non-safe modes. Dangerous effects forbidden in `@safe`.

Mode classification is program-wide and source-order independent:
`Safe < Research < Exploit`. Any Research/Exploit function—including one nested in a module or
impl—elevates the command/evidence mode and makes ordinary `run` refuse before lowering.
An explicit `@safe` function inside that mixed program remains Safe and keeps all Safe-mode checks;
only unannotated functions inherit the aggregate program mode.

Run security fixtures:
bash scripts/run_security_fixtures.sh --out out/gate15_security_fixtures

Include in RC:
bash scripts/build_release_candidate.sh --metal-reference /path --require-metal --include-security --out out/rc


## Examples

### Ordinary safe run
```bash
cargo run --release -p anubis -- run examples/hello_normal.anb \
  --evidence --out out/run_hello
```

This writes `run-summary.json`, `stdout.txt`, `stderr.txt`, `RUN.md`, and
`MANIFEST.sha256`. It is ordinary native execution, not proof execution.

### Safe check + taint rejection
```bash
cargo run --release -p anubis -- check examples/safe_hello.anubis
cargo run --release -p anubis -- check examples/taint_reject.anb --evidence --out out/taint_reject
# expect ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY in diagnostics / bundle
```

### Declassify with policy
```bash
cargo run --release -p anubis -- check examples/policy_declassify_report.anb --evidence
```

### Symbolic
```bash
cargo run --release -p anubis -- check examples/symbolic_assert_pass.anb --evidence
cargo run --release -p anubis -- check examples/symbolic_assert_fail.anb --evidence
```

### RISC0 prove (CPU lane, portable)
```bash
cargo run --release -p anubis -- prove examples/risc0_receipt.anb \
  --backend risc0 --lane cpu \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --evidence --out out/risc0_cpu
# then
cargo run --release -p anubis -- verify-receipt \
  --receipt out/risc0_cpu/risc0_receipt.bin \
  --image-id out/risc0_cpu/risc0_image_id.txt
bash scripts/verify_bundle.sh out/risc0_cpu/evidence-*
```

### Metal lane (when hardware available)
```bash
cargo run --release -p anubis -- prove ... --lane metal-hybrid --metal-reference ...
```

### Doctor
```bash
cargo run --release -p anubis -- doctor
ANUBIS_RISC0_METAL_REFERENCE=/path cargo run --release -p anubis -- doctor --require-risc0 --json
cargo run --release -p anubis -- doctor --metal-reference /path --require-metal --evidence --out out/doctor
```

### Apple-native capabilities
```bash
cargo run --release -p anubis -- capabilities \
  --apple-native \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/apple_native_capabilities
```

The JSON contract is intentionally conservative:
- RISC0/Metal-hybrid is proof-bearing only through `Receipt::verify(image_id)` and observed lane evidence.
- UMPG currently has a runtime-plan emitter, not a shipped scheduler or executor.
- CoreML/Neural Engine is planned advisory infrastructure only and is never proof truth.
- macOS CLI and RISC0/Metal proof flows are current Apple-native surfaces; SwiftUI/iOS/visionOS emitters are planned.

### Runtime probe
```bash
cargo run --release -p anubis -- runtime-probe \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/runtime_probe
```

This writes `runtime-probe.json`, `RUNTIME_PROBE.md`, and `MANIFEST.sha256`.
Probe PASS means the requested local capabilities were observed enough for
planning. It is not a RISC0 receipt, not a verified proof, and not runtime-exec.

### Runtime plan (UMPG-style DAG)
```bash
cargo run --release -p anubis -- runtime-plan examples/risc0_receipt.anb \
  --backend risc0 \
  --lane metal-hybrid \
  --apple-native \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --json --evidence --out out/runtime_plan
```

This writes `runtime-plan.json`, `RUNTIME_PLAN.md`, and `MANIFEST.sha256`. The
plan includes parse, typecheck, taint, symbolic, lowering, RISC0 methods build,
prove, receipt verify, and evidence nodes for the RISC0 backend. `status` remains
`plan-only`; a PASS still requires actual receipt generation, verification, and
bundle validation.

### CPU vs Metal parity (Gate 11)
```bash
bash scripts/check_metal_parity.sh --require-metal --out out/gate11
jq . out/gate11/parity_report.json
```

### Verify bundle
```bash
cargo run --release -p anubis -- verify out/.../evidence-*   # (alias: validate)
```

### Independent portable evidence verify (host-side, no VZ)
```bash
# PCA bundle, engagement content_hash, receipt HMAC chain, run-cap MAC, confinement re-derive
anubis evidence-verify <path> [--json] [--pubkey HEX] [--run-cap-key KEY] [--strict]
# path may be: evidence bundle dir | engagement dir | run_capability.json
```
Honest labels: PCA re-derive `LAB_REAL`; receipt/run-cap MAC `LAB_REAL_HMAC` (not Ed25519).

### Security research domain packs
```bash
anubis research-pack list [--json]
anubis research-pack show poc|fuzz|crypto_research|bounty|emulation [--json]
anubis research-pack scaffold <id> --out DIR [--engagement-id ID]
anubis research-pack validate <id> --source program.anb [--json]
```
Per-capability honesty: `LAB_REAL` / `LAB_REAL_HMAC` / `PLAN_ONLY` / `PARTIAL` / `NOT_IMPLEMENTED`.  
Validate fails closed if proven effects are outside the pack allow-list.

### Crypto doctor (RWC surface inventory)
```bash
anubis crypto-doctor [--json]
```
Honest host-vs-guest backend table + non-claims (CAVP, PQ DIY, TLS/Noise). See `docs/language/RWC_LANGUAGE_MAP.md`.

## Config (portable)
- `--metal-reference PATH`
- `ANUBIS_RISC0_METAL_REFERENCE=PATH`
- `Anubis.toml` (see `Anubis.toml.example`)
- Evidence always records `config_source` + the exact `reference_path` used.

## Phase 8 — Self-host

```bash
anubis selfhost dump-tokens <file>
anubis selfhost dump-ast <file>
# stage0: host interprets SH compiler
anubis run selfhost/src/anubis_sh.anb --allow-research -- lex|parse|check|compile <file> [-o out.rs]
# stage packages: rustc out.rs && ./out compile selfhost/src/anubis_sh.anb -o stage2.rs
bash scripts/run_selfhost_gate.sh   # stage0→1→2→3 + cmp stage2/stage3
```
