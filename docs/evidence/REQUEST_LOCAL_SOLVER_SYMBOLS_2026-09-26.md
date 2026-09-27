# Request-local solver symbols: scoped local receipt

Code commit `e7187a314db5385e2d34fd74dd3b773878112195` replaces two
process-global counters used in contract and match obligation construction
with a counter owned by each `SemanticContext`. The same request shares one
namespace across `contractarg`, `armbind`, `mbind`, and `colidx` symbols.
Exhaustion emits `ANUBIS_SOLVER_SYMBOL_EXHAUSTED`; it does not wrap to a reused
name. This addresses cross-request query drift and collisions within the
covered semantic context. It does not establish stable obligation identities
across source edits or solve the remaining name/binding semantics defects.

An independent read-only reviewer approved the applied final diff, including
the lead's test-only Clippy correction. The reviewed standalone patch SHA-256
was `64ad13eeb2fb714f5f5315211e1883274feaba94b7f15a19586d850ddbd02b7d`.
Lead-owned focused Safe tests passed for distinct symbols within one request,
identical contract/match obligations across repeated typechecks, exhaustion,
and byte-identical cold-process CLI solver/proof-index/SMT outputs. The
inline-contract focused suite passed, as did workspace Clippy with
`-D warnings`, formatting, diff, and documentation-drift checks. The first
Clippy attempt found a test-only `field_reassign_with_default` warning; its
complete failure log and the corrected passing log are retained.

The lead built a release CLI from that clean commit on Linux AArch64 with
`rustc 1.97.0-nightly (82bee9650 2026-05-09)`. The binary SHA-256 is
`8b85333dc2615b13d2d044cf19d421a2d40e7c5f6d4b63145465ee400516fe2b`;
the retained local binary/build manifest SHA-256 is
`ba5683f9265b24c03fbe918aad332e6f350f9675cc5602ccb10f0b4e389a203b`.
The build reused the local Cargo target; it is not an independent rebuild.

The selected Safe `r2arm_*` comparison manifest SHA-256 is
`d48f7f17d4041b1d6e6e03ff8f80b425eb0d040db5b1da5a952d7127d090318b`.
It records source hashes and both binary hashes. Against the preceding
source-bound `1280e21e` CLI on the same registered source bytes, it observed
56 IDs, 69 forms, and no typed verdict flips: each binary produced 34 PASS,
33 DISPROVED, and two UNDECIDED results. The UNDECIDED forms are the valid
and invalid failed-guard write fallthroughs; the valid one remains a precision
defect. This comparison covers only the selected prefix. The full matrix
runner remains `INCOMPLETE`, and older evidence sidecars are not silently
reused against this changed producer.

No full workspace, disposable-guest, hosted, independent clean-room, Apple,
or release gate is claimed for this code commit. The honest FAIL-bundle
verifier and source-closure admission remain separate open dependencies.
