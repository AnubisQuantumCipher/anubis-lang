# IFC2 terminal egress: scoped local receipt (2026-09-27)

**Status: local, unintegrated checker-only repair. The terminal family remains open.**
The code commit is `3b8d9bfe7292d59c05ec7e042ccf31c03be32aa1`, stacked on the
local D9q code commit `73d78dbfee5b2d0329546caf31caf59347c5df1d`. The
fixture-only commits are `20ca203cfd2cf674a74204aec986f5a38264a05d`
and `25ccb2e0ac3dd3eff15930023fd2a2ef8c682aa9`. These are local
commit identities, not merged or hosted results.

The IFC2 builtin transfer now sends `panic(message)` and executable
`exit(status)` through the existing `egress` transfer before ending the
program path (`compiler/src/middle/ifc2/builtins.rs`). That transfer checks
the explicit data label, the current program counter, and tainted sink data
(`compiler/src/middle/ifc2/eval.rs`). Direct `exit()` remains the documented
runtime no-op. The change leaves other analyses and public-output boundaries
in place; it is not a complete terminal, file, or proof-journal policy.

The retained local bundle is named `anubis-ifc2-terminal-20260927`. Its raw
logs and binary are private Work artifacts, not included in this commit.
The digests below identify the exact retained files for review; this Markdown
file is not an independent verifier.

| Bundle-relative artifact | SHA-256 |
|---|---|
| `source-manifest-before-build.json` and `source-manifest-after-build-v2.json` | `c978afad88cb38ce31dbfad92638785a1d54662a3a85cd5a86df12b1590cc50d` |
| `build-v2-result.json` | `ff02feefcb7a9a13de58587186895811d841cb0872a768954841e62fadd4f3a2` |
| `pins/anubis-3b8d9bfe7292d59c05ec7e042ccf31c03be32aa1` | `78e6b7534ae9e80b348d4afe6431f019df187dbdb4b30ea122e957c249812472` |
| `paired-3b8d9bfe-fullfrozen/receipt.json` (original grader) | `475fae7feeed8ef032bc8b431e8795e4c63fba1bd5fccbc9be2b767dcabe8ae6` |
| `ifc2_terminal_probe_v2_frozen.py` | `bcd9dda352115129a645c6c1f38a9e13a4561c72af15b49d0527a0e91ad57c82` |
| `paired-3b8d9bfe-corrected-v3/receipt.json` | `6fea271264584537887ce5ee2eec3fff6fdde9cf9cab2817f24cb82079eceb69` |
| `ifc2_terminal_probe_v3_frozen.py` (corrected grader) | `5f8e64730efcf84a42917625ea1f8661b8020ea95cfb18a866362ec106946b97` |
| `paired-3b8d9bfe-confined-v4/receipt.json` | `6e5cac35c60ee3972582d64a8934318d94924c5b55c373f8bc78b23932ff74f0` |
| `ifc2_terminal_probe.py` (Work-confined grader) | `12717f061c9a834e86d2c254206ea24d6fa5845d70ebfa67b9380aa57c0ec989` |
| `source-manifest-before-v4-probe.json` and `source-manifest-after-v4-probe.json` | `1cbeaa2449a5378a73126aad6fb670c848e0f76626d82a9be8a63aebbfd10c07` |
| `v4-v3-identity-comparison.json` | `36b0f34d2a307472d9113b9e818a5eec8c4dd61a83596e1923888beb2ac891ce` |
| `IFC2_V4_CONFINEMENT_INDEPENDENT_REVIEW_2026-09-27.md` | `b93f816da3424c43bae9c71cb126a1d63e081268b172e145d9ee130b02ad57be` |
| `clippy-all-targets-result.json` | `281a576888e7b9ec653c3a05cfc2457f219f93ab0498c17cbb4bc0468548439b` |
| `IFC2_TERMINAL_FINAL_DIFF_REVIEW_2026-09-27.md` | `2201003a15649a9bd2b6f126a032c84d5c26aa54f196176e73fe1186ffbbe09b` |
| `alias-zero-proposal/checker-probe-20260927/receipt.json` | `f1151b7a2bc14b6a1d1380fc0c36b6aab9a0aee7e3f6b15ca1f81ec476186976` |

