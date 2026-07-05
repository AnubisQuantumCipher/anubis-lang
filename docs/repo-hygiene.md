# Anubis Repo Hygiene

The canonical source tree is `/Users/sicarii/anubis-lang`.

Current cleanup policy:

- Keep generated Rust build outputs, Anubis output directories, evidence bundles, and local editor state out of version control via the root `.gitignore`.
- Preserve nested `.git` directories under `compiler/` and `tools/anubis/` until the operator explicitly approves flattening or deleting them.
- Do not publish crates, push tags, rewrite history, or create a public release without a fresh operator confirmation.
- Before a serious release, initialize the root history intentionally, audit `git status --ignored`, and commit only source, docs, tests, examples, and CI files.

Release-grade source should include:

- Root workspace manifests and lockfile.
- `compiler/` and `tools/anubis/` source.
- `vendor/risc0-circuit-rv32im/` because the full-hybrid backend pins the patched reference boundary.
- `examples/`, `docs/`, `.github/workflows/`, `.gitignore`, and README.
- No `target/`, `out-*`, generated evidence bundle, or local machine artifacts.
