# Independent read-only design review of frozen `d9q_*` cases

An independent review agent inspected the committed fixture sources and
registry intents after registration, without editing files or running a build
or checker. It gave GO for all twelve ACCEPT/REJECT oracles. Public `n`
projections from `Row { k: secret<i64>, n: i64 }` are valid when `n` remains
public; secret `k`, whole-Row rendering and comparison, `secret<Row>` wrapper,
and a secret initialized or written into public `n` must remain rejected at
egress. The review checked that runtime struct equality compares field values
in `compiler/src/backends/run.rs` and that the existing whole-value lane
tracks whole-Row binary expressions in `compiler/src/middle/whole.rs`.

The reviewer placed a call-boundary-only workaround on HOLD. In the current
`compiler/src/middle/mod.rs`, struct-literal source analysis joins protected
fields into a root label, direct field reads inherit that label, and
parameter-to-egress flow drops field paths. The reviewer recommended a typed
distinction between whole/scalar secrecy and protected-field containment,
with conservative treatment of unknown shape, aliasing, returns, joins and
writes. This is a design assessment, not a test result, proof of soundness, or
external expert review. The executable observations are in
[`baseline.json`](baseline.json) and its retained raw logs.