The offline, locked release build ran as a memory-controlled user service and
exited successfully. Its before/after source manifests match, and the copied
candidate checker is read-only and hash-matched to the build receipt. The
manifest includes ignored generated `tests/soundness/matrix/out` files that
predated the build; it is not a tracked-source-only closure or a clean-room
reproduction. The earlier launcher attempt failed before Cargo because the
user bus was unavailable and remains a separate failed receipt.

The corrected focused paired receipt reports `FOCUSED_MATCH`: every selected
frozen case matched its unchanged `ACCEPT` or `REJECT` intent. The historical
secret-controlled `panic`/`exit(status)` cases, first-class status-argument
aliases, and tainted terminal arguments changed from `ACCEPT` to typed security
refusals. Valid public calls, shadowed user functions/local closures, direct
no-argument `exit()`, and declassified-condition controls stayed `ACCEPT`.
The v4 rerun confined every checker's working directory and observed
generated checker output to a fresh private Work child. Its source manifest
stayed byte-identical before and after, and an exact typed-result comparison
with v3 found no
changes in selected source bytes, pin identities, diagnostics, or grades.
An independent reviewer also checked the completed output tree and raw JSON
streams. The v4 runner does not yet enforce a final generated-output inventory
itself; future runs need that internal check.
That stable v4 manifest includes this uncommitted documentation draft and
older generated mono inventory; it does not retroactively make the build's
source closure tracked-only. `ANUBIS_IMPLICIT_FLOW` is required for the
secret-control cases; the tainted cases emit
`ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`. No Anubis program was executed by
this focused checker probe.

The original paired run remains `HAS_FAILURES`. Its Work-only exact-code
grader mistakenly expected the prefix `ANUBIS_TAINTED_SINK` for two tainted
cases; production has long emitted the full `_WITHOUT_DECLASSIFY` code. The
committed registry required typed taint refusal without that mistaken literal.
Independent review confirmed the grader error. The old runner bytes and
receipt were preserved. The versioned correction changes those two expected
codes without changing fixture source, registry intent, or exact-code grading;
the complete frozen pair was rerun with the same checker pins in a fresh
directory. The failed receipt has not been relabeled.

`cargo clippy --locked --offline --all-targets -j 1 -- -D warnings` exited
successfully, and the worktree's tracked source stayed clean. Its source
manifest comparison did **not** stay stable because checker runs generated
ignored files under the source worktree while the lint ran. All generated
files were retained in the vault-pinned Work bundle and the build's original
source manifest was restored byte-for-byte; no broad clean or deletion was
performed. A stable-manifest lint run remains a next-candidate gate. The
documentation-drift check passed on the code tree and was rerun on this
documentation draft; it must be repeated after any further edit.

**Open boundary found in final review.** Both pinned checkers accept an
exploratory `let stop = exit; stop()` under secret control. The IFC2 transfer
models that zero-argument alias call as the direct no-op, while native
first-class builtin lowering dispatches only positive arities and appears to
raise `ANUBIS_ARITY`. This is a source-level runtime divergence hypothesis,
not an executed leak witness. The exploratory sources are not yet registered
matrix cases. Resolve alias arity semantics, freeze the valid and invalid
controls, and obtain an authorized disposable-guest runtime witness. Named
proof commits still lack a program-counter check; file-device egress remains
open separately. The independent final-diff reviewer gave scoped GO for the
recorded checker-only status-argument repair and HOLD on terminal-family
completion.

The full soundness matrix is not a compliant gate witness on this ordinary
Linux VM: registered crash-capable inputs require the repository's disposable
guest lane. No full matrix, workspace suite, hosted CI, native terminal
witness, formal correspondence, self-host bootstrap, external expert audit,
or release seal is claimed. Neither Anubis 1.0 product completion nor full
trust-chain completion follows from this slice.
