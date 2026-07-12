# Phase 8 — Self-hosting (Anubis-SH)

## Status

**REAL bootstrap (SH subset)** — sealed by an actual stage chain, not host×2:

```bash
bash scripts/run_selfhost_gate.sh out/selfhost_gate
# SELFHOST_GATE: PASS (N/N)
```

### What the gate proves

```
stage0  Rust host runs selfhost/src/anubis_sh.anb
        → emits stage1.rs  (interpreter runtime + PAYLOAD AST)
        → rustc → stage1 binary

stage1  stage1 compile anubis_sh.anb
        → emits stage2.rs
        → rustc → stage2 binary

stage2  stage2 compile anubis_sh.anb
        → emits stage3.rs

seal    cmp stage2.rs stage3.rs   # byte-identical fixpoint
```

Hostile re-audit (2026-07-12) found the prior gate only compared
`host(SH)×2` and that early emit was a payload-viewer. Those are fixed:

| Defect | Fix |
|--------|-----|
| Fake fixpoint (stage0×2) | Real stage0→1→2→3; seal is stage2≡stage3 |
| Payload-viewer emit | `sh_codegen` embeds `anubis_sh_interp_rt.rs` + `PAYLOAD` + `sh_run` |
| Parse/check exit 0 on error | `exit(1)` after diagnostics |
| Stale evidence dir | Gate `rm -rf`s OUT before running |
| `If` else never ran in stage-1 | Interpreter uses `else` list, not missing `has_else` |

## What is self-hosted

`selfhost/src/anubis_sh.anb` is a single-file **Anubis-SH** compiler written in Anubis:

| Stage | Implementation |
|-------|----------------|
| Lex | Full SH token stream (comments skipped); byte spans; matches host goldens |
| Parse | Recursive descent; list/map literals; enums, `Name::Variant`, match, if-expressions, for-in collections; `requires`/`ensures`/`uses`; fail-closed |
| Check | Names, arity, annotated type mismatches, builtin allowlist; pattern bindings |
| Codegen | Deterministic Rust **package**: no-deps AST interpreter + embedded program JSON |

Normative subset the compiler is *written in*: [`selfhost/SUBSET.md`](../../selfhost/SUBSET.md).

Runtime: `selfhost/runtime/anubis_sh_interp_rt.rs` (embedded verbatim into stage packages).

## Full-language coverage (oracle-verified)

The self-host compiler *compiles and runs the full executable Anubis language*, not
just the subset it is written in. This is proven differentially against the Rust host
`anubis run` as an oracle over the example corpus:

```bash
bash scripts/run_selfhost_fulllang_gate.sh
# SELFHOST_FULLLANG_GATE: PASS (18 pass / 9 skip)
```

For every `examples/*.anb` the host executes (rc==0), the self-host compiles it
(`compile <ex>` → rustc → run) to **byte-identical stdout and exit code**. Covered
surface, verified end-to-end: `enum` declarations (unit / tuple / struct variants),
`Name::Variant` construction, `match` with tuple/struct/wildcard patterns and
bindings, `if`-expressions with `else if` chains, `for x in <list|map>` collection
iteration, maps, lists, recursion, and the string/IO builtins.

The 9 skips are research/taint/symbolic/proof-only programs that the host itself
rejects under `anubis run` (they require the check/prove path, not execution) — they
are outside the executable-language surface, not gaps in it.

**Fidelity note (honest):** the interpreter iterates map keys in sorted order
(`BTreeMap`); the host uses insertion order. This is observable only for a program
whose *output* depends on map-key iteration order — no program in the verified
corpus does. All other observable behavior matches the host exactly.

## Dogfood: the compiler is written in Anubis's own enums + match + if-expressions

`selfhost/src/anubis_sh.anb`'s parse / check / codegen **logic is authored in idiomatic
Anubis** — its entire AST is an algebraic data type dispatched by `match`, not
string-keyed maps:

- **`enum Stmt`** (7 struct variants) — built in `parse_stmt`, consumed by `match` in
  `jstmt` (codegen) and `check_stmt` (checker).
- **`enum Expr`** (15 variants; tuple shapes for `Int`/`Str`/`Bool`/`Var`, struct for the
  rest) — built in the parser, consumed by `match` in `jexpr` and `check_expr`.
- **if-expressions** in `jbool`, `prec_of`, `json_escape`, and the lexer's char→kind table.

This is enforced, not asserted, by a fail-closed gate:

```bash
bash scripts/run_selfhost_dogfood_gate.sh
```

- **G1 structural** — the compiler's *own parsed AST* must contain the load-bearing enums
  and `match` nodes inside the named codegen/checker functions (uncheatable by comments).
