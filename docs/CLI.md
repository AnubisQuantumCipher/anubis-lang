# Anubis CLI (Ordinary Language Workflows)

Dev form (from repo root):

cargo run -- check <file.anb> [--evidence] [--emit ast,hir,mir] [--out DIR]
cargo run -- build <file>
cargo run -- run <file>   # (or documented shim / interpreter path)
cargo run --release -p anubis -- prove <file> --backend risc0 --lane cpu|metal-hybrid \
  [--metal-reference /path/to/metal-hybrid-prover] [--evidence]
cargo run -- verify-bundle <bundle>
cargo run -- verify-receipt --receipt <path> --image-id <path>
cargo run -- doctor

Installed form (after cargo install or path):

anubis check ...
anubis prove ... --backend risc0
anubis doctor

Behavior:
- `check`: type/taint/policy/solver only; does not emit native by default. With --evidence emits full bundle + *.ast.json etc.
- `build`: emits artifact (native) + optional evidence.
- `prove --backend risc0`: RISC0 receipt path (fresh, journal via verify-receipt).
- `doctor`: reports binary version, git, rustc, RISC0 versions, patched `risc0-circuit-rv32im` path + existence + Metal HAL, `R0_DISABLE_METAL` status, Apple Silicon, Tier-2, smoke checks, evidence scripts/schemas. Supports `--require-risc0`, `--require-metal`, `--metal-reference`, `--evidence`, `--json`.
- All commands accept `--metal-reference` (or env `ANUBIS_RISC0_METAL_REFERENCE`) for the risc0 metal-hybrid reference tree. Evidence records the config source.
- All support --evidence --out where applicable.
- Errors are human + include remediation where possible.

See also: scripts/ for runners and gate verifiers.

## Gate 15 Security Superpowers Commands (in progress)

- `anubis fuzz <harness.anb> --runs N --evidence --out DIR` : Local sandboxed fuzz V1, produces fuzz_report.json and evidence.
- `anubis bounty-report <bundle> --out DIR` or `anubis report <bundle> --format bounty` : Generates bounty-report.md, .json, checks.sarif, reproduction.md, scope.json.
- `anubis harness new <kind> --out file.anb` : Generates safe @fuzz harness template (kinds: bytes, json, etc. local only).

All require proper @ attributes with authorization for non-safe modes. Dangerous effects forbidden in @safe.

Run security fixtures:
bash scripts/run_security_fixtures.sh --out out/gate15_security_fixtures

Include in RC:
bash scripts/build_release_candidate.sh --metal-reference /path --require-metal --include-security --out out/rc


## Examples

### Safe check + taint rejection
```bash
cargo run --release -p anubis -- check examples/safe_hello.anb
cargo run --release -p anubis -- check examples/taint_reject.anb --evidence --out out/taint_reject
# expect ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY in diagnostics / bundle
```

### Declassify with policy
```bash
cargo run --release -p anubis -- check examples/declassify_policy_pass.anb --evidence
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

### CPU vs Metal parity (Gate 11)
```bash
bash scripts/check_metal_parity.sh --require-metal --out out/gate11
jq . out/gate11/parity_report.json
```

### Verify bundle
```bash
cargo run --release -p anubis -- verify-bundle out/.../evidence-*
```

## Config (portable)
- `--metal-reference PATH`
- `ANUBIS_RISC0_METAL_REFERENCE=PATH`
- `Anubis.toml` (see `Anubis.toml.example`)
- Evidence always records `config_source` + the exact `reference_path` used.

