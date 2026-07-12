# Anubis-SH (self-host subset) — Phase 8

Normative subset for `selfhost/src/anubis_sh.anb` and its corpus.

## In scope

- ASCII source only (char index == byte offset for spans)
- `//` line comments and nested `/* */` (skipped in token stream)
- `fn` / `pub fn`, params with optional `: Type`, optional `-> Type`
- `requires(E)` / `ensures(E)` / `uses(id, ...)` clauses (parsed + stored; no Z3)
- `let` / `let mut`, `x = e`, `return e`, `if` / `else`, `while`, `for i in a..b`
- Literals: integers, `true`/`false`, strings with `\"` and `\\`
- Lists `[e, ...]`, index `e[i]`
- Calls, binary ops `+ - * / % == != < <= > >= && ||`, unary `!` `-`
- Builtins: `print`, `println`, `len`, `push`, `char_at`, `ord`, `chr`, `substr`,
  `index_of`, `split`, `read_file`, `write_file`, `args`, `env`
- No `import` in SH v1 sources (single-file compiler)

## Out of scope (for what the compiler is *written in*)

`anubis_sh.anb` is authored in the subset above and re-emits itself to a fixpoint.
It does **not use** enums/match/traits/modules/taint/Z3/proof/research in its own body.

Note the distinction from what it can **compile**: the self-host compiler now parses,
checks, and *executes* the full executable language — enums, `Name::Variant`, match
(tuple/struct/wildcard patterns), if-expressions, `for x in <collection>`, maps —
verified against the host oracle (`scripts/run_selfhost_fulllang_gate.sh`). Still
genuinely out of scope everywhere: traits, modules/`import`, the taint/Z3 engine, and
the proof/research lanes.

## Fixpoint artifact

- **Source:** deterministic Rust emitted by SH codegen (`sha256` equal across
  generations: stage2.rs ≡ stage3.rs).
- **Binary (same-toolchain):** the fixpoint source compiled through a pinned
  reproducible rustc invocation yields byte-identical native binaries after the
  content-derived `LC_UUID` + ad-hoc code signature are normalized out.
- **Not** cross-rustc-version binary identity (different compiler → different code).
