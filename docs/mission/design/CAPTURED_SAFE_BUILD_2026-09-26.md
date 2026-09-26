# Safe captured-source CLI build consumer — design for lead review

Status: read-only source analysis and a revised proposal addressing the
independent design HOLD. No source edit, build, test, executable witness,
evidence admission, or release gate was run by this worker. The proposed CLI
spelling is provisional; this is not approval of an implementation diff.

## Current path and exact boundary

The current `build` handler reads the entry by pathname and creates `out` before
loading the program (`tools/anubis/src/main.rs:2579-2584`). `load_program_items`
parses those supplied entry bytes, discovers `ProjectLayout`, resolves declared
dependencies, and, for imports, calls `combine_from_entry_opts`, which loads the
entry and imports again by pathname (`tools/anubis/src/main.rs:1792-1812`;
`compiler/src/resolve/mod.rs:519-534`, `:257-278`). Build then typechecks,
applies the taint pass, checks solver refusals, and lowers the combined AST
(`tools/anubis/src/main.rs:2625-2708`). That is the existing command behavior,
not a source-snapshot correspondence witness. Its optional evidence path formats
a resolved AST and retains the original entry bytes, but does not retain every
imported original source as a checked correspondence (`tools/anubis/src/main.rs:1815-1841,
:2660-2685`). Current multi-leaf PCA source-check scope is deliberately withheld
(`compiler/src/evidence/mod.rs:2413-2419`).

The private bridge already owns a combined AST and `SourceGraphSnapshot` with
private fields, both derived from one `CapturedTree` and parser-resolved imports
(`compiler/src/resolve/captured.rs:247-309, :334-364`). Its Safe method checks
that same AST, refuses outstanding solver checks, and lowers it to Rust source
(`:267-291`). The Linux/Android reader owns source-relative bytes; `get` never
reopens a path (`compiler/src/package/source_graph_reader.rs:122-149`). It scans
the entire source root, refuses symlinks and non-regular entries, and requires a
genuine procfs fd view (`:1-9, :180-245, :280-350, :447-450`). It is currently
unsupported on other targets (`:129-139`). These limitations must remain
visible in the CLI.

## Proposed first production consumer

Add an **opt-in** `anubis build --captured-source ENTRY` path on the Linux Safe
CLI. It produces a native artifact from the immutable captured compilation
input, but does not issue a verified seal. Keep the existing `build` path as a
comparison and compatibility path while this one earns wider parity. The new
path must never silently fall back to pathname resolution if capture refuses.

At the start of the `Build` arm, before `read_to_string(input)` or
`create_dir_all(out)`, branch on `captured_source`:

1. Reject `--evidence` and `--bounty` with a named unsupported-evidence error,
   because neither the current bundle schema nor its verifier can claim that
   source-graph edges match the checked AST. Reject `--no-verify` in this lane;
   the point of the consumer is the checked build. Reject `--full-hybrid`, since
   this adapter is a Safe native-core route rather than the hybrid emitter.
2. Discover `ProjectLayout` once. Its manifest parser already rejects malformed
   and unknown-edition manifests (`compiler/src/project/mod.rs:185-205, :247-290`).
   Require an empty declared dependency map. Any declared dependency, even an
   apparently unused one, returns `ANUBIS_CAPTURE_DEPS_UNSUPPORTED` before
   capture. Do not invoke `resolve_workspace`, read a lockfile as admission, or
   mount a dependency. This is a dependency-free *compilation* lane only.
3. Derive `entry_rel` by `layout.entry.strip_prefix(&layout.src_root)` and parse
   its source-relative spelling as `PortablePath`. Reject outside-root,
   nonportable, parent-component, and unrepresentable names with an explicit
   `ANUBIS_CAPTURE_ENTRY_UNSUPPORTED` error. Do not canonicalize through a
   symlink to make the input fit; the reader must see and refuse symlinks.
   `ProjectLayout` chooses `root/src` when present and otherwise `root`
   (`compiler/src/project/mod.rs:231-290`), so this check also refuses a
   manifest project entry outside its selected `src_root`. The captured tree
   must contain `entry_rel` before parsing begins. Require the output root to
   lie outside `src_root` in this first consumer, or return
   `ANUBIS_CAPTURE_OUTPUT_OVERLAPS_SOURCE` with an `-o` remedy. The reader
   includes prior artifacts under its root; an overlapping output would make
   repeat builds consume unrelated generated files and eventually hit a
   capture limit. Compare normalized absolute, symlink-free path components;
   treat a bind-mount alias as an acquisition/usability limit rather than
   claiming a global filesystem proof.
