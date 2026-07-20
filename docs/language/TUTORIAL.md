# Anubis tutorial

This is the **adoption path** for Phase 7. It is verification-first: you learn by
running `check`, reading **Contracts** from the AST, and only then executing.
Samples under `tests/fixtures/dx/` are exercised by `scripts/run_dx_gate.sh`.

**Authority map**

| Document | Role |
|----------|------|
| This tutorial | How to *use* the language day to day |
| [`LANGUAGE.md`](../../LANGUAGE.md) | Full language reference (syntax + runtime) |
| [`SPEC.md`](SPEC.md) | Normative sketch (modes, evidence, contracts, packages, DX) |
| [`UNSUPPORTED.md`](UNSUPPORTED.md) | Honest non-claims |
| [`CLI.md`](../CLI.md) | Command surface |

---

## 1. Install and hello

From the repo root:

```bash
cargo build --release -p anubis
export PATH="$PWD/target/release:$PATH"
anubis --version
```

Hello world (fixture `tests/fixtures/dx/hello.anb`):

```anubis
fn main() {
    print("hello, anubis");
}
```

Always **check before run**:

```bash
anubis check tests/fixtures/dx/hello.anb
anubis run tests/fixtures/dx/hello.anb
```

`check` typechecks, tracks taint in Safe mode, and discharges solver obligations when
present. `run` only proceeds for programs that lower to the native safe subset (or
research path with `--allow-research`).

---

## 2. Modes and taint (Safe by default)

Anubis is dual-mode:

- **Safe (default):** `tainted` data must not reach sinks without an explicit
  `declassify(policy, reason)`. Leak fixtures fail closed.
- **Research / exploit:** intentional dual-use surface for authorized lab work;
  requires `--allow-research` on CLI paths that execute research constructs.

```bash
# expected FAIL — demonstrates Safe-mode taint enforcement
anubis check tests/fixtures/stdlib/io_leak.anb
```

If you are writing ordinary applications, stay in Safe mode. If you are building a
local PoC kit, read `POC_KIT.md` and the offensive-platform docs — never run research
against unauthorized targets.

---

## 3. Contracts: `requires` / `ensures`

Contracts are **source truth**, not prose. They attach to functions and are consumed by
the solver, the doc renderer, and LSP hover.

```anubis
// Integer division with a nonzero divisor
pub fn div(a: u32, b: u32) -> u32 requires(b != 0) ensures(result == a / b) {
    return a / b;
}
```

Fixture: `tests/fixtures/dx/contracts_doc.anb`.

### Verification-first docs

```bash
anubis doc tests/fixtures/dx/contracts_doc.anb
```

Output includes a **### Contracts** section built from AST `requires`/`ensures`, not
from free-form markdown claims. Options:

```bash
anubis doc path/to/file.anb --format md
anubis doc path/to/file.anb --format json
anubis doc path/to/file.anb --private          # include private fns
anubis doc path/to/file.anb --out docs/api.md
```

Doc comments (`//` immediately above a function) are preserved by the lexer and
attached via `associate_docs` for the prose blurb under each heading.

---

## 4. Types, control flow, and the executable core

The language is Turing-complete at runtime: loops, mutation, recursion, lists, maps,
structs, enums + `match`. Full grammar: `LANGUAGE.md`.

Minimal shape:

```anubis
fn factorial(n: u32) -> u32 requires(n >= 0) {
    if n == 0 {
        return 1;
    }
    return n * factorial(n - 1);
}

fn main() {
    print(factorial(5));
}
```

```bash
anubis check path/to/file.anb
anubis run path/to/file.anb
```

---

## 5. Standard library

```bash
anubis run tests/fixtures/stdlib/math_collections.anb
```

- Core collections / math: `docs/language/STDLIB_CORE.md`
- Crypto surface (audited host crates): `docs/language/CRYPTO.md`
- Import form: `import std.crypto;` (and related modules as documented)

---

## 6. Packages (Phase 6)

Dependencies are **proof-carrying**. A lock file pins content hashes; consumers re-verify
signed evidence before mounting dep modules.

```bash
anubis package lock --root .
anubis package verify --root .
```

See `docs/language/PACKAGES.md`. Gate: `bash scripts/run_package_gate.sh`.

---

## 7. REPL

The REPL is **check-first**: every entry is parsed, typechecked, and obligation-checked
before evaluation.

```bash
# one-shot expression (wrapped as main + print)
anubis repl --eval '2 + 3'
# → 5

# interactive
anubis repl
anubis> 1 + 1
anubis> :quit

# exact fidelity: lower via the same native path as `anubis run`
anubis repl --exact --eval '2 + 3'
```

- **Default:** fast AST interpreter (`compiler/src/interp`) for snappy exploration.
- **`--exact`:** incremental compile through `anubis run` lowering when you need
  production fidelity (lists of structs, full native builtins, etc.).
- Type errors fail closed (non-zero exit on `--eval`):

```bash
anubis repl --eval 'let x: u32 = true'   # fails
```

---

## 8. Editor / LSP

```bash
anubis lsp   # stdio JSON-RPC; used by the VS Code extension
```

What the language server does today:

| Feature | Status |
|---------|--------|
| Diagnostics from parse + typecheck + `check_obligations` | REAL |
| Hover: signature + **Contracts** | REAL |
| Completions, rename, debugger | OUT (see UNSUPPORTED) |

### VS Code

1. Open `editors/vscode-anubis` as an extension development host, or package it.
2. Ensure `anubis` is on `PATH`, or set `anubis.lspPath` in settings.
3. Open a `.anb` / `.anubis` file — TextMate highlighting + LSP diagnostics/hovers.

### tree-sitter

`editors/tree-sitter-anubis` is a **highlight-oriented** grammar (not the parser of
record). `LANGUAGE.md` + the Rust frontend remain authoritative for syntax.

```
editors/tree-sitter-anubis/grammar.js
editors/tree-sitter-anubis/queries/highlights.scm
```

---

## 9. Proofs (optional depth)

When you need a receipt, not just a typecheck:

```bash
anubis prove examples/proof/proof_factorial_input.anb \
  --backend risc0 --lane cpu \
  --input-json '{"n":5}' --evidence --out out/proof_factorial_5
```

Metal hybrid and doctor requirements are covered in `docs/INSTALL.md` and
`docs/APPLE_NATIVE.md`. Proof claims stay precise: what is post-quantum, what is not,
and what the journal actually binds.

---

## 10. Gate: prove the DX stack yourself

```bash
bash scripts/run_dx_gate.sh out/dx_gate
# expect: DX_GATE: PASS  (10 checks: unit, doc, repl, hello, lsp, editors, tutorial, p5/p6)
```

That gate is the seal for Phase 7 — not aspirational docs.

---

## Where next

1. Skim **§Comments through §Functions** in `LANGUAGE.md`.
2. Write a small library with `requires`/`ensures`, then `anubis doc` it.
3. Wire the VS Code extension and hover a contracted function.
4. When ready for multi-crate work: Phase 6 package docs + trust signers.
