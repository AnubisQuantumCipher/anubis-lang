# Installing and Running Anubis (Local Release-Candidate)

## Build from source (recommended for this host)

```bash
cd /path/to/anubis-lang
cargo build --release -p anubis
./target/release/anubis --version
./target/release/anubis doctor --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover
```

The binary is at `./target/release/anubis`.

## Optional local install (symlink)

```bash
mkdir -p "$HOME/.local/bin"
ln -sf "$(pwd)/target/release/anubis" "$HOME/.local/bin/anubis"
export PATH="$HOME/.local/bin:$PATH"
anubis --version
```

A helper script skeleton lives at `scripts/install_local.sh` (copy/adapt as needed).

## Uninstall

```bash
rm -f "$HOME/.local/bin/anubis"
# (optional) cargo uninstall if you used `cargo install`
```

## Version

```bash
anubis --version
# or
cargo run --release -p anubis -- --version
```

## Using the portable metal reference

```bash
# CLI flag (highest precedence)
anubis prove foo.anb --backend risc0 --lane cpu \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover

# Env
ANUBIS_RISC0_METAL_REFERENCE=/Users/sicarii/Desktop/metal-hybrid-prover \
  anubis doctor --require-risc0

# Anubis.toml (see Anubis.toml.example)
```

Evidence bundles always record which source was used (`config_source`).

## Running the full release-candidate builder (after changes)

```bash
bash scripts/build_release_candidate.sh \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --require-metal \
  --out out/release_candidate
```

See the generated `RELEASE_CANDIDATE_REPORT.md` + `release_candidate.json` + `MANIFEST.sha256`.

## Verification after install

```bash
anubis doctor --metal-reference /path --require-risc0 --require-metal --json
```

All sealed gates + language fixtures (25/25) must still pass.