4. Acquire `CapturedTree::capture(layout.src_root, CaptureLimits::default())`
   once. Feed only that tree and `entry_rel` to `prepare_captured_project`.
   The parser, import resolver, combined AST, source graph, Safe classifier,
   typechecker, solver gate, and native lowering consume the saved bytes or
   compiler-embedded stdlib. The CLI does not separately parse or reread the
   entry. `prepare_captured_project` already refuses missing entries and
   source/import collisions (`compiler/src/resolve/captured.rs:213-245,
   :386-538`). A `--allow-research` flag on an actually Safe program remains
   inert, as in the current CLI (`tools/anubis/src/main.rs:7155-7167`). Before
   typechecking or lowering, require both
   `frontend::program_mode(items) <= Safe` **and**
   `evidence::items_have_unresolved_mode_elevator(items) == false`. The latter
   already recursively detects Research/Exploit blocks in expression and
   statement positions, but **its current attribute branch is incomplete**:
   it recognizes only `research` and `exploit`, while `parse_fn` treats
   `research`, `poc`, `fuzz`, `proof`, `defensive`, and `audit` as Research before
   a later `@safe` can override the stored mode
   (`compiler/src/frontend/mod.rs:3165-3175`). Extract that exact Research
   alias match into a small frontend helper consumed by `parse_fn` and the
   **same shared evidence predicate**; this is one attribute policy, not a
   second AST walker. The predicate maps the helper's matches to
   `Mode::Research` and keeps its existing `exploit` mapping to
   `Mode::Exploit` before the CLI calls it
   (`compiler/src/evidence/mod.rs:1449-1485`;
   `compiler/src/middle/mod.rs:7280-7482`). The current bridge only calls
   `program_mode` (`compiler/src/resolve/captured.rs:274-276`), so its existing
   non-Safe control does not cover that bypass. Do not write a partial second
   walker or duplicate the alias map in the CLI. If the shared predicate is
   later moved out of `evidence`, have evidence and the captured builder call
   the same implementation. No Research/Exploit source reaches the new native
   builder even with `--allow-research`.

   The helper can be `frontend::is_research_mode_attribute(name: &str) -> bool`,
   with `parse_fn` retaining its existing last-attribute-wins behavior and the
   evidence predicate asking whether **any** declared mode outranks the stored
   mode. Keep `emulation` and `agent` out of this helper: they are recognized
   authority attribute names but are not in `parse_fn`'s Research mapping
   (`compiler/src/frontend/mod.rs:213-225, :3165-3175`). This is an explicit
   compatibility choice rather than guessing from an allowlist.

5. Check with the **Safe-default** profile using
   `typecheck_ex_detailed(ast.clone(), Mode::Safe, false)`, retain its
   `TypecheckFailure`, apply the taint pass, and keep the ordinary solver
   refusal gate. `typecheck` currently means `verified=false`, while the
   stronger `--verified` effect/capability lane is separate
   (`compiler/src/middle/mod.rs:5266-5271, :5312-5339`). IFC v2 already runs
   in Safe typechecking (`:5480-5489`); this proposal neither removes it nor
   confers a verified effect seal. A per-function `@verified` remains governed
   by its current checker rules. Label the command's completion as a checked
   Safe-default native build, not a fully verified program or proof artifact.
