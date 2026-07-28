# Anubis documentation

**You do not need to read the audit to use the language.** This project keeps an unusually large
evidence trail on purpose — but that trail is written for auditors, not for someone trying to write
their first program. Pick the path that matches what you are here to do, and ignore the rest.

Sizes are given so you can budget. Nothing on the **Use it** path is longer than a coffee break.

---

## Pick your path

### 🚀 I want to use Anubis · *~40 min end to end*

Read these four, in order. Stop whenever you can do what you came to do.

| | Doc | Size |
|---|---|---|
| 1 | [Install](INSTALL.md) — build the binary, verify it runs | 2K |
| 2 | [Tutorial](language/TUTORIAL.md) — hello, then secrets, then contracts | 8K |
| 3 | [Language reference](../LANGUAGE.md) — the syntax and semantics you actually write | 28K |
| 4 | [CLI reference](CLI.md) — every subcommand and flag | 14K |

Then reach for these **when you need them**, not before:

- [Builtins](language/BUILTINS.md) (9K) — the complete callable inventory
- [Information flow](language/INFORMATION_FLOW.md) (11K) — how `secret<T>` / `tainted<T>` work, and how to *construct* a secret
- [Packages](language/PACKAGES.md) (5K) — `Anubis.toml`, lockfiles, proof-carrying dependencies
- [Crypto](language/CRYPTO.md) (8K) — the vetted primitives and the permanent non-claims
- [Examples](EXAMPLES.md) — programs you can run right now, including two full applications

### 🔍 I am evaluating whether to trust this · *~30 min*

| | Doc | Size | Why |
|---|---|---|---|
| 1 | [Capabilities](CAPABILITIES.md) | — | what exists, what is partial, what is currently **open** — with the status vocabulary explained |
| 2 | [Claims](CLAIMS.md) § *Known open issues* | 89K | **the single source of truth for current status.** Read the first ~130 lines; the rest is per-item forensics you can skip |
| 3 | [Anubis vs SPARK](SPARK_VS_ANUBIS.md) | 7K | an honest, re-runnable comparison against an established verifier |
| 4 | [1.0 freeze](language/SPEC_1_0_FREEZE.md) | 7K | exactly which commands and syntax are frozen, and the explicit residuals |

**The one thing to understand before anything else:** a green gate here means *an empty published
residual inventory* — no defect in the list that is currently published. It does not mean no
defects. The project says this about itself, repeatedly, and means it.

### 🔬 I am auditing this / hunting a false accept · *hours*

Start with the evaluator path, then:

| Doc | Size | What it is |
|---|---|---|
| [Claims](CLAIMS.md) | 89K | the live residual — read it **in full**, it is the authority |
| [Roadmap](language/ROADMAP.md) | 183K | the 11-phase arc; the living **STATUS** layer is at the top, the rest is dated narrative |
| [Maturity claim matrix](../MATURITY_CLAIM_MATRIX.md) | 82K | the append-only ledger of every tranche since 2026-07-05. **Historical by construction** — see its own header before reading |
| [Unsupported](language/UNSUPPORTED.md) | 47K | the per-feature non-claims record |
| [Trust boundaries](TRUST_BOUNDARIES.md) | 1K | what the toolchain trusts and what is explicitly not claimed |
| [Reproducibility](REPRODUCIBILITY.md) | 1K | which commands reproduce which results |
| [Self-host](language/SELFHOST.md) | 16K | stage chain, fixpoint definition, trusting-trust defense |
| [Independent witnesses](language/phase9_independent_witness/) | — | third-party clean-clone reproductions |
| [Harness integrity audit](HARNESS_INTEGRITY_AUDIT_2026-07-28.md) | 14K | can a gate print PASS while testing nothing? (answer: some could) |

Found a case where a green `anubis check` certifies something `anubis run` violates? That is the bug
class that matters most here — report it privately per [`SECURITY.md`](../SECURITY.md).

### 🛠️ I am working on the compiler

| Doc | What it is |
|---|---|
| [CONTRIBUTING](../CONTRIBUTING.md) | the one invariant, the single green-or-not command, what a good change looks like |
| [AGENTS](../AGENTS.md) | the standing briefing — rules, verification bar, and the gotchas that cost hours |
| [Architecture map](../ARCHITECTURE_MAP.md) | crate layout and the call flow from CLI to lowering *(dated 2026-07-11 — verify before relying on its counts)* |
| [Pipeline maps](.) | [language core](LANGUAGE_CORE_PIPELINE_MAP.md) · [solver](SOLVER_PIPELINE_MAP.md) · [RISC0](RISC0_PIPELINE_MAP.md) · [Metal](METAL_BACKEND_PIPELINE_MAP.md) — all dated snapshots |
| [Completion blueprint](COMPLETION_BLUEPRINT.md) | the standing phased plan to close the false-accept class |
| [Grammar](language/GRAMMAR.md) · [Spec](language/SPEC.md) | both subordinate to [`LANGUAGE.md`](../LANGUAGE.md), which wins on conflict |

### ⚔️ I am doing authorized security research

[Offensive platform](language/OFFENSIVE_PLATFORM.md) (11K) · [PoC kit](language/POC_KIT.md) (5K) ·
[Vulnerability taxonomy](security/VULNERABILITY_TAXONOMY.md) · [Research profile](language/SECURITY_RESEARCH_PROFILE.md)

VZ isolation is **mandatory** for this work, and the isolation marker in a receipt is host-written —
read the boundary box in [Capabilities § Confine](CAPABILITIES.md) before you rely on it.

---

## How this documentation is layered

