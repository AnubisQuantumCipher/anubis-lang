# Industry programs (Anubis)

## ENNEAD — Byzantine-Fault-Tolerant Consensus Kernel

[`ennead_consensus_kernel.anb`](ennead_consensus_kernel.anb) is a deterministic,
offline State-Machine-Replication kernel — the agreement core of every blockchain,
replicated database, and fault-tolerant control plane — written to exercise **both
halves of Anubis at once**: a live runtime *and* a machine-checked safety proof.

- **`anubis check`** asks Z3 to **prove** the six integer-safety theorems that make
  BFT sound, including the **quorum-intersection lemma** (any two quorums of `2f+1`
  drawn from `n = 3f+1` overlap in `≥ f+1` replicas → at least one honest witness →
  no split-brain). A broken contract is *rejected*, not tested — verified with a
  negative control (claiming `overlap ≥ f+2` is disproved with a counterexample).
- **`anubis run`** boots a council of replicas through five scenarios: the happy
  path, a crashed leader (view change), an **equivocation attack** (Set on the
  throne — double-proposing + double-voting, caught and rejected), a beyond-budget
  case that **safely stalls** instead of splitting, and a 7-seat tribunal under
  compound Byzantine + crash faults. It then proves all honest logs are identical
  (Agreement) and hash-chains the committed history.
- **`anubis build --evidence`** seals source + HIR/MIR + the actual Z3 SMT-LIB
  queries + native artifact into a verifiable Proof-Carrying Artifact.

```bash
cd /Users/sicarii/anubis-lang
cargo build --release -p anubis

./target/release/anubis run   examples/industry/ennead_consensus_kernel.anb
./target/release/anubis check examples/industry/ennead_consensus_kernel.anb
./target/release/anubis build examples/industry/ennead_consensus_kernel.anb \
  --evidence --out out/ennead_evidence
```

Success signals: `run` ends `SEAL#496053801  verdict=PASS` with
`all-agreement=true   all-expected=true`; `check` prints `check passed`; `verify`
on the bundle prints `bundle valid: true`. Language surface: enums (protocol ISA +
fault model + verdicts), exhaustive `match`, traits with default methods, generics,
closures with `map`/`filter`/`all`/`count`, methods, `Option`, maps, string
interpolation — plus six SMT-verified contracts with a loop invariant.

## AETHER — Agent Governance & Release-Authority Kernel

[`aether_agent_governance.anb`](aether_agent_governance.anb) is a deterministic,
offline **policy kernel for autonomous agents** — the decision-and-evidence layer
that should sit in front of any coding agent, swarm worker, or tool-calling LLM
that can touch production deploys, key material, fund movements, external
disclosure, or the shell. It does **not** execute tools; it adjudicates and seals.

- **`anubis check`** proves two integer-safety contracts with Z3 — a severity
  clamp (`requires(x < 1_000_000) ensures(result >= x)`) and a trust-floor
  derivation — so the risk arithmetic that gates irreversible actions cannot
  silently under-report.
- **`anubis run`** classifies each tool-call by risk class and irreversibility,
  propagates taint from untrusted origins (web fetch, user paste, poisoned tool
  output), enforces dual-control on irreversible actions, quarantines agents that
  climb a compromise kill-chain, and hash-chains every decision into a
  tamper-evident receipt ledger. It closes with a **same-evaluator reverse-cover
  audit** and **four negative controls** before emitting a verdict.
- **`anubis build --evidence`** seals the source, HIR/MIR, SMT queries, and native
  artifact into a verifiable Proof-Carrying Artifact.

Success signals: `run` ends `VERDICT: POLICY_KERNEL_CERTIFIED` with
`SUMMARY: allow=3 witness=2 deny=3 quarantine=7 … seals=6/6`; `check` prints
`check passed`. Honest boundary: ordinary safe execution plus two SMT contracts —
**not** a RISC0 proof of the full simulator, and not production authorization.
Language surface: enums with payloads, exhaustive `match`, taint propagation,
maps, closures, methods, string interpolation.

## LIFELINE Critical-Infrastructure Recovery Optimizer

[`lifeline_resilience_optimizer.anb`](lifeline_resilience_optimizer.anb) is an offline disaster-recovery decision-support kernel. It simulates interdependent civil-infrastructure cascades, exactly enumerates 1,024 bounded recovery portfolios, enforces resource and equity constraints, runs a same-evaluator reverse traversal, and emits a deterministic certificate. A separate hostile oracle confirmed the optimum. See [`LIFELINE_RESILIENCE_OPTIMIZER.md`](LIFELINE_RESILIENCE_OPTIMIZER.md) for the verified run and honesty boundary.

