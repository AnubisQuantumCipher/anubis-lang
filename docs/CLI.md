# Anubis CLI (Ordinary Language Workflows)

Dev form (from repo root):

cargo run -- check <file.anb> [--evidence] [--emit ast,hir,mir] [--out DIR]
cargo run -- build <file>
cargo run -- run <file>   # (or documented shim / interpreter path)
cargo run --release -p anubis -- prove <file> --backend risc0 [--evidence]
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
- `doctor`: reports Rust toolchain, RISC0 env, metal-hybrid ref, Apple Silicon/Metal if relevant, evidence scripts, git branch/status.
- All support --evidence --out where applicable.
- Errors are human + include remediation where possible.

See also: scripts/ for runners and gate verifiers.
