# 02 — Frontend & tooling review (read-only map)

Tree: `b11cacaf` (HEAD, 2026-09-23 02:40 -0400). Date of review: 2026-09-23.
Method: grep plus targeted line ranges. No cargo, no git state changes, and no edits outside this file.
Behavioural probes ran against a **copy** of `target/release/anubis` in a scratch directory
(sha256 `0a412d5f38022f11f644f23515c7116eda4e693a73852b6addd9ccb163012d58`, mtime 2026-09-23 00:00).
That binary predates HEAD by about 2.7 h, so its behaviour may not match HEAD source exactly. Where a
probe result and a code reading agree, both are cited.

## 0. What was read

| Depth | Files |
|---|---|
| **Read in full** | `compiler/src/lsp_analysis.rs`; `compiler/src/diagnostics/mod.rs` 1–708 (non-test); `compiler/src/resolve/mod.rs` 1–712 (non-test); `compiler/src/package/lock.rs`; `compiler/src/package/mod.rs`; `docs/language/{DIAGNOSTICS_JSON,SPEC,GRAMMAR,EDITIONS,SPEC_1_0_FREEZE,PACKAGES}.md`; `docs/language/STDLIB_CORE.md` (outside scope, but it is the stdlib doc); `editors/vscode-anubis/{extension.js,package.json}`; `tools/anubis/src/main.rs` 2171–2340 (LSP server) and 2990–3030 (JSON emission) |
| **Targeted ranges** | `frontend/mod.rs` (types 1–640, lexer unwrap sites, `Parser` 1794–1940, 2143–2176, 2855–2992, 3313–3434, 3548–3800, 3925–4046, 4124–4150, 4400–4580); `fmt/mod.rs` 1–60; `doc/mod.rs` 1–60; `interp/mod.rs` (error sites only); `project/mod.rs` 1–160; `package/resolve_deps.rs` 55–222, 596–646; `package/registry.rs` 1–30, 185–212; `package/proof.rs` 60–100 |
| **Signatures / greps only** | `package/{merkle,cache,trust,semver,summary,entitlements,confinement}.rs`; `stdlib/mod.rs` 1–30; all `compiler/stdlib/std/*.anb` (only the `pub fn` names); `editors/tree-sitter-anubis/grammar.js` (header and keyword set); `anubis.tmLanguage.json` (patterns) |
| **Not read** | generated `tree-sitter-anubis/src/{parser.c,grammar.json,node-types.json,tree_sitter/*}`; bodies of `semver.rs`, `summary.rs`, `entitlements.rs`, `confinement.rs`; the fmt printer body; the stdlib function bodies |

## 1. File inventory

