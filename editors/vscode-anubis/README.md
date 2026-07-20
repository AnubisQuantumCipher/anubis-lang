# Anubis for VS Code

Verification-first editor support for `.anb` / `.anubis` / `.anub`.

## Features

| Feature | Source |
|---------|--------|
| Syntax highlighting | TextMate grammar `syntaxes/anubis.tmLanguage.json` |
| Comments / brackets | `language-configuration.json` |
| Diagnostics | `anubis lsp --stdio` (spawned by the VS Code language client) → parse + typecheck + `check_obligations` |
| Hover | Signature + **Contracts** (`requires` / `ensures`) |

## Install (dev)

```bash
# 1) Build the language server
cd /path/to/anubis-lang
cargo build --release -p anubis
export PATH="$PWD/target/release:$PATH"
which anubis && anubis lsp --help 2>/dev/null || anubis --help | grep lsp

# 2) Install extension deps
cd editors/vscode-anubis
npm install

# 3) Launch Extension Development Host
#    VS Code → Run and Debug → "Extension" or:
code --extensionDevelopmentPath="$PWD" /path/to/anubis-lang
```

Open any `tests/fixtures/dx/*.anb` file. You should see:

- Keyword/type highlighting
- Red squiggles on type errors (try `let x: u32 = true;`)
- Hover over a contracted `fn` name → Contracts section

## Settings

| Setting | Default | Meaning |
|---------|---------|---------|
| `anubis.lspPath` | `anubis` | Path to the `anubis` binary that supports `anubis lsp` |

## Not in MVP

Completions, rename, go-to-definition, debugger — see `docs/language/UNSUPPORTED.md`.

## Validate without a GUI

From repo root:

```bash
python3 scripts/test_lsp_roundtrip.py
bash scripts/run_dx_gate.sh
node -e "JSON.parse(require('fs').readFileSync('editors/vscode-anubis/package.json'))"
node -e "JSON.parse(require('fs').readFileSync('editors/vscode-anubis/syntaxes/anubis.tmLanguage.json'))"
```
