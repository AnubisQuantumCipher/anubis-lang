# Match guard write transfer — local code receipt, 2026-09-26

Code commit: `88a2903c6534843d1c831ac47b11d27e87b1f114`.
The change carries a failed match guard's write into later arms and keeps the
pre-write value at calls that execute before a later binder write. It also
restores the reached, statement-free `if let` call whose shadowing binder had
silently hidden a violated `requires`. The mechanism is in
[`compiler/src/middle/mod.rs`](../../../compiler/src/middle/mod.rs); exact Safe
CLI controls are in
[`match_guard_write_transfer.rs`](../../../tools/anubis/tests/match_guard_write_transfer.rs).

The frozen staged patch `9c20c471a1ee5e06eb0d99081c288ae35bba3284d3cec5b720b392a7094c74e8`
received independent static GO for source-bound testing. On the committed
source, the focused CLI test passed, the adjacent filtered match tests passed,
workspace Clippy with `-D warnings` passed, and `cargo fmt --all --check`
passed. Their retained local logs have SHA-256 values
`cddc76b8ceffc0c627ef4b8c47918c1c6a814fada2fd33d5fb6056265d3662f0`,
`67351d8133f93572ed244d159a9706e53b527bd212bb0a2651afa409ab7a500a`,
and `d545f5d7889475743d81887fe7c5646106133741145c8253c4b3b0b439995005`,
respectively. The clean-head technical binary and build-log identities are in
[`build-pin.json`](build-pin.json). This is a local source-bound build, not a
clean-room reproduction.

## Same-source matrix comparison

The frozen registry SHA-256 was
`514d9c2297f1b0ab17da8f490e82278a2d5a081558f1f7f373422500b3f9da95`.
Both complete provisional runs used that registry and runner SHA-256
`ff388fde5209e8bb40c02483050df1281397f0bcb9ba54bdbe5ca27e89dcd5aa`.
The immutable baseline binary SHA-256 was
`8b85333dc2615b13d2d044cf19d421a2d40e7c5f6d4b63145465ee400516fe2b`;
the clean-head candidate was
`16705b9df6e971b31c0b1cf2a3a69a057a66d790d392951341cb53e718dddf35`.
The runner recorded a result row for every registered form, including tool
timeouts. The exact per-form results are in the
deterministically compressed [baseline](baseline-matrix-receipt.json.gz) and
[candidate](candidate-matrix-receipt.json.gz) receipts, the compact
[verdict table](matrix-verdicts.tsv.gz), and [comparison](comparison.json).
The uncompressed receipt SHA-256 values are
`183eb417977b3e18a2b6bbdfda128b348b53d454f5d0e90cfe24e845911889d6`
and `4b382041c5bef7aaec9af59b91640bcfa2e8bef0dfc508b67137ae205924d485`.
They can be recovered with `gzip -dc`; the typed comparison and source hashes
are reviewable in those artifacts.

The observed typed changes were:

| Registered forms | Baseline → candidate | Classification |
|---|---|---|
| `r2arm_nested_iflet_binder_call_invalid` | PASS → DISPROVED | Former silent accept now has a checked counterexample. |
| `r2arm_guard_write_binder_shadow_invalid`, `r2arm_guard_write_failed_invalid`, `r2arm_guard_write_stale_nonmatch_invalid`, `r2arm_guard_write_stale_prior_nonmatch_invalid` | UNDECIDED → DISPROVED | Violated fallthrough calls now use the post-write value. The `stale_nonmatch` source duplicates `failed_invalid`; the distinct `stale_prior` case remains separately registered. |
| `r2arm_guard_write_failed_valid` | UNDECIDED → ACCEPT | Required valid fallthrough restored. |

The valid call before a later binder write stayed accepted; direct violated
controls stayed disproved. A normal Safe run of the harmless binder witness
was refused before native execution with
`ANUBIS_ASSERTION_DISPROVED`; its compact
[run control](runtime-refusal.json) retains the source, binary, and output
hashes. The earlier source-matched baseline ran that witness and entered
`f(0)`; see the [registration receipt](../SHADOWED_MATCH_IFLET_REGISTRATION_2026-09-26.md).

Four valid performance forms changed from ACCEPT to TOOL_TIMEOUT at the
unchanged runner limit: `rv28_valid_perf_2_value_block_nesting_exponential`,
`rv28_valid_r28_p2_nested_value_block_match_blowup`,
`rv30_valid_perf1_oom_self_recursive_cycle`, and
`rv30_valid_perf2_timeout_callsites`. Their cause is unresolved, and the
uncontrolled run does not establish performance non-regression. The forms remain open
tool/performance observations. The OOM-designated case must not be rerun on
this Linux guest under the repository's disposable-guest rule.

Both matrix receipts deliberately report **INCOMPLETE**: typed expectations
and the reviewed canonical manifest are not integrated, and the runner does
not claim a source-commit-bound binary. The candidate provisional run still
records 39 silent accepts and 115 valid wrong-class primary cases at this
frozen registry. Subsequent min/max fixture edits changed the live registry;
this receipt is historical for the hash above. No full workspace, hosted,
disposable-guest, independent rebuild, or release gate is claimed.