| Tier | What it answers | Where |
|---|---|---|
| **0 — front door** | "what is this and should I care" | [`README.md`](../README.md), ~150 lines |
| **1 — map** | "which document do I want" | this file |
| **2 — reference** | "how do I do X" | [Capabilities](CAPABILITIES.md) · [Examples](EXAMPLES.md) · [Tutorial](language/TUTORIAL.md) · [CLI](CLI.md) · [`LANGUAGE.md`](../LANGUAGE.md) |
| **3 — evidence** | "prove it" | [Claims](CLAIMS.md) · [Roadmap](language/ROADMAP.md) · [Matrix](../MATURITY_CLAIM_MATRIX.md) · [Unsupported](language/UNSUPPORTED.md) |
| **archive** | "what did this used to say" | [`docs/history/`](history/) — dated, superseded, never current |

Each tier is meant to be a complete answer at its own level. If you find yourself in tier 3 to
answer a tier-1 question, that is a bug in this map — please say so.

---

## Rules this documentation follows

These are not aspirations; they are enforced by
[`scripts/run_docs_drift_gate.sh`](../scripts/run_docs_drift_gate.sh) as gate **G16**:

1. **Live quantities are re-derived by command, not typed by hand.** Fixture counts, builtin counts,
   and Lean theorem/module counts in the owned docs are re-measured from the tree on every gate run;
   a stale number fails the build.
2. **Coverage can only go up.** The number of stamps the scanner checks is ratcheted in
   [`docs/.docs_drift_coverage_floor`](.docs_drift_coverage_floor). Lowering it requires editing that
   file in a visible commit — because an exemption is the one edit that makes a gate greener by
   making it check less.
3. **Named absolute claims are rejected.** A published banlist of unfalsifiable phrasings ("total",
   "closed forever", "no defects") fails the gate unless the surrounding text scopes or negates them.
4. **A dated seal is not a current claim.** Anything written as "as of *date*" or under a
   content-addressed binary pin describes that artifact, not today's tree.

If you are adding a doc that carries a live number, add it to `LIVE_FILES` in
[`scripts/lib/docs_drift_scan.py`](../scripts/lib/docs_drift_scan.py) so the number is checked. A
doc that is not in that list is not protected from drift.

---

## Full index

**Language & usage** —
[LANGUAGE](../LANGUAGE.md) ·
[Tutorial](language/TUTORIAL.md) ·
[Spec](language/SPEC.md) ·
[Grammar](language/GRAMMAR.md) ·
[Builtins](language/BUILTINS.md) ·
[Stdlib core](language/STDLIB_CORE.md) ·
[Core features](language/CORE_FEATURES.md) ·
[Turing completeness](language/TURING_COMPLETENESS.md) ·
[Information flow](language/INFORMATION_FLOW.md) ·
[Packages](language/PACKAGES.md) ·
[Crypto](language/CRYPTO.md) ·
[RWC map](language/RWC_LANGUAGE_MAP.md) ·
[Install](INSTALL.md) ·
[CLI](CLI.md)

**Status & evaluation** —
[Capabilities](CAPABILITIES.md) ·
[Examples](EXAMPLES.md) ·
[Claims](CLAIMS.md) ·
[Roadmap](language/ROADMAP.md) ·
[1.0 freeze](language/SPEC_1_0_FREEZE.md) ·
[SemVer policy](language/SEMVER_1_0_POLICY.md) ·
[Language completeness](language/LANGUAGE_COMPLETENESS.md) ·
[Unsupported](language/UNSUPPORTED.md) ·
[Proof scaling](language/PROOF_SCALING.md) ·
[SPARK comparison](SPARK_VS_ANUBIS.md)

**Platform & backends** —
[Apple native](APPLE_NATIVE.md) ·
[Metal backend](METAL_BACKEND.md) ·
[RISC0 backend](RISC0_BACKEND.md) ·
[RISC0 input ABI](proof/RISC0_PARAMETERIZED_INPUT_ABI.md) ·
[RISC0 proof status](proof/RISC0_PARAMETERIZED_PROOFS_STATUS.md) ·
[Metal hybrid reference](RISC0_METAL_HYBRID_REFERENCE.md)

**Security research** —
[Offensive platform](language/OFFENSIVE_PLATFORM.md) ·
[PoC kit](language/POC_KIT.md) ·
[Research profile](language/SECURITY_RESEARCH_PROFILE.md) ·
[Vulnerability taxonomy](security/VULNERABILITY_TAXONOMY.md)

**Audit & assurance** —
[Trust boundaries](TRUST_BOUNDARIES.md) ·
[Reproducibility](REPRODUCIBILITY.md) ·
[Self-host](language/SELFHOST.md) ·
[Independent witnesses](language/phase9_independent_witness/) ·
[Harness integrity audit](HARNESS_INTEGRITY_AUDIT_2026-07-28.md) ·
[Portability audit](PORTABILITY_AUDIT.md) ·
[Maturity matrix](../MATURITY_CLAIM_MATRIX.md)

**Internal / contributor** —
[Architecture map](../ARCHITECTURE_MAP.md) ·
[Language core pipeline](LANGUAGE_CORE_PIPELINE_MAP.md) ·
[Solver pipeline](SOLVER_PIPELINE_MAP.md) ·
[RISC0 pipeline](RISC0_PIPELINE_MAP.md) ·
[Metal pipeline](METAL_BACKEND_PIPELINE_MAP.md) ·
[Completion blueprint](COMPLETION_BLUEPRINT.md) ·
[Completion ledger](COMPLETION_LEDGER_2026-07-28.md) ·
[Handoff](HANDOFF.md) ·
[Type system phase](language/TYPESYSTEM_PHASE.md) ·
[Repo hygiene](repo-hygiene.md) ·
[ADRs](adr/)

**Archive** — [`docs/history/`](history/): superseded plans, closed audits, and dated seal reports.
Nothing in it is a current claim.