6. Lower that same private AST with the checked IR's monomorphization
   inventory. A lowering refusal is an error: do not substitute
   `honest_analysis_marker`, because this route promises an executable
   artifact. The legacy native path uses that marker for unsupported programs
   (`compiler/src/backends/native/mod.rs:164-199`). Compile the resulting Rust
   string with the existing audited runtime-dependency compiler
   (`compiler/src/backends/run.rs:6228-6319`), but **do not use** the current
   direct writer at `compiler/src/backends/native/mod.rs:30-49` for final output:
   it writes `anubis_out.rs` directly, and the Rust compiler copies directly
   to `anubis_out`, which can leave an old executable or follow an output
   symlink on failure (`compiler/src/backends/run.rs:6310-6318`). Use the
   staged publisher specified below.
7. Print `native artifact` only after atomic publication. If a source graph
   digest is shown, label it `compilation input identity`, never a verified
   package closure, certificate, runtime confinement, or proof. No PCA,
   source-check verdict, package-publish status, or `bounty_ready` field is
   emitted on this path. Report the *new unique artifact path*; never report
   an older `out/anubis_out` as the product of this invocation.

The compiler-facing API should keep construction and artifact production
together. A narrow public backend facade accepts `(src_root, entry_rel,
output_root)` and returns a `CapturedSafeNativeBuild` with private fields and
read-only accessors for the *published* artifact path, companion Rust source
path, and informational compilation-graph digest. Internally it captures,
calls the private resolver constructor, checks/lowers, compiles, and publishes
the pair. Keep `PreparedCapturedProject`'s AST and graph private; do not expose
a mutable AST, graph builder, or a caller-provided alternate Rust source. The
resolver may offer a crate-private `prepare_captured_project_from_root` to the
native backend; the CLI sees only `build_captured_safe_native`. The CLI adds
its layout/dependency/option guards before invoking that facade.

Proposed public shape (names for review, with no public constructors or mutable
AST/graph access):

```rust
pub fn build_captured_safe_native(
    src_root: &Path,
    entry_rel: PortablePath,
    output_root: &Path,
) -> Result<CapturedSafeNativeBuild, CapturedBuildError>;

pub struct CapturedSafeNativeBuild { /* private */ }
impl CapturedSafeNativeBuild {
    pub fn artifact_path(&self) -> &Path;
    pub fn rust_source_path(&self) -> &Path;
    pub fn compilation_graph_digest(&self) -> &str;
}

pub enum CapturedBuildError {
    Capture(CaptureError),
    Resolve { kind: CapturedResolveKind, detail: String },
    NonSafeMode { mode: Mode },
    ModeElevator,
    Typecheck(TypecheckFailure),
    SolverRefused { checks: Vec<SolverCheck>, refusals: Vec<SolverCheck> },
    Lowering { detail: String },
    NativeCompile { detail: String },
    Output(OutputPublishError),
}
```

`CapturedResolveKind` is a closed, typed mapping from the private resolver
error enum (entry absent, UTF-8/parse, import, collision, limit, graph) rather
than a reparsed `Display` string. `OutputPublishError` has distinct overlap,
symlink/non-directory, collision, atomic-unavailable, and I/O variants.
The CLI's layout and incompatible-option preflight likewise uses explicit
branches; display text is presentation only.

Use **typed failure variants all the way through the bridge**. In particular,
replace `check_and_lower_safe_rust() -> Result<String, String>` with a private
`Result<CheckedSafeRust, CapturedSafeCheckFailure>` whose variants distinguish
`NonSafeMode`, `ModeElevator`, `Typecheck(TypecheckFailure)`,
`SolverRefused { checks, refusals }`, and `Lowering`. Store the exact generated
Rust string in `CheckedSafeRust` with private fields. `SolverCheck` retains its
existing wire fields, but call `solver_stream_refusals` and the centralized
`solver_outcome` classifier (`compiler/src/middle/mod.rs:133-155, :193`) rather
than infer outcomes from rendered English. The public facade maps capture,
resolve, check, native compile, and output publication failures into a
`CapturedBuildError` with a stable `kind()` and `Display` message. The private
`CapturedResolveError` remains private; map each variant to a public category
by a Rust match, never by parsing its text. An import parse failure, analysis
limit, solver unknown, and Cargo/tool error must remain distinguishable.

