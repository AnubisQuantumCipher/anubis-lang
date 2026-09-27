# IFC2 named proof-journal path control — local checker receipt

**Status: scoped Safe-checker repair; guest journal and release gates EXTERNAL / NOT RUN.**
The fixture commit is `7adba035a7a2b579906ac22648d770242b5c17fb` and the code
commit is `11086043538d732c4b1ddd1562792949c6a88e0d`. These are local,
unintegrated commits, not hosted or merged results. This receipt follows the
[terminal](IFC2_TERMINAL_EGRESS_LOCAL_2026-09-27.md) and
[zero-argument exit alias](IFC2_EXIT_ALIAS_ZERO_LOCAL_2026-09-27.md) receipts
without changing their historical observations.

Supported, well-formed `proof_commit_u32(name, value)` and
`proof_commit_bool(name, value)` write named values to the public proof-guest
journal. The prior IFC2 path refused direct secret values but did not test the
program counter that decides whether a commit happens. On the frozen cases,
the previous pinned checker accepted secret-controlled named commits. The code
now sends the committed value and current path through the existing `egress`
transfer. It reports `ANUBIS_IMPLICIT_FLOW` for the selected secret-path cases,
preserves `ANUBIS_SECRET_EXFILTRATION` for direct secret data, and treats a
tainted committed value as an integrity-sink refusal. That last change from
baseline `ACCEPT` to `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` is deliberate:
the existing information-flow policy says egress is also an integrity sink.
Legacy sink inventories still omit these names; this receipt does not claim
every analysis consumer now shares the classification.

Only argument 1 is committed data in the current guest lowering. The journal
name is derived from source identifier spelling rather than a runtime secret
value. The accepted secret-name case is a positional precision control under
that implementation, not support for dynamic journal names. The repair leaves
`proof_commit_u64`, invalid-arity calls, automatic `main` return commits,
`proof_assert`, first-class aliases, and proof-guest `u32` result-value parity
for separate review. It does not change the proof backend's executable image.

The retained private evidence bundle is named `anubis-ifc2-terminal-20260927`.
Paths below are relative to it; raw logs and checker pins are retained there,
not copied into the repository. Hashes identify the bytes, but do not by
themselves prove independent source correspondence.

| Bundle-relative artifact | SHA-256 |
| --- | --- |
| `proof-journal-pc-proposal/baseline-checker-20260927/receipt.json` | `0e69e625a5a3a9258a7b878592e0565460a45a50c76cf0f4160a6670a78c1f7e` |
| `proof-journal-pc-proposal/source-manifest-before-build.json` | `c46341e9f9fd4c9307bdeaef1b5cbdf5d46d98c8ce5264819647f305cb791b8a` |
| `proof-journal-pc-proposal/source-manifest-after-build.json` | `c46341e9f9fd4c9307bdeaef1b5cbdf5d46d98c8ce5264819647f305cb791b8a` |
| `proof-journal-pc-proposal/source-manifest-after-candidate-checker.json` | `c46341e9f9fd4c9307bdeaef1b5cbdf5d46d98c8ce5264819647f305cb791b8a` |
| `proof-journal-pc-proposal/source-bound-build-result.json` | `1309accf2ecb107ea5220380772fdcbf8dad251e6cf767eef71a1205370dd4a6` |
| `proof-journal-pc-proposal/source-bound-build.log` | `249a14a40c4421496f75ff3d68fc712333d1c307d2572b196a7f8ffb7959c296` |
| `pins/anubis-11086043538d732c4b1ddd1562792949c6a88e0d` | `f8b94e68a68e9962b14564617fdd15fb34e4300925f303dfee311fece8c6dfb3` |
| `proof-journal-pc-proposal/candidate-checker-20260927/receipt.json` | `812d6bb3ce7f2c6b1725fe28029f18605a8522eaf6595cd1ed5e3d218cab5e31` |
| `proof-journal-pc-proposal/run_candidate_checker.py` | `33a92db1f59b05c2fb7037ad8736538ecde152ba5a3594da857fdaf5cc8a52c9` |
| `proof-journal-pc-proposal/targeted-rust-test.log` | `a6f782604bb31451d7ae70fce700837e528d54681394387d08bcd1f0f6b631a7` |
| `proof-journal-pc-proposal/clippy-all-targets.log` | `2a84fd5645e8e19065b32b3cc49ced20d34d2f8c982566764c869f9026dc66f2` |
| `proof-journal-pc-proposal/fmt-v2-result.json` | `752a73bd52b82c360b99025021a330517acf37de9849296c34c078507d0fa3ea` |
| `proof-journal-pc-proposal/APPLIED_CODE_FINAL_DIFF_REVIEW_2026-09-27.md` | `69cce055ae2ef8e35fa1ba7ec567b3017f81196ed92d1132fd0393d00105e4a4` |
| `proof-journal-pc-proposal/CANDIDATE_CHECKER_INDEPENDENT_REVIEW_2026-09-27.md` | `30903b952ae001aef4e58047b0308d741b127a796cbdc9cc00235ae64318cdc2` |

The prior read-only checker pin predates the fixture commit; the baseline
receipt binds each new source byte hash rather than implying the fixture was
in that older tree. Its `source_commit_label` identifies the old compiler pin,
not the revision where the new fixture was added. The secret-guard fixtures
initialize their secret from a fixed literal; they probe the static path
transfer and are not an executable varying-secret disclosure witness. The
bounded, locked, offline release build exited successfully at the code commit.
The source manifest matched before and after
the build and the candidate checker run, the worktree was clean at build time,
and the earlier immutable pin remained unchanged. The manifest includes
pre-existing ignored generated inventory, so this is a stable local build,
not a clean-room reproduction. The focused Rust tests, format check, strict
all-targets Clippy, and documentation-drift check passed on the code tree.

The candidate receipt reports `FOCUSED_MATCH` on the frozen registered cases:
the selected secret-path commits changed from `ACCEPT` to the exact typed
implicit-flow refusal, and the tainted-value case changed from `ACCEPT` to
the exact typed integrity refusal. Direct secret-value refusal and all
selected valid controls retained their expected outcomes. The older selected
proof-bool controls also retained their typed outcomes across the previous
and candidate pins. Every recorded candidate check had no tool or JSON
protocol issue. These are Safe-checker observations only: no Anubis program,
proof guest, native journal, or evidence verifier was executed.
The runner records post-check hashes but does not make their equality a grading
condition; the independent reviewer compared them for this retained receipt.
That grader hardening remains follow-up work.

The full soundness matrix remains an isolation hold on this ordinary Linux
VM because it includes crash-capable inputs. The Linux nested-KVM benign-boot
receipt is not a Research or crash-workload authorization. File-device egress,
complete proof-journal semantics and application forms, native/guest behavior,
formal correspondence, whole-workspace gates, hosted CI, external review, and
both mission finish lines remain open.
