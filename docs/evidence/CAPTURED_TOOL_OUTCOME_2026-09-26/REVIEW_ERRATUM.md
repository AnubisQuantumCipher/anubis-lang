# Captured source digest transcription erratum

The original [independent code review](INDEPENDENT_REVIEW.md) printed a
65-character SHA-256 string for `compiler/src/resolve/captured.rs` and stated
that it matched the reviewed source. The string has an extra `9` after
`ecfb719`. That printed match assertion is invalid. The report is retained
unchanged so its provenance and other bounded findings remain visible.

The committed file at `2683669c2ae87ceea66d19ebb0e6995578c289eb` has
SHA-256 `ea5a6795eb25eb948bdfb59bf1d412c28dbecfb719b3aaad8311199b3786ace6`.
The raw source manifests retained for the code unit contain this digest. The
commit's two-file binary diff from `05c5377f6aff2e3bde0b659094fe73ab9a8067d8`
has SHA-256 `b241ca11b51c78d9f8d765411110a288599ef305a618a086532a75f77f3b8433`,
matching the reviewed patch. The middle-source SHA-256 is
`b33ce29df925fd251585568354d14844a0c9ef83102f4e0c51de48c1b08c4ee0`.
These are rehashes of committed Git objects, not an inference from an
abbreviated commit or a source-matched binary claim. The corrected digest is
in [gate-receipt.json](gate-receipt.json); the original review's printed digest
must not be used for source verification.

The lead's first clean-build wrapper also stopped before Cargo because its
expected digest copied the same extra digit. That failed precondition run was
preserved locally; the corrected wrapper built the committed HEAD and compared
its binary hash to the immutable tested snapshot. Neither the original review
text nor the failed wrapper has been rewritten into a passing record.
