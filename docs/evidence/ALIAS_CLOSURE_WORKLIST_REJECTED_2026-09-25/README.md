# Rejected closure-effects worklist prototype

The prototype is **rejected for integration**. It restores the required valid
`rv30_valid_n02` wrapper case, but accepts programs that release an annotated secret
through a previously captured printing closure. No compiler change from this
experiment is integrated. The alias candidate and its independent stack-overflow
failure remain unresolved.

## Observed differential

The finite controls were checked under the lead's memory-capped wrapper, with
complete logs, immediate exit codes, immutable pins and source hashes retained.
These were ordinary fixed safe-mode programs, not the known crash/stress fixtures.

| Control | Baseline ord3b | Alias ord3x1 | Rejected prototype |
|---|---|---|---|
| `rv30_valid_n02` | accepts | `ANUBIS_ANALYSIS_LIMIT` | accepts |
| Straight pure wrapper | `ANUBIS_ANALYSIS_LIMIT` | `ANUBIS_ANALYSIS_LIMIT` | accepts |
| Public captured value | accepts | `ANUBIS_ANALYSIS_LIMIT` | accepts |
| Original self-assignment printer and scalar twin | `ANUBIS_ANALYSIS_LIMIT` | `ANUBIS_ANALYSIS_LIMIT` | accepts |
| Original mutual-assignment printer and scalar twin | `ANUBIS_ANALYSIS_LIMIT` | `ANUBIS_ANALYSIS_LIMIT` | accepts |

The other proposed negative controls receive semantic exfiltration diagnostics on
the prototype. That limited success did not expose the straight-assignment capture
hole. Full observations and diagnostic classes are in
`closure-cycle-controls/results.json` and `closure-capture-controls/results.json`.

The accepted printer controls also ran through the normal safe `run` command,
without `--no-verify` or research permission. Their public output is `42` for the
secret input `42`, and `123456` for `123456`. The initial JSON run summaries retain
output hashes; the exact generated native binaries were then executed to retain
stdout/stderr bytes and check those hashes. Both observations returned exit code
`0`. The generated Rust, source, summaries, output bytes and hash comparisons are
retained under `closure-capture-runtime/`. Native binaries remain local and are
identified by SHA-256; they are not release artifacts. The buggy compiler's raw
`PASS`/`contracts_verified` fields are observations of its output, not endorsed
assurance claims.

The registered `ord3_capture_*` cases preserve the intended **REJECT** contract;
`case-map.json` maps them to the original source names. An analysis-limit refusal
does not establish a semantic security rejection. The complete matrix was not run,
and previous full-matrix totals must not be attributed to this expanded registry.

## Why the design failed

Native closures capture values before a later reassignment. Ordinary effect
application instead resolves free names in the current application scope. The
prototype deduplicates that incorrect cycle and never reaches the old captured
printer. Including whole-lane captures in the state key does not make ordinary
effects consume them. This is a transfer-function defect even with an injective
key. The replacement must preserve and apply ordinary captured binding versions
before introducing cycle deduplication.

`closure-cycle-independent-review.md` gives the source trace and blocking review.
`closure-cycle-prototype-audit.md` is retained as the original design audit, whose
semantic-completeness assumption was refuted; it must not be cited as validation.
Neither review is formal correspondence evidence.

## Source binding and limits

`closure-cycle-build/receipt.json` binds the immutable prototype binary to its
full delta from the declared base and a source manifest. The adjacent
`source.patch` includes the inherited alias delta; `closure-cycle-prototype.diff`
isolates the rejected worklist change. Baseline/alias binary hashes are in the
control results. Their source associations are inherited comparison provenance,
not fresh build attestations from this experiment. The alias association and
complete candidate source snapshot are retained in the earlier
[failed-workspace source manifest](../ALIAS_CANDIDATE_VERIFY_2026-09-25/source-manifest.json).
The existing [matrix history](../../../tests/soundness/matrix/history.tsv) associates
ord3b with checker source `e7b5650759bcd48cacd52ddf1498be3725343cfb`;
`comparison-pin-provenance.json` retains that lookup and the historical verification
summary. This experiment freshly verifies the comparison binary hashes and their
observed behavior; it does not independently reproduce their builds or establish
release-grade source-to-binary attestation. `manifest.json` binds the retained
evidence bytes.

The build succeeded, and only the recorded finite controls were executed. No full
workspace, matrix, corpus, stack-safety or performance pass is claimed. The known
recursive closure-source crash was not re-executed. Its follow-up still requires
the mandated disposable guest. The helper guard-extension proposal is a separate,
unvalidated candidate and is not part of this prototype.