| Path | Lines | Role |
|---|---:|---|
| compiler/src/frontend/mod.rs | 4785 | Lexer, recursive-descent/Pratt parser, AST, trait desugaring, doc-comment association, caret renderer |
| compiler/src/resolve/mod.rs | 1013 | Multi-file import graph (DFS, cycle/escape checks); "combine" pass that name-mangles modules into one flat item list |
| compiler/src/diagnostics/mod.rs | 1112 | `anubis-diagnostics/1` JSONL types and renderer |
| compiler/src/lsp_analysis.rs | 279 | LSP diagnostics and hover logic (the JSON-RPC loop is in `tools/anubis/src/main.rs:2171`) |
| compiler/src/fmt/mod.rs | 834 | AST pretty-printer with round-trip self-verification |
| compiler/src/doc/mod.rs | 255 | `anubis doc` (Markdown/JSON of signatures, contracts, effects) |
| compiler/src/interp/mod.rs | 286 | Partial AST interpreter for `anubis repl` |
| compiler/src/stdlib/mod.rs | 124 | `include_str!` registry of 13 embedded `std.*` modules; content digests |
| compiler/src/project/mod.rs | 430 | `Anubis.toml` (serde/toml), `Edition`, `ProjectLayout::discover` |
| compiler/src/package/*.rs | 3326 total | cache 93, confinement 423, entitlements 531, lock 98, merkle 151, mod 44, proof 238, registry 305, resolve_deps 646, semver 172, summary 551, trust 74 |
| compiler/stdlib/std/*.anb | 1254 (+13-line MANIFEST.sha256) | collections 149, crypto 214, io 32, iter 60, math 80, net 18, option 36, pwn 434, rand 11, result 43, str 94, testing 67, time 16 |
| docs/language/DIAGNOSTICS_JSON.md | 310 | Normative JSONL spec (provisional per the freeze doc) |
| docs/language/SPEC.md | 163 | "Normative sketch"; defers to `LANGUAGE.md` |
| docs/language/GRAMMAR.md | 77 | EBNF that describes itself as "intentionally partial" |
| docs/language/EDITIONS.md | 92 | Edition policy (proposal) |
| docs/language/SPEC_1_0_FREEZE.md | 110 | 1.0 frozen surface plus post-freeze additions |
| docs/language/PACKAGES.md | 171 | Package manager, lockfile, trust |
| editors/tree-sitter-anubis/ | grammar.js 177, highlights.scm 25, corpus 52, plus generated files | Highlight-only grammar ("not the parser of record", grammar.js:1) |
| editors/vscode-anubis/ | extension.js 68, package.json 41, tmLanguage 50, language-configuration 17, README 57 | TextMate highlighting plus an LSP client that launches `anubis lsp` |

## 2. Source → AST

**Lexer.** `lex_spanned` (frontend/mod.rs:773–1617) is a hand-written `Peekable<CharIndices>` scanner.
It produces `SpannedToken { token, span: {start,end} }` with byte offsets. Comments are real tokens
(`LineComment`/`BlockComment`), and the parser filters them out (1808–1819). Unknown characters become
`Token::Other(String)` and are not diagnosed at lex time. Radix literals are normalised to decimal
strings (1389–1398). Interpolated strings keep their `${…}` text, and the parser re-lexes it later
(3584–3679).

**Parser.** Hand-written recursive descent. Binary expressions use precedence climbing
(`parse_expr`, 3548–3574, with the ladder at 4124–4148):
`|| (4) < && (5) < | (6) < ^ (7) < & (8) < comparisons, all at 10 < << >> (18) < + - (20) < * / % (30)`.
All operators are left-associative. Every comparison shares level 10, so `a < b < c` parses as
`(a<b)<c` rather than being rejected. The `as` cast is handled as a postfix inside the loop (3551–3559).
Unary `-` and `!` are handled in `parse_primary` (3738–3760). A `no_struct` flag resolves the
`for i in 0..n {` ambiguity (1797–1800, 1830–1851).

**AST shape.**
- Types are **strings**: `params: Vec<(String,String)>` (246) and `ret: Option<String>` (251).
- `return`, `break` and `continue` are encoded as pseudo-calls, e.g. `Expr::Call{callee:"return"}` (3313–3325, 3767–3791).
- Traits are erased at parse time by `resolve_traits` (641). A read-only `trait_env` sidecar is kept (96–110).

**Spans.**
- Items carry spans (230–315).
- Among statements, only `Let` and `LetPattern` do. `Assign`, `If`, `While`, `For` and `ExprStmt` have none (444–511).
- Among expressions, only `StructLiteral`, `FieldAccess`, `EnumConstruct`, `Match`, `If`, `MapLiteral` and `IfLet` carry spans. `Var`, `Literal`, `Call`, `Binary`, `Unary`, `Index` and `Cast` do not (513–640).
- This is the root cause of the "semantic diagnostics have no location" residual.
- Spans inside `${…}` are relative to the fragment, not the file, because a sub-parser is used (3674–3679).

**Error recovery.**
- At item level: emit "expected item", bump one token, continue (1922–1926).
- Statements and expressions recover mostly by fabricating error nodes: `Expr::Other("missing-init")` (3396, 3431) or `Expr::Other("eof")` (3794).
- The legacy `parse(tokens)` entry (4475) sets spans to token indices, not bytes.
- `parse_source` flattens all diagnostics into one `"; "`-joined string (4504–4515), which is how imported-module parse errors reach the user.

**Silent error node (defect).** `parse_primary`'s fallback `other => Expr::Other(format!("{:?}", other))`
(frontend/mod.rs:4042) consumes the token and records **no diagnostic**. Probe results:
- `fn main(){ let x = 1 + ; }`, `let x = );`, `let x = §;` and `"a ${ 1 + } b"` all give `anubis check` exit **0** (JSON `verdict:"pass"` observed directly for `let x = );` and `"a ${ 1 + } b"`). `anubis run` then refuses with `ANUBIS_UNSUPPORTED_NATIVE_LOWERING` (`Semi`/`RParen`/`Other("§")`).
- `requires(§)` and `assert(§)` also pass `check` (rc 0).
- `ensures(§)` and `invariant(§)` are refused further down the pipeline (`ANUBIS_CONTRACT_UNPROVABLE` / `ANUBIS_LOOP_INVARIANT_UNVERIFIABLE`).

So `check` reports a syntactically invalid program as passing. Execution still fails closed.

**Can malformed input panic?**
- **Yes, via stack overflow.** There is no recursion-depth limit anywhere in the parser. Probe: nesting depth 1000 and 3000 gave rc 0; depth 10000, 30000 and 200000 each **aborted with `has overflowed its stack` (rc 134, core dumped)**.
- `unwrap`/`expect` on the user-input path are all guarded by a preceding `peek`/`matches!`/`is_none` check:
  - 1082, 1122, 1399, 1447, 1476, 1535: lexer, after `peek` matched
  - 2087: after `is_none` break
  - 2375: after `matches!(Ident)`
  - 2705, 2736: after `check_keyword("if")`
  - 4406: `expect("parser has eof token")`, an invariant of the lexer
- Indexing in `interp_string` (3594–3651) is bounds-checked by the loop guards.
- No panicking unwrap on user input was found. The overflow is the live crash.

## 3. Name resolution

- **Module-level resolution** (`resolve/mod.rs`):
  - `import a.b;` → `src_root/a/b.{anb,anub,anubis}`, then `a/b/mod.*` (138–166).
  - `std.*` resolves only to the embedded registry, and an unknown `std.x` is refused rather than falling back to disk (82–92).
  - Package deps mount by first path segment (95–135).
  - Symlink escapes → `ANUBIS_IMPORT_ESCAPE`; cycles → `ANUBIS_IMPORT_CYCLE` (three-colour DFS, 227–300).
  - Embedded std modules may import only `std.*` (274–283).
- **Bindings are strings, not stable identities.** `combine_graph` (383–435) flattens every module into
  one item list and renames functions `"{prefix}__{name}"`, where the prefix is the dotted path with
  non-alphanumerics mapped to `_` (314–320, 461–462). Consequences, from reading the code (not probed):
  - `a.b` and `a_b` produce the same prefix `a_b`, so their mangled names can collide.
  - Only `Item::Fn` is renamed. Structs, enums and impls from different modules land unmangled in one namespace (458–486).
  - The import alias is the **last** path segment (322–324), and `alias_to_prefix.insert` (403–409) silently overwrites, so `import x.util; import y.util;` → the last one wins.
  - A cross-module call reaches the resolver as `EnumConstruct{enum_name: alias, …}` and is disambiguated by string lookup. A name that is both an enum and a module → `ANUBIS_AMBIGUOUS_PATH` (674–679).
- **Visibility.** `pub fn` versus private. It is enforced only for `alias::f` cross-module calls
  (`ANUBIS_PRIVATE_ITEM`, 680–690). There is no visibility on structs, enums or fields in the resolver.
- **Local scoping** (let/shadowing/blocks) is not in `resolve/`. It lives in the middle end, which is outside this review's scope.
- **Location loss.** Import spans are collected (175–183) but ignored (`_span`, 285, 402). Every
  resolver error is a bare `String` with no line or column.

## 4. `anubis-diagnostics/1`: implemented vs documented

- **Emitted by** `anubis check --message-format=json` only (main.rs:2669–2685, 2997–3019). Unknown formats → `ANUBIS_MESSAGE_FORMAT_UNKNOWN` (main.rs:2679), as documented.
- **Diagnostic fields** (diagnostics/mod.rs:226–263): `$type, schema, code, family, status, defect_locus, agent_action, severity("error"), build_blocking(true), message, obligation?, counterexample?, location?, budget?, suggestions([])`. These match doc §4.
- **Summary** (266–279): `verdict, counts{total,disproved,undecided,replay_mismatch,refused}, coverage?`, where coverage is `{certified, trusted_to_solver, discharged, uncertified, not_discharged, witnesses_retained}` (287–310). The opening line of doc §5 lists only four coverage keys. The two extra keys are described further down the same section, so the doc is internally inconsistent but not wrong.
- **Epistemic classes.** `Status` = `disproved | undecided | replay_mismatch | refused` (77–88). This is a separate field from `severity`, which is always `"error"`.
  - Mapping (`classify`, 496–566): `Disproved` → `ANUBIS_ASSERTION_DISPROVED` or `ANUBIS_WRAP_RISK`; `Undecided` → `ANUBIS_ASSERTION_UNDECIDED`; `ReplayMismatch` → `ANUBIS_REPLAY_MISMATCH`; residual → `refused` with `ANUBIS_SOLVER_UNAVAILABLE` / `ANUBIS_SOLVER_TRUST` / `ANUBIS_ASSERTION_UNPROVEN`.
  - Locus and action come from `middle::refusal_locus`, not from the status.
  - Every frontend, type, effect or taint refusal is `status: refused` + `family: frontend` + `repair_program`, with the `code` lifted from the message's leading `ANUBIS_*` (333–351, 354–372).
- **Versioning.**
  - The `schema` string is embedded in every line (`SCHEMA`, 50).
  - The additive rules in doc §10 are policy only: there is no schema test for unknown-field tolerance on the producer side.
  - SPEC_1_0_FREEZE.md §1a declares the schema **provisional, not frozen**.
- **Location precision.**
  - Byte span plus 1-based line and column, counting columns in Unicode scalars (frontend `line_col`, 4521–4539).
  - It is filled **only** for parse errors of the entry file (`diagnostics_of_parse_errors`, 380–411).
  - Solver diagnostics always have `location: None` (618). Frontend and semantic refusals do too (368).
  - This matches doc §7/§11 ("semantic refusals have no location").
- **Gaps against the doc and the invariant:**
  1. Doc §2 says `verdict` is `fail` whenever the check failed. It holds only if parsing reports the failure. §2 above shows `check` passes malformed input (`let x = );`) with `verdict:"pass"`, because the parser dropped the error (frontend/mod.rs:4042).
  2. Doc §1 says nothing else shares the channel and that a failure yields a stream. main.rs:2986–2996 admits in a code comment that I/O early returns yield **empty stdout**.
  3. A frontend refusal is a single diagnostic built from a possibly `"; "`-joined string, so several errors collapse into one `code`. This contradicts doc §4's "one code per finding" for non-parse refusals.
  4. If the solver has failures, a concurrent frontend or type refusal is not emitted (main.rs:3004–3011). This is documented at diagnostics/mod.rs:637–644.
  5. A parse error in an **imported** module becomes one location-less `ANUBIS_IMPORT_PARSE` refusal (resolve/mod.rs:269–270).
  6. `budget.consumed` is never populated, as documented.
  7. The LSP uses different codes: `ANUBIS_PARSE`, `ANUBIS_TYPECHECK` and `ANUBIS_OBLIGATION` (lsp_analysis.rs:37, 65, 99) versus `ANUBIS_PARSE_ERROR` in JSON (diagnostics/mod.rs:389).

## 5. LSP

- **Server.** Hand-rolled stdio JSON-RPC in `tools/anubis/src/main.rs:2171–2340`.
  - Capabilities advertised: `textDocumentSync: 1` (full) and `hoverProvider` only (2257–2262).
  - Handlers: `initialize`, `initialized`, `shutdown`, `exit`, `didOpen`, `didChange`, `hover`. Everything else → `-32601` (2323–2334).
- **Missing features.** No completion, go-to-definition, references, rename, code actions, document symbols, formatting or semantic tokens. UNSUPPORTED.md:593 declares rename and completion out of scope.
- **Diagnostics** (`lsp_analysis::analyze_source`, 25–71). Pipeline: parse, then typecheck, then `check_obligations`. Location handling:
  - Parse errors: accurate range.
  - Typecheck `Err`: pinned to **0:0–0:1** (58–67).
  - Semantic diagnostics without a span: **1:1** (78–79).
  - Every failing proof obligation: **0:0** (92–101), with message `name: FAIL detail`. There is no counterexample and no status class, and the `disproved`/`undecided` distinction is lost.
  - Everything is severity 1.
- **Single-file only.** It calls `parse_source_detailed` and `typecheck`, never `combine_from_entry`. Imports and package deps are not resolved, so a multi-file project is analysed without its modules.
- **Hover.** Signature, contracts, `uses(...)`, and taint/secret qualifiers for a function whose *name* matches the word under the cursor (105–198). Lookup is by name only, first match, across modules and impls. No hover on variables or types.
- **Robustness and position encoding:**
  - A malformed JSON body returns `Err` via `?` and terminates the server (main.rs:2210).
  - A message with no `Content-Length` ends the loop (2205–2207).
  - Hover treats the UTF-16 `character` as a byte offset (2304–2313).
  - Diagnostics report columns in Unicode scalars (`line_col`), not UTF-16. Both are wrong for non-ASCII text.

## 6. Package manager / project system

- **Manifest.** `Anubis.toml` is parsed with serde/toml. A malformed manifest is a hard error.
  - `[package] edition`: only `"2026"` is accepted. An unknown edition → `ANUBIS_EDITION_UNKNOWN` (project/mod.rs:67–87). A missing edition defaults to `E2026` (54).
  - The per-package edition rules that EDITIONS.md describes are not implemented: only one edition exists.
- **Dependency specs.** `DepSpec` supports version / path / git+rev / registry (project/mod.rs:132–149).
- **Lockfile** (`Anubis.lock`, TOML, lock.rs):
  - Each entry pins `name, version, source, path|git+rev|registry, content_sha256 (Merkle root), evidence_sha256, signer_public_key`.
  - Gaps:
    - `version: u32` is **never validated** (lock.rs:47–49).
    - The staleness check only confirms that every manifest dependency *name* exists in the lock (resolve_deps.rs:80–90). A changed version or source in `Anubis.toml` is not detected, and extra lock entries are accepted.
- **Integrity.**
  - Every resolve re-hashes the materialised tree against `content_sha256` → `ANUBIS_CACHE_HASH_MISMATCH` (resolve_deps.rs:146–160).
  - Signed evidence is required: PCA, Ed25519, trust store, source binding, and a summaries re-derive (proof.rs:60–97, resolve_deps.rs:162–172).
  - Unsigned dependencies are allowed only with both the CLI flag and `ANUBIS_ALLOW_UNSIGNED_DEPS=1` (main.rs:1757–1760). This matches PACKAGES.md.
  - Merkle single-leaf roots ignore the file path (merkle.rs doc comment), so renaming a single-file package keeps its hash.
- **Resolution.**
  - Transitive DFS: cycle → `ANUBIS_DEP_CYCLE`; same name with different version or content → `ANUBIS_DEP_VERSION_CONFLICT` (resolve_deps.rs:237–307).
  - There is no version unification or solving: one version per name, and a conflict is a hard error.
  - SemVer requirements go to `semver.rs` (not read).
- **Fetching** (security-relevant):
  - git: `git clone --depth 1 <url> <dir>` and `git checkout <rev>` with **no `--` separator** (resolve_deps.rs:611–637). A URL or rev beginning with `-` would be read as an option.
  - The clone directory is the predictable, shared `$TMPDIR/anubis-git-fetch/<name>/<rev>`, and it is reused if it exists, without checking HEAD (600–607). The content-hash check mitigates this.
  - Registry: `http://` is accepted as well as `https://` (registry.rs:113, 162). Fetching goes through `curl -fsSL`.
  - Tarballs are unpacked with system `tar -xzf` into the cache parent (registry.rs:191–197). Protection against path traversal and symlinks in an untrusted archive is left to tar's defaults.
- **Build scripts.** None exist. No `build.anb` or pre/post hooks were found in `package/`, `project/` or PACKAGES.md, so dependency resolution never runs package code.

## 7. Standard library inventory

There are 13 embedded modules (stdlib/mod.rs:10–27), content-locked by `compiler/stdlib/MANIFEST.sha256`.
The `pub fn` counts below come from grep:

| Module | pub fns | Surface |
|---|---:|---|
| std.collections | 18 | `set_*` (new/contains/len/to_list/insert/remove/union/intersect/diff), `omap_*` (new/len/has/get/put/remove/keys/values/to_map) |
| std.crypto | 35 | sha256, HMAC, HKDF, PBKDF2, Argon2id, password_hash/verify, Ed25519 sign/verify, AEAD keygen/nonce/encrypt/decrypt, ECDH, hybrid_encrypt/decrypt, rand_bytes, secret_eq, commit |
| std.io | 6 | read_text, write_text, append_text, read_env, print_line, print_err |
| std.iter | 12 | iter_map/filter/reduce/sum/count/any/all/find/enumerate/zip/flatten/for_each |
| std.math | 11 | add/sub/mul/abs/min/max/clamp/sign/pow_u/gcd/lcm |
| std.net | 4 | http_get_text, http_post_text, connect_host, send_host |
| std.option | 5 | is_some/is_none/unwrap_or/map/and_then |
| std.pwn | 49 (+2 private) | pack/unpack LE/BE, p8–p64, cyclic/de_bruijn, payload/pad/nop_sled/rop chain, xor, hexdump, crash classification, run_local* (offensive toolkit, research-gated per STDLIB_CORE.md) |
| std.rand | 2 | gen, bytes |
| std.result | 6 | is_ok/is_err/unwrap_or/map/map_err/and_then |
| std.str | 17 | is_empty/len/upper/lower/trim/contains/starts_with/ends_with/join/lines/eq_ignore_case/repeat/index_of/substr/pad_left/pad_right/strip_prefix |
| std.testing | 6 | assert_true/eq/ne/lt/le/gt |
| std.time | 2 | now, elapsed_ms |
| **Total** | **173** | |

- **Documented vs implemented.** STDLIB_CORE.md names the same 13 modules, but no document in scope gives a per-function reference, signatures or contracts for `std.*`. BUILTINS.md covers the 213 Rust builtins beneath them, not these wrappers.
- SPEC_1_0_FREEZE.md §2 freezes "embedded `import std.*`" without enumerating the functions, so the frozen stdlib API is undefined.
- There is no `std.fs`/`std.path`, `std.process`, `std.fmt`, `std.json` or `std.sync`/concurrency module. `std.net` and `std.time` are thin.

## 8. Gaps against a 1.0 product

| # | Gap | Evidence |
|---|---|---|
| 1 | Parser drops unexpected tokens as `Expr::Other` with no diagnostic, so `check` returns pass or `verdict:"pass"` on malformed source (`let x = );`, `requires(§)`, `assert(§)`, `"${1+}"`) | frontend/mod.rs:4042, 3794; probes in §2 |
| 2 | Unbounded recursion: nesting depth ≥10000 aborts with a stack overflow (rc 134) | frontend/mod.rs:3548–3574 (no depth guard); probe |
| 3 | No normative grammar. GRAMMAR.md is "intentionally partial"; SPEC.md is a "normative sketch" that defers to LANGUAGE.md and says "code and gates win" | GRAMMAR.md:3–6; SPEC.md:7 |
| 4 | Most statement and expression nodes have no span, so semantic diagnostics cannot carry a location | frontend/mod.rs:444–640; diagnostics/mod.rs:368, 618 |
| 5 | Diagnostics schema not frozen; only `check` emits it; empty stdout on I/O early return | SPEC_1_0_FREEZE.md §1a; DIAGNOSTICS_JSON.md §11; main.rs:2986–2996 |
| 6 | Resolver is string mangling: only fns are namespaced; prefix collisions (`a.b` vs `a_b`); last-segment alias overwrite; errors carry no spans | resolve/mod.rs:314–324, 403–409, 458–486, 285 |
| 7 | LSP is minimal: diagnostics plus fn-name hover only; single-file (no imports); obligations and type errors pinned to 0:0 and flattened; not UTF-16 aware; malformed JSON kills the server | lsp_analysis.rs:25–101; main.rs:2210, 2257–2262, 2304–2313 |
| 8 | Editors: tree-sitter grammar lacks trait/match/for/loop/closures/attributes and has 2 corpus tests; TextMate keyword list lacks `invariant`/`secret`/`declassify`/`research`/`exploit`; VS Code extension is unpublished, v0.1.0 | grammar.js:22–30; test/corpus/basic.txt; anubis.tmLanguage.json |
| 9 | No parser fuzzing: `proptest` is a declared dependency but no `proptest!` exists in the tree; no `fuzz/` directory | Cargo.toml:43; grep |
| 10 | Lockfile: version unchecked; staleness checks names only | lock.rs:47–49; resolve_deps.rs:80–90 |
| 11 | git fetch without `--`; shared predictable tmp dir; `http://` registry; system `tar` on untrusted archives | resolve_deps.rs:600–637; registry.rs:113, 191–197 |
| 12 | Editions: one edition; a missing edition silently defaults (EDITIONS.md says it becomes an error at 1.0) | project/mod.rs:54; EDITIONS.md |
| 13 | Stdlib API not specified per function; frozen surface is not enumerated | SPEC_1_0_FREEZE.md §2; STDLIB_CORE.md |
| 14 | `fmt` returns any file containing `//` or `/*` unchanged, including inside a string literal such as `"https://…"`; trait files are refused; interpolation is rewritten to `+` | fmt/mod.rs:28–43 |
| 15 | `doc` silently falls back to a single-file parse when graph or dep resolution fails, which undercuts the "fail-closed on the entry graph" claim in SPEC.md Phase 7 §2 | doc/mod.rs:35–37 |
| 16 | REPL interpreter rejects unary ops other than listed, complex assignment, and most calls (`ANUBIS_REPL_UNSUPPORTED`) | interp/mod.rs:89, 153, 193, 257 |
| 17 | Conformance: language fixtures exist (`tests/fixtures/language_core`, 260 entries; `modules`, 6), but no fixture exercises the malformed-expression pass or the depth crash above | directory listing; probes |