## Sovereign General Ledger + Risk Engine (`sovereign_gl_risk_engine.anb`)

**Domain:** fintech / banking / payments core control plane  
**Currency model:** integer **USD cents** (no floating-point money)

### What it does

1. Seeds a chart of accounts + opening balances (GAAP-ish classes)
2. Posts a full double-entry journal day (debits must equal credits)
3. Builds a trial balance and proves **Assets = Liabilities + Equity + Revenue − Expense**
4. Runs a bank reconciliation (deposits in transit, outstanding checks)
5. Multilateral payment **netting** across counterparties (gross vs net funding)
6. Single-name **concentration risk** in basis points vs a policy limit
7. Hash-chained **audit checksum** over journal fingerprints + control outcomes
8. Seals a **period-close verdict** (`PERIOD_CLOSED_BALANCED` or control fail)

### Run

```bash
cd /Users/sicarii/anubis-lang
cargo build --release -p anubis

./target/release/anubis run examples/industry/sovereign_gl_risk_engine.anb \
  --evidence --out out/industry_gl
```

Success signal:

- last line `0` (verdict code Balanced)
- `SUMMARY ok=1 residual=0`
- `close.verdict=PERIOD_CLOSED_BALANCED`

### Why this is “industry needed”

Every regulated money-moving org needs:

| Control | Where in the program |
|--------|----------------------|
| Double-entry integrity | `validate_journal_shape` + `post_journal` |
| Period books balance | `equation_residual` / trial balance |
| Cash vs bank | `bank_recon` |
| Treasury liquidity efficiency | netting `efficiency_bps` |
| Credit concentration | `concentration_bps` vs `limit_bps` |
| Audit trail seal | `chain_hash` + close verdict |

This is a **computational core** (policy + math + seal), not a database or UI product shell.
You would wrap it with storage, auth, and a report pipeline in production.

## NEXUS — Networked EXecution Under Sealed policy

[`nexus_execution_kernel.anb`](nexus_execution_kernel.anb) is the flagship **action kernel** for agents with hands: intent → taint/effect → policy gate → hash-chained public journal + private witness store → freeze-on-Abort, with dual-use exploit gates and declassify rules. See also [`NEXUS.md`](NEXUS.md).

Companions:

- [`nexus_zk_decision.anb`](nexus_zk_decision.anb) — minimal public decision-function journal (6/6 battery)
- [`nexus_zk_decision_proof.anb`](nexus_zk_decision_proof.anb) — small prove-shaped disclose decision

### Layers

| Layer | What it does |
|-------|----------------|
| L0 Intent | First-class `Intent` values (kind, path class, host, taint, declass, epoch) |
| L1 Taint/effect | Credential/PII gates + path/host scope + offensive lab boundary |
| L2 SMT helpers | `fuel_tick`, `clamp_score`, `policy_id`, `advance_receipt` |
| L3 Journal | Public hash-chained rows; raw targets stay private |
| L4 Selective disclosure | Public digests vs offline private witness store |
| L5 Hybrid meta | Honest `executed=0` plan markers (no fake GPU claims) |
| L6 Kill / freeze | Charter kill epoch; Abort freezes all agents |

### Run

```bash
cd /Users/sicarii/anubis-lang
./target/release/anubis run   examples/industry/nexus_execution_kernel.anb
./target/release/anubis check examples/industry/nexus_execution_kernel.anb
./target/release/anubis run   examples/industry/nexus_zk_decision.anb
./target/release/anubis run examples/industry/nexus_execution_kernel.anb \
  --evidence --out out/nexus_kernel
```

Success signals:

- `VERDICT: NEXUS_KERNEL_CERTIFIED`
- `seal_score 10/10`
- negative controls `passed 6 failed 0`
- expected-outcome battery `hits == checks`
- `check passed`

### Distinct from AETHER

AETHER is agent capability dual-control governance. NEXUS adds **engagement kill clocks**, **path-class scopes**, **target digests**, **explicit declassify policies**, **fuel budgets**, **public/private journal split**, **exploit-module dual-use gates**, and an **expected-outcome scenario battery** with freeze cascade.

### Honesty boundary

Offline deterministic kernel — **not** live tool mediation, not institutional deployment authority, not a full-engine formal proof of every branch. SMT contracts cover the integer helpers; stream policy is runtime-sealed with negative controls.
