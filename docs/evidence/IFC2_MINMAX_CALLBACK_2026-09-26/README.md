# IFC2 min/max callback selection — local code receipt, 2026-09-26

Code commit: `6b85cf2207e5b3126a18e2df1190a4efa49a2ecc`.
At the prior pin, `min_by` and `max_by` called the callback with an abstract
element label but did not feed a key-selected incumbent into later callback
arguments. The registered Safe programs
`ifc2r2_minmax_callback_argument` and
`ifc2r2_minmax_callback_args_decided_by_results` checked clean while their
public callback transcript could depend on a secret key.

[`builtins.rs`](../../../compiler/src/middle/ifc2/builtins.rs) now models the
selected incumbent and revisits unknown-length callbacks until selection and
continuation labels stabilize. A bounded
[`repeatable.rs`](../../../compiler/src/middle/ifc2/repeatable.rs) summary
recognizes keys constructed identically from each frozen capture snapshot;
unmodeled calls, stores, control flow, callable alternatives, or budget
exhaustion disable only that shortcut. The callback effects remain checked.
The summary models the runtime's single supplied callback argument and
`Int(0)` padding for extra lambda formals. The original unannotated helper
signature is captured before D9 inference, so a later inferred type cannot
silently confer summary eligibility.

The independent static reviewer found no new soundness blocker in the final
diff and gave GO for source-bound testing. The committed-head technical pin
and toolchain are in [`build-pin.json`](build-pin.json). Focused full-Safe
integration tests passed, bounded summary unit tests passed, workspace
Clippy with `-D warnings` passed, `cargo fmt --all --check` passed, and the
docs-drift gate passed. Retained log SHA-256 values are, in that order,
`6a5e65953349a6ca4c6fd4c78a8b7d15bd8f7b32c2d29c0d3627491bc54dbf29`,
`9ff27fa94f1e4cc1972fc4d73ad36784246c7ce0304f2eecf3855c8d42e2a151`,
`d1a06272803c14a4e55ea3d0231d4349ce4cbce72357f2f5a8e62f96c3e4554e`,
and `fa407caff018254150f2fae760516d0eb214c5c1f753f2bddd9246eda9389a1c`.
An initial unit fixture had invalid `if` syntax; it was corrected before the
passing unit and lint runs. An initial full-Safe test exposed the pre-existing
D9 qualifier precision defect described below; the registered ACCEPT intent
was preserved.

## Exact selected Safe sources

The [source-bound comparison](selected-comparison.json) binds the registry
SHA-256 `94a63ed3f25eef1f96a318fab66c9deb2b2f6e27f7790aabb6180b6ace0b0575`,
every selected source hash, the prior binary SHA-256
`16705b9df6e971b31c0b1cf2a3a69a057a66d790d392951341cb53e718dddf35`,
and the clean-head binary SHA-256
`06ab383684f3e953c613748938719fade05f573176457cb8133071a4e580df51`.
Its only typed flips are the two original leak cases: PASS became a typed
`ANUBIS_SECRET_EXFILTRATION` refusal. The registered fixed captured-key
comparison, negation, helper, and padded-argument controls for both operators
still PASS. The two newly registered valid `Row` helper controls remain
refused as `ANUBIS_SECRET_TO_PUBLIC`; the same refusal occurs on the prior
immutable binary. D9 infers a public nominal formal for an originally
unannotated helper receiving a value with a secret field. These are **open
precision defects**, not accepted security outcomes. The IFC2-only test
checks that this slice does not add another refusal; it does not waive the
full-check acceptance requirement.

The [finite runtime witness](runtime-witness.json) records a normal Safe run
on two sources differing only in the protected `s` literal. The prior binary
accepted both and printed `1,2,1,3` for `s=42` and `1,2,2,3` for `s=0` on
successive lines. The clean-head binary refused the `s=42` source before
native execution with `ANUBIS_EXECUTION_CHECK_FAILED` and
`ANUBIS_SECRET_EXFILTRATION`. Normal Safe native runs of the registered
padded fixed-key min/max controls succeeded and retained their useful public
callback transcripts. The [source-bound valid rerun](runtime-valid-rerun.json)
records the registered fixture IDs, source hashes, command shape, compiler
identity, output, and generated native artifact hashes. No `--no-verify`
bypass was used. This finite witness
shows this leak mechanism; it is not a noninterference theorem.

The min/max refusals have typed status `refused`, not a checked solver
counterexample. This receipt establishes the selected check behavior and
finite runtime witness only. The full matrix was not rerun after this commit:
its performance/OOM cases require controlled investigation and the mandated
guest. The previous provisional full-matrix count belongs to the prior
registry and binary, not this head. Other IFC2 round-2 leaks, D9 precision,
the canonical matrix manifest, final workspace and hosted gates, independent
rebuild, production proof correspondence, and release acceptance remain open.
