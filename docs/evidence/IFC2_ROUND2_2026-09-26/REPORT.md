# IFC v2 round-2 evidence recovery — 2026-09-26

Later integration update: the [registration receipt](../ROUND2_REGISTRATION_2026-09-26.md)
maps the historical leak carriers into the soundness matrix and records focused
current checker observations. The recovery facts below remain historical; the
registration does not turn the complete round into a current green result.

This is a provenance recovery and source-review report, not a current compiler verdict or a
closure claim. No recovered program, historical harness, compiler, or crash probe was executed.
The lead supplied worktree baseline `f7e906d1`; this worker did not use Git, build, stage, or
edit the defect registry or compiler.

The recovery command reports **75 findings**, **75 historical confirmed verdicts**, and no
missing verifier results or full source programs. Its category counts are **41 leak**, **28
overrefusal**, **6 robustness**. The former `review2/found.json` contains only **51** entries;
it is preserved as a partial historical extraction, not used as the complete finding list.
See [recovery-summary.json](recovery-summary.json).

## Reviewable entry points

- [inventory.tsv](inventory.tsv): every finding's ID, category, lens, canonical program,
  original and independent evidence location, expected semantics, execution hazards and current
  recheck requirement.
- [inventory.json](inventory.json): the same mapping with the original finding and verifier
  records intact, including root cause, original output pairs, exit statuses, runner, notes,
  verifier input substitutions, stdin and setup records where present. **Every current status
  is `unrechecked`.** This describes this recovery worker's evidence boundary; subsequent lead
  verification belongs in separate receipts.
- [src/](src/): full programs recovered from the review. One exception is explicitly documented:
  `r2ctl_R01_shift_register_hits_loop_limit` embeds pseudocode in the report. Its canonical
  program was recovered from `control/p/r08_delay64.anb` and checked byte-for-byte against
  `verifier/p/r2ctl_R01_shift_register_hits_loop_limit.anb`. The pseudocode remains untouched
  in the original review.
- [original/review-ifc2-r2-raw.txt](original/review-ifc2-r2-raw.txt): complete workflow result,
  with lens provenance, notes and independent verdicts. The `.txt` is JSON. The separately
  retained [raw JSON](original/review-ifc2-r2-raw.json) carries the same finding IDs but lacks
  the finding-level `lens` field.
- [original/review-ifc2-r2.js](original/review-ifc2-r2.js): original workflow definitions,
  review scope, pin names, report criteria and verifier task.
- [verifier/results.json](verifier/results.json), [verify.log](verifier/verify.log),
  [verify.py](verifier/verify.py), `verifier/findings_*.json` and `verifier/p/`: original
  independent verifier outputs, execution recipe and source/twin files. **Do not execute
  this historical harness on the host.** It contains old runner paths and mutating setup.
- [original/probe-text-evidence.tar.gz](original/probe-text-evidence.tar.gz): original review
  probes, outputs, controls, traces, harnesses and retained pinned source snapshots. Its
  uncompressed text includes large historical traces. It contains no included native binaries
  or runtime/build directories. Inspect selected members instead of expanding the entire
  archive during routine review.
- [source-manifest.json](source-manifest.json): direct-copy source paths, hashes and lengths.
  [archive-manifest.json](archive-manifest.json): every archive member's original source path,
  hash and length, plus explicit exclusions. These hashes establish recovery integrity, not
  compiler soundness or authorization of the historical runs.

`recover.py` is a copy-only recovery recipe for the original cache paths. It never invokes the
compiler or any archived harness. Re-running it intentionally overwrites these recovered
artifacts, so do not run it while a lead is sealing this evidence directory.

## Evidence limits and missing current proof

The original review names `anubis-ifc2-v8e`; lens notes identify source commit `d6801d4d` and
warn that the original worktree changed during review. Retained source snapshots explain its
line citations. `anubis-ord3x4l` supplied the other-lanes baseline; runtime replays sometimes
used `whole10` or `ordret2a` because the newer runner refused the valid program under study.
These outputs do not establish behavior of the current tree.

The verifier harness records memory-capped native `run --no-verify` commands. It does **not**
record a disposable-guest identity or provide a current isolation receipt. Historical outputs
are therefore retained as historical observations, never re-labelled as authorized isolation.
Old-runner trap output also cannot establish that the newer runtime leaks the same operand.
Current runtime, crash, exploit-kit and resource-exhaustion replay requires the repository's
mandated disposable guest. An unavailable guest is a block on that replay, not permission to
run the probe on the host.

The workflow's notion of a historical leak used differing outputs with IFC-only acceptance;
it is not automatically the repository's stronger direct-versus-laundered acceptance test.
A current closure receipt must include direct refusal controls and actual full-check behavior
where relevant. Similarly, equal outputs for the supplied pair of inputs are bounded witness
evidence, not a noninterference theorem. A robustness probe producing a controlled limit is
not, by itself, proof that the reported precision or performance problem is fixed.

