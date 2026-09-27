# Min/max callback feedback — candidate withheld, 2026-09-26

Status: **not integrated; required precision regression fails**. This receipt
does not close the round-2 findings `r2p_06_min_by_max_by_callback_args_decided_by_results`
or `r2_fn_L1_min_by_max_by_callback_argument`.

## Baseline and concrete boundary

Baseline compiler:
`/home/sicarii/.cache/anubis-item21/pins/anubis-ifc2land-3`, SHA-256
`0504173ba4771dfb6ce9aa0c4365088bd86fabdf1f9875d83065e39207826df6`.
The earlier landing receipt attributes this pin to `e516b1f3`; its bytes were
rehashed here. A full reproduction of that old build's provenance is not claimed.
Current compiler source at continuation base
`f7e906d1a31533e485aaeeac3ca1c2d1d0877c01` has the same missing callback feedback.

`baseline-runtime.json` records the full commands, exit statuses and outputs.
The original finding checks successfully and prints `1,2,2,3` for synthetic
secret `42`, but `1,2,1,3` for synthetic secret `142`. Direct secret printing
is rejected with `ANUBIS_SECRET_EXFILTRATION`. These were ordinary finite native
programs on the Linux QEMU guest, under a memory cap; no research, crash, fuzz,
exploit or disposable-guest claim applies to these runs.

The runtime comparator invokes callbacks on the incumbent and the next element.
Earlier keys decide which incumbent reaches later calls. The baseline abstract
interpreter checks the callback on original elements and labels only the final
winner, missing disclosures made by those later callback arguments.

## Candidate and independent review

`candidate.diff`, `ifc2_minmax.rs`, `cases/` and `candidate-manifest.json`
preserve the unintegrated candidate and source/instrument hashes. It models
known sequences in order and unknown sequences with monotone selection-label
feedback. Collection shape controls callback scheduling; key labels affect the
argument, preserving constant logging with secret keys.

Independent final-diff review found a valid program newly refused:

```anubis
fn main() {
    let s: secret<i64> = 42;
    let m = min_by([1, 2, 3], |x| { println(x); s });
}
```

Every key is the same captured value, so comparisons tie. The callback's public
argument transcript does not disclose it. `equal-key-baseline-runtime.json`
records baseline acceptance and equal transcripts for the supplied secret pair,
for both min and max. This bounded execution supports the source argument; it
is not a general noninterference theorem.

The original extra negative tests accidentally used constant keys too. Their
sources were corrected to make the key depend on the argument. The constant-key
case was added as a required **passing** test and ACCEPT matrix fixture.
No expectation was changed to bless its refusal.

## Validation and next executable step

- Baseline cargo integration test: **1 passed, 2 failed**, recorded in
  `baseline-tests.txt`. The failures demonstrate the missing feedback and IFC2's
  unnecessary singleton callback analysis.
- Current tests against the candidate compiler library: **5 passed, 1 failed**,
  exit **101**, in `review-regressions.txt`. The failing test is the required
  equal-key precision control. These tests compile valid finite inputs and do
  not run crash/resource-exhaustion probes.
- Candidate tests include full Safe-check rejection for the original mechanism,
  IFC2-only alias/compound/nested cases, and public/declassified/constant-output,
  collection-shape, singleton and pair controls. IFC2-only acceptance does not
  establish acceptance by every other checker lane.
- The library was built with `cargo test --locked --release -j 2 -p anubis-compiler
  --test ifc2_minmax`, using the pinned nightly. After review, the final test
  source was compiled directly against that library while the separate CLI build
  was compiling its dependencies. Exact command and hashes are in the manifest.
- No final full-workspace, full-matrix, corpus or mandatory guest result exists
  for this candidate. Do not publish a verified seal or integrate this patch.

Next: independently review [REPEATABLE_KEY_DESIGN.md](REPEATABLE_KEY_DESIGN.md),
then implement sound key repeatability/equality that preserves captures, call
resolution, mutations and external observations. Abstract label equality alone
cannot establish equality of runtime keys. Keep the valid control and rerun the
negative witnesses and full applicable gates after the semantic change.
