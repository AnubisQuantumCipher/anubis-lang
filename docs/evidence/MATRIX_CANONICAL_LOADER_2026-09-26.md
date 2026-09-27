# Fixed-path matrix inventory loader — local receipt, 2026-09-26

Code commit: `b3ddf665f2f4f75988360395493e5f31f2bf8d11`.
The runner now accepts a canonical expectation inventory only at its fixed
in-repository `tests/soundness/matrix/canonical/manifest.v1.json` path. It
requires the exact registered form set, each source hash, intent and category,
the runner and provenance TSV hashes, and typed diagnostic tuples with source
spans or obligation fingerprints. It hashes the same bounded manifest bytes it
parses and compares that identity again after checking. A caller-supplied
expectation file cannot replace a valid canonical inventory. The established
`ANUBIS_SECRET_TO_PUBLIC` frontend refusal is classified explicitly, including
when it appears with an interprocedural exfiltration finding.

The fixed-path loader and its fake-checker tests received independent static
review after two corrections: legitimate D9 diagnostic classification and the
parse-versus-digest replacement window. The final patch SHA-256 is
`859bb37e180d7d51d8c2c88fd6e7bf26c7262e64f65a079c5847951216be4fe8`.
On the integrated tree, `python3 tests/soundness/matrix/test_run.py` passed
(`Ran 48 tests`, `OK`); its retained local log SHA-256 is
`731f6985d4974a7a557bda3375a68c0e70e8d2502fc48f364be4e6361cf35c27`.
`bash -n` and Python AST parsing passed. The canonical documentation-drift
gate, `bash scripts/run_docs_drift_gate.sh`, passed with zero drift; its retained
local log SHA-256 is
`9e711c1ff280e63cdf28e70117f52405331f385eccf226c5ce394a01ef2bd7c9`
(rerun after this documentation edit).
The first attempted drift command named a nonexistent script and exited before
running a gate; the named canonical gate above was then run and passed.

This code does **not** check in the live canonical manifest. The compiler's
current security findings often lack stable producer locators, and a complete
source-to-binary build identity is not connected to this runner. It therefore
still reports `INCOMPLETE`, exits with code 2 and never publishes history via
`--record`, even if every provisional row matches. Manifest property labels
are reviewed assertions, not semantics derived from source. The fake-checker
suite ran no Anubis program or Research/OOM case. No full matrix, product
assurance, independent clean-room or release result is claimed.