No current pin results, current resource measurements, new runtime twins, current full matrix,
corpus, or isolated replay results were produced by this worker. All remain required before
claiming the round closed. Companion observations in the lens notes (such as proof-journal PC
behavior without native observability) are retained as observations, not silently promoted to
additional confirmed findings.

## Safe next implementation units from source review

The following are source-level leads, not validated fixes. Current source inspection reproduced
the reported code shapes at the start of this recovery. The lead is independently implementing
`min_by`/`max_by`; registry IDs `ifc2r2_minmax_*` are reserved for that work.

| Mechanism | Finding IDs | Current source anchor inspected | Required behavior and siblings |
|---|---|---|---|
| Callback arguments selected by prior key results | `r2_fn_L1_min_by_max_by_callback_argument`, `r2p_06_min_by_max_by_callback_args_decided_by_results` | `compiler/src/middle/ifc2/builtins.rs:691`, `min_by`/`max_by` arm | Model the running extremum's key dependence when analyzing subsequent callback invocations. Preserve public callbacks and secret payloads with public shape controls. Parent owns this unit. |
| Narrowed receiver loses optional fields | `r2ctl_L06_method_narrowing_drops_maybe_fields` | `compiler/src/middle/ifc2/eval.rs:2111`, `method_call` | Copy the chosen type's presence metadata alongside its fields. Ensure future order/shape metadata survives narrowing too. |
| Runtime defaults disappear from abstract kinds | `r2p_10_try_on_empty_payload_gives_bottom`, `r2p_11_join_enum_drops_runtime_default_for_short_payload`, `r2p_12_struct_string_index_ignores_maybe_field` | `eval.rs:2346` `try_value`; `value.rs:1601` `join_enum`; `eval.rs:2893` `index_read` | Missing payload/optional field can yield public runtime `Int(0)`. Preserve that alternative so method fallback arguments are evaluated. Cover Some/Ok, positional payload joins, named fields and string indexing together. |
| Bool proof-commit result has wrong kind | `r2p_13_proof_commit_bool_result_modelled_as_argument` | `compiler/src/middle/ifc2/builtins.rs:186` | Model the bool commit's scalar runtime result instead of returning the argument's shape. Audit native and guest stubs and keep journal disclosure checks. |
| Computed remove and maybe-key merge | `r2v_07_remove_computed_key_keeps_known_keys_present`, `r2p_05_remove_by_computed_key_keeps_keys_present`, `r2v_08_merge_drops_first_map_value_when_second_key_maybe`, `r2p_04_merge_drops_first_map_value_for_maybe_key` | `compiler/src/middle/ifc2/builtins.rs`, `mutate` remove and `merge` | A computed removal can remove any known key. If the winning merge map may lack a key, retain the losing map's value. Cover later secret insertion and direct-value controls. |
| Nested writes invent presence | `r2v_05_nested_write_into_missing_key_makes_it_present`, `r2v_06_nested_write_into_missing_field_makes_it_present`, `r2p_03_set_path_nested_write_marks_missing_key_present` | `compiler/src/middle/ifc2/eval.rs:1425`, `set_path` | A nested write into a missing field/key is a runtime no-op; distinguish terminal insertion from traversal and preserve optional presence across field/index forms. |
| Folded values skip concrete pattern decisions | `r2v_09_pattern_decision_top_skips_concrete_parts` | `compiler/src/middle/ifc2/eval.rs:3009`, `pattern_decision` | A value may be top joined with concrete kinds. Join both contributions instead of returning early at top; cover enum, struct and list subpatterns. |
| Declared explicit egress ignores PC | `r2ctl_L08_panic_under_secret_pc_not_egress`, `r2ctl_L09_exit_status_under_secret_pc_not_egress`, `r2_fn_L4_panic_and_exit_under_secret_pc_not_refused`, `r2p_09_panic_exit_under_secret_pc_not_refused` | `compiler/src/middle/ifc2/builtins.rs:160` and `:171` | Enforce D1 for the panic message and observable exit status while preserving continuation lifting and the special no-argument exit behavior. Proof commits require a separate guest-journal claim boundary. |

Larger mechanism units need a deliberate representation change, not a refusal-only shortcut:
ordered map/struct shapes; maybe-presence equivalence under select; named enum payload joins;
statement/return merges that preserve equal shapes; precision of closure/recursion summaries;
flow-sensitive/path-sensitive stores; and worklist-based recursive solving with resource
bounds. Their complete finding mappings and reported mechanisms remain in the inventory.
Changing one walker or one shape arm is insufficient under the repository's walker-parity rule.

Runtime message redaction is distinct from rejecting a leaking source program: after operands
are removed from a trap, a source may legitimately pass because trap occurrence is outside the
stated termination-channel claim. Closure must then demonstrate redaction in every sibling
runtime and preserve rejection of explicit egress. Inventory rows explain this distinction.