For the bridge's Rust lowering, use the monomorphization inventory of the IR
that passed the solver gate, or explicitly compare it with the legacy path.
The present bridge lowers using the `typed` inventory after checking a
taint-applied clone (`compiler/src/resolve/captured.rs:277-289`), whereas the
legacy native builder lowers with the `tainted` inventory
(`tools/anubis/src/main.rs:2640-2650, :2690-2697`;
`compiler/src/backends/native/mod.rs:187-192`). Do not assume they are
equivalent without a source-matched comparison. The legacy
`SymbolicEngine::generate_constraints(&src)` result is assigned to an unused
local (`tools/anubis/src/main.rs:2640-2641`), so omitting that call from the
captured route does not remove the visible solver-refusal gate at `:2651-2658`.

### Staged output publication and collision policy

The captured lane must not hand `compile_native_rust_to_exe` a stable final
path. After capture and successful check/lowering, open or create the output
root component by component under a held directory descriptor, refusing
symlinks and non-directory components. Reject `..` in the output path rather
than resolving it through an unexpected directory. Create a fresh,
tool-owned, private staging directory **inside that held output root** using
`mkdirat` with exclusive creation. Create the staged Rust-source and
executable leaves through that directory descriptor using
`openat(O_CREAT|O_EXCL|O_NOFOLLOW)`, never `std::fs::write` or `std::fs::copy`
to an unchecked final pathname. Split the current native compile helper so
it can return its private Cargo-built executable as a temporary product;
copy its bytes through the already-open staged executable descriptor. The
Cargo compile receives the exact checked Rust string, and the adjacent
staged `.rs` receives that same string. Check both staged outputs are complete
regular files, record their hashes, and fsync files and the staging directory
as appropriate for a durable publication claim. This change is necessary
because the current helper's final `std::fs::copy` follows the destination
path (`compiler/src/backends/run.rs:6310-6318`).

Publish the whole staging directory under a fresh unique name such as
`out/captured-<invocation-id>/` with `renameat2(..., RENAME_NOREPLACE)` relative
to the held output directory. The published directory contains
`anubis_out` and `anubis_out.rs`; this keeps the source/executable pair
together. If the final name exists, retry with a new unique name within a
bounded policy, then refuse if no fresh name can be created. Never overwrite
an existing regular file, symlink, directory, or previous captured build.
Refuse an output-root symlink or symlinked ancestor with a named
`ANUBIS_CAPTURE_OUTPUT_SYMLINK` error; no path-following fallback is allowed.
`RENAME_NOREPLACE` or a reviewed equivalent is required for atomic no-clobber
publication on this Linux lane. A kernel/filesystem that cannot provide it
returns `ANUBIS_CAPTURE_OUTPUT_ATOMIC_UNAVAILABLE` rather than silently using
a non-atomic copy. Fsync the held output directory after rename if claiming
crash-durable publication; otherwise state that the operation was atomic but
not yet crash-durable.

On capture, check, lower, compile, or publication failure, no final directory
from this invocation exists and the CLI does not print a native-artifact path.
Clean only this invocation's known staging directory after verifying its
identity; retain and report it if safe cleanup cannot be established. An old
`out/anubis_out` or older `out/captured-.../anubis_out` stays unchanged, and
the CLI never treats it as the new result. Repeat builds use new published
directories, even for identical source, so success cannot mean a stale cache
hit. This is an output identity and collision policy, **not** a claim that
Cargo, Rust, the filesystem, or hardware were formally verified.

## Refusals and precision controls

