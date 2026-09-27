# IFC v2 landing evidence (2026-09-26)

What landed in e516b1f3: IFC v2 ([design](../../mission/IFC_V2.md)), running beside the other
lanes in every Safe-mode check (decision D8), with the fixes for its first independent review,
and the core runtime's trap messages without operand values (decision D7).

## Pins

| pin | sha256 | what |
|---|---|---|
| `anubis-ifc2land-3` | 0504173ba4771dfb6ce9aa0c4365088bd86fabdf1f9875d83065e39207826df6 | the build of e516b1f3 (pin == build) |
| `anubis-ifc2-v8f` | 6ea5f815138b1ea30580359c9f4d3972d3eb3b9394680501272e9c55dc4aeaf9 | the development build the IFC-v2-alone comparison was measured on (later changes in e516b1f3: a field of a struct or enum declared twice takes both types; constant conditions; an operand with no value; literal counts in `take`/`drop`/`chunk`/`window`; summary contexts keyed by the functions their arguments hold; the stack guard) |
| `anubis-ord3x4l` | 74b7c80091c948788e0310d54b2409bfdccf4e5596bf502513a04f69dd7a723b | the lanes at d052f817 (the comparison's baseline) |

Every run was memory-capped (`systemd-run --user --scope -p MemoryMax=…`) on the Omarchy aarch64
QEMU guest.

## Files

- `verify.txt`: the full verification of `anubis-ifc2land-3` (workspace tests, solver tests,
  matrix totals, corpus, gates); `tests.txt` the per-binary test results, `corpus.tsv` the corpus
  verdicts (path, rc, certified, discharged, trusted).
- `review1-findings.tsv`: the 107 confirmed findings of IFC v2's first independent review (five
  lenses and an independent verifier, each finding re-run with two secret values on a runtime
  runner), with the matrix case each is registered as, whether the lanes also accepted it, and the
  root cause the reviewers localized.
- `suite.txt`: the 107 review programs through IFC v2 alone (`anubis ifc2-report`) on the landed
  pin, graded against each finding's intent.
- `ifc2-alone-vs-lanes.tsv` and `.summary.txt`: every matrix case (2206, before the round-1 cases
  were registered) with IFC v2 alone against the lanes' recorded verdicts (`cmp_ifc2.py`); the
  IFC v2 table is `run.sh` driven through a wrapper that maps `check <file>` to
  `ifc2-report <file>` with a 120 s timeout.
- `shadowfn.anb`: the witness of RT-LAMBDA-PARAM-SHADOWS-FN-VALUE (`check` rc 0; `run` fails to
  compile natively with E0425).

## Results

- Workspace tests 1461 passed, 0 failed, 0 crashed test binaries; solver tests 123/123.
- Matrix, the landed tree: 2319 cases, 2196 PASS, 0 silent accepts, 112 wrong-class, 11 INVALID
  (8548248c: 58 silent accepts).
- Corpus (968 programs) with IFC v2 beside the lanes: 0 verdict changes against the recorded
  baseline; certificates 955 certified, 1421 discharged, unchanged.
- Gates: nexus ALL PASS, package PASS, security fixtures 356/356, language fixtures 271/271, docs
  drift 38 stamps 0 drift, walker completeness PASS, label census PASS, G27 10/10.
- Round-1 suite on the landed pin: 107/107 as intended.
- IFC v2 alone (v8f): rejects all 58 registered leaks the lanes accept; accepts 105 valid programs
  the lanes refuse; refuses no valid program the lanes accept; no `ANUBIS_IFC2_LIMIT`. The 117
  registered rejections it does not make are contract (DISPROVED, MIXED, WRAP, UNDECIDED) and
  capability cases, which are not information flow.
