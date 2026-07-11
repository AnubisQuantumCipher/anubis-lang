# LIFELINE Critical-Infrastructure Cascade and Recovery Optimizer

LIFELINE is a safe-mode, single-file Anubis decision-support kernel for compound-disaster recovery planning. It models ten interdependent civil assets across power, water, healthcare, communications, fuel, shelter, wastewater, cooling, and medicine cold-chain services.

The program is intentionally offline and deterministic. It does not connect to live infrastructure, predict casualties, or issue operational commands.

## What the program computes

LIFELINE validates its model before indexed access, simulates cascading failures to a bounded fixed point, and scores service loss under four weighted scenarios. It then enumerates every one of the `2^10 = 1,024` possible recovery portfolios.

A portfolio is feasible only when it satisfies all of these policies:

- budget at or below 66 planning units;
- crew demand at or below 104 hours;
- fuel demand at or below 48 units;
- every action prerequisite selected;
- at least two actions directed at assets whose vulnerability score is `>= 9`.

Feasible portfolios are compared lexicographically by hard service-floor violations, robust objective, cost, action count, and mask. The robust objective combines worst-case loss, probability-weighted expected loss, equity-weighted loss, and a large penalty for breaching minimum service floors.

After choosing a plan, the program enumerates the entire search space in reverse order using the same evaluator and comparator. This is a traversal-consistency check, not an independent implementation. It also recomputes the chosen plan twice and rejects an invalid dependency reference, a shortened bit-power table, an overflow-scale population, and a deliberately over-budget all-actions mask. An external red-team oracle separately reimplemented the model and confirmed the optimum.

The final rolling checksum is deterministic but deliberately labeled non-cryptographic. The application gate signs the Anubis evidence bundle with a one-time Ed25519 key and requires that exact public key during verification. A real deployment would pin an institutional public key outside the bundle.

## Live result from the 2026-07-11 run

The current embedded model produced:

- `1,024` candidate portfolios;
- `134` feasible portfolios;
- selected mask `611` with five actions;
- cost `64`, crew `88`, fuel `35`;
- worst-case modeled loss reduced from `2,977,016` to `2,174,930`;
- expected modeled loss reduced from `2,698,935` to `1,509,924`;
- hard service-floor violations reduced from `22` to `6`;
- same-evaluator reverse traversal check `1`;
- invalid-model negative control rejected `1`;
- shortened powers-table negative control rejected `1`;
- overflow-scale population negative control rejected `1`;
- over-budget tamper negative control rejected `1`;
- verdict `PLAN_CERTIFIED` and exit code `0`.

Those numbers describe this embedded model and objective only. They are not forecasts of real-world human outcomes.

## Run and verify

```bash
cd /Users/sicarii/anubis-lang
cargo build --release

./target/release/anubis check \
  examples/industry/lifeline_resilience_optimizer.anb \
  --emit ast,hir,mir \
  --evidence \
  --out out/lifeline_resilience/check

./target/release/anubis run \
  examples/industry/lifeline_resilience_optimizer.anb \
  --evidence \
  --json \
  --out out/lifeline_resilience/run

rg '^(SEARCH|PLAN |ROBUST|EQUITY|AUDIT|NEGATIVE|VERDICT|SUMMARY)' \
  out/lifeline_resilience/run/stdout.txt
```

Verify the timestamped check evidence bundle with:

```bash
./target/release/anubis verify out/lifeline_resilience/check/evidence-<STAMP>-safe
```

## Honest capability boundary

What is real here:

- ordinary safe execution through Anubis's Rust-transpiled runner;
- solver-checked contracts on four bounded integer helper functions;
- a monotone bounded cascade simulation;
- exact optimization over the declared ten-action set;
- deterministic stdout and emitted Rust source across replay;
- a same-evaluator reverse traversal check plus external independent oracle confirmation;
- an Ed25519-signed Anubis evidence bundle verified against the gate's expected public key.

What is not claimed:

- a proof of the whole simulator or optimizer;
- a RISC0 proof or verified receipt for this run;
- optimality outside the declared action set and objective;
- calibrated hazard probabilities or validated human-impact coefficients;
- live data ingestion, databases, networking, asynchronous work, or a user interface;
- reproducible native Mach-O bytes; the gate compares semantic stdout and emitted Rust source, not platform UUID/signature bytes;
- production readiness or authorization for emergency deployment.

The correct production architecture would keep this deterministic kernel behind a conventional host application with authenticated data, schema validation, calibration, human approval, monitoring, and independent safety review.