| Trigger | Proposed visible result | Why |
| --- | --- | --- |
| Declared dependencies | `ANUBIS_CAPTURE_DEPS_UNSUPPORTED` | No package mount or proof admission in captured graph. |
| Entry outside `src_root`, unportable name, invalid UTF-8, parse error, or missing captured entry | Typed `Entry` or `Resolve` failure, with module identity and stable code | Never substitute another entry or reopen the path; invalid input is not a contract disproof. |
| Reader unavailable, missing genuine procfs, symlink, special file, or source-root budget exceeded | `ANUBIS_CAPTURE_SOURCE_REFUSED` with the reader's precise reason | No fallback to legacy resolver or silent tree exclusions. |
| Research/Exploit function, nested block, or overwritten higher-mode attribute | Typed `NonSafeMode` or `ModeElevator`; visible `ANUBIS_CAPTURE_SAFE_ONLY` | The adapter neither grants Research consent nor substitutes a host isolation witness. |
| Safe-default typecheck failure or analysis limit | Typed `Typecheck(TypecheckFailure)` | Preserve semantic findings and limit separately; do not call an invalid program disproved. |
| Solver refusal | Typed `SolverRefused { checks, refusals }` | Preserve disproved, undecided, and invalid statuses from centralized `solver_outcome`; no display-text parsing. |
| Unsupported executable lowering or Cargo failure | Typed `Lowering` or `NativeCompile` | No analysis-only executable presented as the program; a compiler/tool failure is not a contract disproof. |
| Evidence/bounty, no-verify, or full-hybrid flag | Explicit option incompatibility error before capture/artifact | The narrow path has no truthful implementation of those promises. |
| Output root inside source root, symlinked output component, existing final name, or missing atomic rename support | Typed `Output` error with overlap/symlink/collision/atomic-unavailable reason | No stale executable is returned and no unreviewed path is overwritten. |

Do not reject ordinary Safe imports, embedded `std.*` imports, or valid traits
merely to achieve a passing subset. The private resolver already retains trait
sidecars and tests a same-named changed import body
(`compiler/src/resolve/captured.rs:682-723`). Check native behavior and
verdicts on the same frozen programs, including valid higher-order contracts
and a negative contract twin. A new refusal of a valid program is a precision
defect, not a successful security fix.

## Lead-run validation required before integrating

Use a source-matched lead-built compiler and a separate output directory for
each test. Record commands, exit codes, full logs, binary hash, source tree,
and the exact captured inputs; keep code and docs commits separate. No test in
this list was run by this worker.

- Focused Safe CLI integration: manifestless multi-file and manifest `src/`
  layouts; `main` imports a project module and embedded stdlib; compare output
  and diagnostics with the current build path. Include an imported trait and
  module/enum name control.
- Freeze capture, then replace an import body *under the same function name*
  before check/lower/compile. The emitted Rust and native artifact must carry
  the captured body. Do the same for the entry source. Mutation after capture
  must not change the checked verdict or compiled behavior. The existing
  private Rust-source test is a starting control, not a native witness.
- A violated contract in an imported module must refuse before native output;
  a satisfied twin must build. Pattern-match `CapturedSafeCheckFailure` in
  tests, not its display string. Exercise disproved, undecided, malformed
  solver status, typecheck diagnostic, and analysis-limit classes separately.
  Compare Safe-default effects/capability behavior with current `build`; do
  not count either command as a `--verified` effect seal. IFC v2 must still
  refuse the same Safe negative controls.
- Test hidden later/nested Research and Exploit items by **classification
  only**, without executing them: a block in an `if` expression, array-held
  lambda, match guard, and loop bound/invariant. Freeze an independent
  parse/classify control for each overwritten attribute spelling:
  `@research(...) @safe`, `@poc(...) @safe`, `@fuzz(...) @safe`,
  `@proof(...) @safe`, `@defensive(...) @safe`, `@audit(...) @safe`, and
  `@exploit(...) @safe`. Assert the frontend's final stored `Mode::Safe` where
  that is what it parses, but the shared predicate still returns true and the
  captured builder returns typed `ModeElevator` before typecheck or lowering.
  Include the same forms inside an imported module, nested module, impl, and
  trait method so the shared recursion cannot silently omit a boundary.
  Retain benign `@safe` and ordinary Safe functions as acceptance controls.
  Test unresolved, duplicate, cyclic, shadowed stdlib, and alias-collision
  imports separately.
