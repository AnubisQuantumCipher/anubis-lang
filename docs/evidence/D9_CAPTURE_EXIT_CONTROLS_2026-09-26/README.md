# D9 capture and early-exit controls — scoped registration (2026-09-26)

Fixture-only commit `49baf0bdfeb054bee41be3dd4492be0fbb13eb8d`
freezes four paired Safe-mode sources in
[`tests/soundness/matrix/cases/`](../../../tests/soundness/matrix/cases/).
The [registry](../../../tests/soundness/matrix/registry.tsv) records their
intended outcomes. The [machine receipt](gate-receipt.json) binds source
hashes, the source-matched CLI, saved outputs, the matrix harness, the
documentation-drift gate, and [independent read-only review](INDEPENDENT_REVIEW.md).
The [baseline](baseline.json) records selected diagnostic fields and each JSON
summary; the complete CLI JSON output is retained in `logs/*.stdout`.

| Case | Intended outcome | Observed Safe check |
| --- | --- | --- |
| `d9q_capture_early_g_invalid` | REJECT: a closure captures the earlier printing `g`, so a secret-guarded call leaks an output event | exit 1; typed `ANUBIS_IMPLICIT_FLOW` refusal from IFC v2 |
| `d9q_capture_early_g_valid` | ACCEPT: the closure captures the earlier pure `g`, so the later printing shadow is irrelevant | exit 0; JSON pass, no diagnostic |
| `d9q_secret_early_return_invalid` | REJECT: a secret-controlled return skips later public output | exit 1; typed `ANUBIS_IMPLICIT_FLOW` refusal from IFC v2 |
| `d9q_secret_guard_assert_continue_valid` | ACCEPT: a discharged assertion continues and leaves later public output unconditional | exit 0; JSON pass, no diagnostic |

The lead ran ordinary `check --message-format=json` with a SHA-identified
CLI built from code commit `700b81f4cc42e8efaab7a7f39aae4bf509188263`
and its recorded Cargo lockfile in a capped Linux guest scope. This is
selected-source Safe evidence. The invalid cases were refused by the existing
IFC v2 lane, **not** by a demonstrated direct D9 binding or continuation
producer. The direct producer, valid-program precision after repair, a native
runtime differential witness, the full matrix, and a release gate remain open.
No program was executed for this receipt.

The initial, unregistered early-return probe returned a public `i64` and
therefore had an additional independent public-return leak. Its original
source and outputs remain under [superseded-probe](superseded-probe/probe.json).
The registered source declares a secret return to isolate the output-event
question. The source identities differ; this is a pre-registration correction,
not a retroactive change to a matrix case.

The [independent docs review](INDEPENDENT_DOCS_REVIEW.md) initially held this
receipt for imprecise baseline wording, missing retained inventory-review
provenance, and ignored logs. The raw-output pointer above, versioned
[v1](../../mission/review/D9_V7_BINDING_INVENTORY_V1_HOLD_2026-09-26.md)
and [v2](../../mission/review/D9_V7_BINDING_INVENTORY_V2_HOLD_2026-09-26.md)
reviews, and staged logs resolve those publication conditions. The two
inventory patches remain unintegrated.

A preliminary documentation-drift rerun failed closed while this README was
being edited; its [derivation error](preliminary-drift-failure.err) is retained.
The final gate result applies to the stable candidate and is recorded in the
machine receipt. The failed attempt is not counted as a pass.