- **G3 ablation** — mechanically neuter one `match` arm (e.g. `jexpr`'s `Expr::Var`) and
  require the self-build to break. Load-bearing ⟺ removal breaks the output. This is a
  mechanized genuineness proof that neither Zig (`zig3≡zig4` is prose, never diffed) nor
  Dafny (self-host loop never closes) ships.

The dogfooded compiler still reproduces itself byte-identically (source + binary
fixpoint, `SELFHOST_GATE: PASS (9/9)`).

**Honest boundary:** the tree-walking execution **runtime** (`anubis_sh_interp_rt.rs`) is
hand-written Rust — a fixed trusted seed, not itself dogfooded. The claim is precise: the
compiler's *parse/check/codegen logic* is authored in Anubis; the runtime is a Rust seed.

## Host support

```bash
anubis selfhost dump-tokens <file.anb>
anubis selfhost dump-ast <file.anb>
```

Schema: `compiler/src/selfhost_schema/`.

## Driver

```bash
# Stage-0 (host interprets SH source)
anubis run selfhost/src/anubis_sh.anb --allow-research -- \
  lex|parse|check|compile <file> [-o out.rs]

# Stage-1+ (standalone package produced by compile)
rustc -O out.rs -o shc && ./shc compile selfhost/src/anubis_sh.anb -o stage2.rs
```

`--allow-research` is required for the host driver because native safe lowering rejects some I/O/taint combinations used by the self-host pipeline; the SH checker itself is Safe-oriented for SH programs.

## Fixpoint definition (honest)

**Source seal:** byte-identical stage2 and stage3 sources:

```
stage1 compile anubis_sh.anb -o stage2.rs
stage2 compile anubis_sh.anb -o stage3.rs
cmp stage2.rs stage3.rs   # required
```

**Binary seal (same-toolchain):** the byte-identical fixpoint source, compiled through
a pinned reproducible rustc invocation, yields **byte-identical native binaries**:

```
rustc -O -C codegen-units=1 -C debuginfo=0 --remap-path-prefix=$OUT=. canon.rs -o stage2.bin
# (same for stage3.rs under the same canonical filename)
codesign --remove-signature stage2.bin stage3.bin   # strip ad-hoc signature
python3 scripts/macho_normalize.py stage2.bin stage3.bin   # zero content-derived LC_UUID
cmp stage2.bin stage3.bin   # required
```

`LC_UUID` and the ad-hoc code signature are the only per-link nondeterministic Mach-O
fields and carry no program semantics; they are normalized out. The binaries are kept
runnable (with `LC_UUID`) and liveness-checked (`stage2.bin version` → `anubis-sh 0.1.0`)
before normalization. Determinism of a single stage (same binary, same input → same
emit) is also checked for hello.

**Not claimed (honest residual):**

- **Cross-toolchain-version** binary identity. The binary seal holds for *one* rustc +
  flag set. A different rustc version emits different code; that is expected and not
  claimed. What *is* now gated is **external reproducibility under a pinned
  toolchain/image** — see the reproducibility note below and
  `scripts/run_selfhost_repro_gate.sh`; that is a distinct, weaker claim than
  cross-version identity.
- **Trusting-trust closure.** A byte-identical self-host fixpoint does **not** prove the
  seed (Rust host + rustc + LLVM) is backdoor-free — a compromised seed reproduces its own
  backdoor through the fixpoint silently. We do **no** Diverse Double-Compilation (Wheeler).
  This is an explicit open `NEEDS-HUMAN` obligation, never a checkbox the fixpoint ticks.
  Forbidden phrasing: "backdoor-free", "trust root proven".
- **Compiler correctness.** `stage2 ≡ stage3` proves *determinism / self-reproduction*, not
  semantic correctness. The differential oracle raises confidence over the corpus; it is
  not a proof for all programs.
- **Native lowering in Anubis / "the whole compiler in Anubis".** Codegen emits a
  deterministic interpreter *package* (runtime + AST payload), not a reimplementation of the
  host's `lower_program_to_rust`; and the tree-walking runtime is hand-written Rust — a
  fixed trusted seed, not itself dogfooded. Honest phrasing: the compiler's *parse/check/
  codegen logic* is authored in Anubis; the execution runtime is a Rust seed.
- **Z3 / taint engine in Anubis; replacing the Rust host as the default toolchain.** The
  Rust host remains the trusted seed of the bootstrap chain by design (a trusted base is
  standard for compiler bootstraps); the SH checker is Safe-oriented and does not run Z3.
- Phase-number identity with historical “Phase 8 = hostile re-audit” roadmap rows
  (re-audits are process, not the self-host finish line).

**Closed 2026-07-12 (was a residual):** *the compiler's own source is authored in the SH
subset.* It now uses `enum Stmt` / `enum Expr` + `match` + if-expressions in load-bearing
positions and still reaches the byte-identical source + binary fixpoint. Enforced by
`scripts/run_selfhost_dogfood_gate.sh` (structural + ablation). See *Dogfood* above.

**Earned 2026-07-12 — external reproducibility (`scripts/run_selfhost_repro_gate.sh`, fail-closed):**
the byte-identical fixpoint source, compiled under a pinned toolchain with `$HOME` + build
dir remapped and `SOURCE_DATE_EPOCH=0`, produces a binary that is (a) deterministic across
independent build dirs and (b) free of host/user identity paths — verified by a negative
control (a build without the remap leaks 9 machine paths). A **hermetic Linux lane** builds
the same source inside a pinned `rust` image (recorded by digest) and reproduces a
bit-identical ELF across independent container runs. A third party can re-derive the exact
bytes from `repro_manifest.json`. **This proves reproducibility, not trust:** it does *not*
close trusting-trust (a subverted rustc reproduces its own subversion here too) — that
remains the open `NEEDS-HUMAN` above, needing a second independent backend
(see `SELFHOST_REPRO_PLAN.md`).

## Codegen honesty

Codegen emits a deterministic **interpreter package** (runtime + AST payload), not a full reimplementation of host `lower_program_to_rust`. That is enough to prove:

1. SH front-end is real (lex/parse/check on self + corpus)
2. Stage packages execute (hello prints `hello, anubis`)
3. Self-compilation reaches a fixed point (stage2 ≡ stage3)
4. Failures exit non-zero

## Layout

```
selfhost/
  SUBSET.md
  src/anubis_sh.anb
  runtime/anubis_sh_interp_rt.rs
  corpus/
  golden/{tokens,ast}/
scripts/run_selfhost_gate.sh
compiler/src/selfhost_schema/
docs/language/SELFHOST.md
```