- Test malformed/unknown-edition manifest, nonempty dependency map,
  outside-`src_root` entry, missing entry, malformed/invalid-UTF-8 entry and
  imported module, nested import, symlink/special file, capture limit, and
  platform refusal. Assert the precise typed stage and module identity for
  each. Verify that each error leaves no new native artifact and that a
  selected captured lane never falls back to the legacy resolver.
- Publish into a fresh output root, then repeat with identical and changed
  source: each success returns its own unique published directory; prior
  executable and `.rs` bytes remain unchanged; the new executable has the
  expected native behavior. Put an old `anubis_out` and a symlink at potential
  final names first; neither may be followed or reported as new. Refuse a
  symlinked output-root component and an output root inside `src_root`. Force
  a publication-name collision and atomic-rename-unavailable path through a
  test double: both must fail without a new artifact or altered old bytes.
  Force compile failure after staging and verify no final directory appears;
  only this invocation's private staging may be removed or reported.
- Test evidence/no-verify/hybrid flag incompatibilities, and unchanged legacy
  build/check/run behavior. Run the final focused formatter/lint suite and an
  independent review of the actual implementation diff, including output
  filesystem races and stale-artifact behavior. Broader suite and platform
  witnesses remain separate gates.

## Reviewable implementation breakdown

Keep the following as separate lead-integrated code units with source-bound
controls and final-diff review before enabling the CLI path:

- **Compiler check result:** retain private AST/graph ownership; add the
  share the frontend's complete Research attribute-alias mapping with the
  existing evidence predicate, apply the recursive
  mode-elevator guard before typecheck, and replace the bridge's String
  result with typed mode, typecheck, solver, and lowering failures. Unit tests
  are Safe-only or parse/classify-only for Research/Exploit sources.
- **Native staged publisher:** split compile from final-file copy; create
  staged source and executable by held output-directory descriptors, validate
  both, publish one unique directory atomically with no replacement, and
  surface typed output errors. Focused tests cover collisions, symlinks,
  compile failure, unchanged prior artifacts, and repeated builds.
- **CLI opt-in consumer:** add `Build.captured_source`, perform option,
  manifest-dependency, entry-relative-to-`src_root`, and output-overlap
  preflight before any entry read or output creation; invoke only the opaque
  compiler facade; print its exact new artifact path and Safe-default scope.
  CLI tests compare the legacy and captured lanes on valid and invalid
  dependency-free programs. Evidence flags are refused until the checker-input
  correspondence and verifier schema are reviewed.

The lead can combine these into one code commit if the final coherent diff
remains reviewable; this list assigns interfaces and gates rather than claiming
that separate commits exist. Documentation and the source-bound receipt belong
in a later docs-only commit under the repository discipline.

## Remaining dependencies after this unit

This consumer would bind Safe native build checking and lowering to one
captured compilation graph, but would not make the compiler sound as a whole.
`GraphCoverage::Compilation` excludes unimported package surface
(`compiler/src/package/source_graph.rs:215-223`). The manifest is parsed
separately from the prepared compilation graph. It may physically sit outside
the captured `src/` root or inside a root-without-`src` capture, but neither
case binds the manifest to the checked graph; a concurrent manifest edit is
also outside this correspondence. This unit must make no source-closure or
package-admission claim about it. A future versioned checker-input witness
must bind the selected manifest, lockfile, dependency bytes, compiler and
stdlib identities, exact parsed imports, AST/sidecars, check outcome, emitted
Rust, and artifact before evidence verification can attest a multi-file build.
The existing evidence verifier's multi-leaf refusal must remain in place until
that correspondence is independently checked. `CapturedTree` is not an atomic
view of an actively changing external filesystem
(`compiler/src/package/source_graph_reader.rs:1-5`); it is an immutable view
*after acquisition*. It currently scans unrelated files under the source root,
including prior `out`/`target`/`evidence` directories (`:6-7`). The proposed
output-overlap refusal keeps this lane's own repeated builds outside that
root, but existing unrelated generated files there can still cause a bounded
capture refusal. A selective source-closure acquisition policy and broader
layout support are needed before this becomes the default. Apple capture support, verified
package dependencies, `check`/`run` parity, proof execution, and platform
isolation witnesses remain open.
