# Struct qualifier precision registration — 2026-09-26

Fixture-only commit `d49f32b167136b1dac19022519e874cab7fe6990`
froze twelve `d9q_*` Safe matrix cases before an implementation change.
They pair public-field projections from a `Row` with a protected `k` field
against actual egress of `k`, the whole value, a `secret<Row>` wrapper, a
secret written into public `n`, and a comparison depending on `k`. The
independent read-only [design review](review-notes.md) gave GO for their
ACCEPT/REJECT intents and HOLD for a call-boundary-only exception. The same public-field
validity must survive direct reads, annotated and unannotated helpers,
aliasing, and forwarding.

The [selected baseline](baseline.json) records every source hash, the exact
registry SHA-256
`45f4b71cd9ca298c83e2ef0113be82b04988ac06b7a6dbe3f5d54e275c497d29`,
source tree, immutable compiler SHA-256
`06ab383684f3e953c613748938719fade05f573176457cb8133071a4e580df51`,
typed diagnostic classes, exit codes, and raw log hashes. Its JSON SHA-256 is
`7618e34fbfc9a90fec76eaf1259ae79fc7be8b9a99aff08aab6b5463a4a8d928`.
The [deterministically compressed raw stdout/stderr logs](logs.tar.gz) have
SHA-256 `d37905ff3516bb034db9c308e4492a156792b6732980ea5a7e1c25b28414e3f4`;
their members can be read with `tar -xOzf` and checked against `baseline.json`.
This is a local source-bound technical pin, not an independent rebuild.

At that pin, the clean-Row ACCEPT control passes. The other five ACCEPT
controls are refused: direct public `row.n` gets
`ANUBIS_SECRET_EXFILTRATION`; the annotated/unannotated, alias, and forwarding
helper controls get both `ANUBIS_SECRET_TO_PUBLIC` and
`ANUBIS_INTERPROC_EXFILTRATION`. All six REJECT controls are refused with
typed security diagnostics, not solver counterexamples. The public-`n`
constructor and write cases are egress negatives: acceptance of an unused
secret field initialization would not establish safe public release.

The mechanism is broader than D9's inferred formal. The legacy source walker
promotes any protected struct field to a root secret, then a direct public
field read inherits that root label. Separately, the parameter-to-egress
summary loses the field access path, and the call-boundary check treats the
nominal `Row` formal as wholly public. Repair must distinguish a structural
protected field from a whole-value secret, preserve `secret<Row>` and secret
writes, and carry the distinction through aliases, returns, and joins. The
existing whole-value lane can contribute but does not by itself erase the
legacy refusals. A narrow exception at one call site would leave the direct
valid control broken and could launder the negatives.

The fake-checker matrix harness [passed](fake-checker-harness.log) (log SHA-256
`6b9f33ff6333b80c5b641d6055c6a1d803ed494c696c0fb2ac662d97c42974df`)
and documentation drift passed after
registration. The full soundness matrix was not rerun; its canonical
expectation/source manifest is still absent. No current full-workspace,
hosted, disposable-guest, clean-room, production proof correspondence, or
release result is claimed.
