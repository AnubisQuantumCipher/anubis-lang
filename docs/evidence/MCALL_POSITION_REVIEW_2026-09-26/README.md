# Same-source match contract review

This is a **withheld repair review**, not a soundness closure. The technical
baseline pin from compiler commit
`b835b94b44fa8705dd973eb1133f862706fd04a8` and the provisional repair
pin from `8f1589410c85a0f5b982913cb6a8bf089e89f464` checked the same
tracked fixture bytes. [paired-results.tsv](paired-results.tsv) records each
source hash, its introduction and current-content commits, the pin identities,
both typed checker outcomes, and hashes of the ignored raw logs.
[verdict-flips.tsv](verdict-flips.tsv) selects rows whose observed class changed.
Direct twins are retained as comparisons, even when they disprove the contract;
they are not counted as closure of their carrier cases.

The repair now disproves the violated calls in a match scrutinee, a match
guard, an if-let scrutinee, and a guard assignment right-hand side. It also
disproves the reached nested `pred() && x > 0` invalid control that the baseline
silently accepted. These improvements remain bounded by the following review
failures:

- `r2arm_partial_guard_pred_false_valid` changes from `ACCEPT` to a
  `requires@f` `DISPROVED` result even though `pred()` returns false before
  the call. This is a false disproof of a valid program.
- `r2arm_duplicate_direct_before_invalid` and
  `r2arm_duplicate_direct_after_invalid` change from checked `DISPROVED` to
  `UNDECIDED` when a second call with the same displayed obligation name is
  present. Their direct-only twins remain `DISPROVED`. A refusal is not a
  substitute for the lost counterexample.
- The nested nonmodelable-if violated case changes from silent `ACCEPT` to
  `UNDECIDED`, and its valid untaken-body control also changes from `ACCEPT`
  to `UNDECIDED`. Neither has the required final class.
- `r2arm_expr_match_untaken_guard_alias_valid` is `DISPROVED` on both pins.
  This is a pre-existing precision defect, not a new repair regression. The
  live-guard invalid twin is `DISPROVED` on both pins.
- M-ARM-BINDER still contains sibling-binder false accepts and same-binder
  undecided valid controls. Their unchanged verdicts remain in the paired
  table and append-only history.

The `r2arm_arm_tail_write_fact_invalid` carrier was introduced at
`db367d4911d364e0a7f8477382c3e731a780cbbd`, then corrected at
`17107ab4acc2f27d0c1f98118305e09d96facf09` to give it a zero-argument
entrypoint that calls the helper. Its corrected source SHA-256 is
`83050822ce7bbff42980022dd26f6a9fd7d01e99797bb946ce40487f940c5958`.
The lead reran that exact source on both pins; each returned
`ANUBIS_ASSERTION_DISPROVED` for named `requires@f`. The committed
[baseline](corrected-arm-tail-baseline.json) and
[repair](corrected-arm-tail-repair.json) check summaries record the source hash
and diagnostic; the manifest records their hashes. The old focused-table row
for the pre-correction source is superseded, not silently reused. Its direct
twin remains the separately measured comparison. This is a passing rejection
control, **not** a confirmed stale-branch-fact false accept.

The baseline and repair focused runs originally covered the fixture tree at
`db367d49`; the corrected arm-tail row uses the separately hashed rerun logs.
A documentation-drift run logged after `db367d49` reported its stamp count
and zero drift. Its log contains no HEAD or clean-tree marker, so it is not a
source-bound gate witness. A later run on the uncommitted integration worktree,
after the arm-tail correction, also reported PASS and zero drift; the manifest
records its ignored log hash, but it is likewise not a source-bound gate witness. The
repair binary was built at `8f158941`; the later `compiler/src/lib.rs` change
adds a source-bound regression test, so a broad compiler-input diff to
`17107ab4` is nonempty. This receipt does not claim a binary rebuilt from the
later tree. Neither the final test suite, full matrix, mandatory guest lanes,
hosted gate, independent final-diff review, nor release seal was run for this
review candidate.

Matrix history appends baseline rows only for fixtures introduced at
`db367d49`, and repair rows for the full focused comparison. The repair label
is explicitly `mcall-repair-withheld-8f158941`. The next implementation must
preserve the named disproofs while restoring valid paths and direct-call
counterexamples, then receive a new source-bound pin and independent review.
