# Examples — programs you can run right now

**Tier 2 · reference.** Every program below runs on the prebuilt binary with
`anubis check <file>` (or as noted). Start with the [tutorial](language/TUTORIAL.md) if you want to
be *taught*; start here if you want to *see it work*.

The *reject* demos each ship a matching *accept* guard, so you can see the checker is **precise, not
trigger-happy** — the same program with the leak removed passes.

---

## Start here — two full applications

These are the flagship programs. Both are real applications, not diagrams.

### ⭐ NEXUS — a secure AI agent the compiler keeps honest

**[`examples/showcase/nexus/`](../examples/showcase/nexus/)** — an autonomous AI agent whose safety
is proved by the compiler rather than promised by a system prompt. Every AI-agent security failure
of the moment is, in NEXUS, either a **compile error** or a **machine-checked proof**:

| The agent must… | …and Anubis makes it a property, not a promise |
|---|---|
| **keep private beliefs secret** | its beliefs are `secret<T>`; the checker proves no secret ever reaches a public sink — **a leak is a compile error**, not a runtime incident |
| **not be hijacked by untrusted input** | every sensor and command is a `taint_source` that **must** be validated and `declassify`-ed with an auditable *policy + reason* before the agent may act on it — unsanitized influence is rejected |
| **not over-act** | outbound action is a **linear, use-once capability** — `cap_acquire("net.send")` then `cap_use`: it earns the right to broadcast exactly once and spends it exactly once |
| **never combine all three** | reads-private **+** untrusted-input **+** can-exfiltrate — *the lethal trifecta*, the canonical agent-exfiltration bug — is `ANUBIS_LETHAL_TRIFECTA`; NEXUS passes only because it routes every flow through validation, declassify, and a spent capability |
| **be auditable without exposing its thoughts** | it emits a **hash-committed record of its own cognitive integrity** — evidence it reasoned inside its safety envelope — while the beliefs themselves (`secret<T>`) never leave |

It is **475 lines** that `check` clean **and** `run` — SMT-discharged contracts, `trait` dispatch,
`enum` kinds, generics, the higher-order builtins, and a `while … invariant(...)` integrity chain.
Re-derive the contract inventory rather than taking a number on trust:

```bash
grep -c 'requires\|ensures' examples/showcase/nexus/nexus_cognitive_kernel.anb
```

```bash
anubis check examples/showcase/nexus/nexus_cognitive_kernel.anb   # → check passed
anubis run   examples/showcase/nexus/nexus_cognitive_kernel.anb   # → runs; proves its own integrity
```

```
[Phase 3] Private belief formation: secret<T>, contract-verified
  beliefs formed and fused (secret<i64> — checker enforces no leak)
  Z3 proved: 0 <= fused < 10000, 0 <= surprise < 10000
...
The kernel proved its own cognitive integrity. It revealed NOTHING about what it deliberated.
```

The information-flow half —
[`nexus_checker_security.anb`](../examples/showcase/nexus/nexus_checker_security.anb) — is where the
`taint_source → declassify` discipline and the capability-gated broadcast are proved in the checker
lane (`anubis check --verified` → passed).

> **Scope of the evidence.** NEXUS's checked source exercises private-state, untrusted-input,
> capability-egress, and integrity controls; `check` and `run` are the evidence **for this program**,
> not a total-language proof.

### ⭐ Anubis Vault — a high-threat password manager

**[`examples/showcase/anubis_vault/`](../examples/showcase/anubis_vault/)** — an operational CLI
password / contacts vault for high-threat ops: dual REAL/DURESS worlds, Argon2id 19 MiB,
ChaCha20-Poly1305, verified-lane linear `fs.write`/`fs.read` caps, build-once multi-op product
binary, in-language `delete_file` destroy, and confinement with **no `net.send`**. Selftest
**16/16**; product battery **76 PASS / 0 FAIL** — the script is
[`examples/showcase/anubis_vault/scripts/thorough_test.sh`](../examples/showcase/anubis_vault/scripts/thorough_test.sh)
and its recorded run is [`TEST_RESULTS.md`](../examples/showcase/anubis_vault/TEST_RESULTS.md).

```bash
anubis check --verified examples/showcase/anubis_vault/vault.anb
anubis run examples/showcase/anubis_vault/vault.anb --allow-research   # → SELFTEST GATE PASSED: 16/16
anubis build examples/showcase/anubis_vault/vault_contacts.anb -o examples/showcase/anubis_vault/product
# then multi-op without recompile: product/anubis_out create|verify|list|delete-one|delete-all|destroy
```

---

## Verification — contracts, counterexamples, invariants

| Program | What it shows |
|---|---|
| [`ring_buffer_underflow.anb`](../examples/showcase/ring_buffer_underflow.anb) | the solver hands you the **counterexample** — `check` disproves `ensures(result >= 0)` at the wraparound state, then proves the fix |
| [`verified_private_settlement.anb`](../examples/showcase/verified_private_settlement.anb) | **contracts + secrets in one file**: the named SMT debit/credit obligations discharge and the fixture's explicit secret-to-public guards reject |
| [`verified_loop.anb`](../examples/showcase/verified_loop.anb) | a **loop invariant** discharged to establish a postcondition |
| [`suggest_contracts_demo.anb`](../examples/showcase/suggest_contracts_demo.anb) | `check --suggest-contracts` **infers** the missing `requires`/`ensures` for you |
| [`ennead_consensus_kernel.anb`](../examples/industry/ennead_consensus_kernel.anb) | Z3 proves a **BFT consensus kernel can't split-brain** (quorum-intersection, with a negative control) |

## Security — leaks that are compile errors

| Program | What it shows |
|---|---|
| [`tainted_input_to_shell_rejects.anb`](../examples/security/tainted_input_to_shell_rejects.anb) | **command injection is a compile error** — `input() → shell()` is `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` |
| [`http_trifecta_leg3_rejects.anb`](../examples/security/http_trifecta_leg3_rejects.anb) | secret **+** untrusted input **+** network egress → `ANUBIS_LETHAL_TRIFECTA` (the AI-agent exfil bug as a type error) |

The full corpus lives in [`examples/security/`](../examples/security/) — each `_rejects.anb` has a
paired `_accepts.anb`.

## Confinement and proof

| Program | What it shows |
|---|---|
| [`vz_confine_demo.anb`](../examples/showcase/vz_confine_demo.anb) | **the proof drives the hypervisor** — `vz confine` derives isolation from the program's proven effect set |
| [`amnesia_unlearning_witness.anb`](../examples/showcase/amnesia_unlearning_witness.anb) | a **machine-unlearning deletion witness** — `run --allow-research` over before/after manifests: verdict PASS on a clean purge, FAIL when data is retained ([how to run](../examples/showcase/AMNESIA.md)) |
| [`hermes_dual_path_seal.anb`](../examples/hermes_dual_path_seal.anb) | dual-path agreement seal (fact/sum/collatz/risk journal → Clear) |

## The language as a language

| Program | What it shows |
|---|---|
| [`hall_of_two_truths.anb`](../examples/hall_of_two_truths.anb) | language stress demo — structs/enums/maps/sort/hash-chain journal; `check` + `run` |
| [`anpu_recursive_sdf_renderer.anb`](../examples/anpu_recursive_sdf_renderer.anb) | recursive SDF ray-march + deterministic receipt (`ANUBIS_PROOF_INPUTS=proof_mode=0,challenge=0`) |
| [`programs/formal_kernel/`](../examples/programs/formal_kernel/) | pure-Anubis SAT kernel + independent Python oracle (12/12) |
| [`programs/double_entry_ledger/`](../examples/programs/double_entry_ledger/) · [`expense_ledger/`](../examples/programs/expense_ledger/) | accounting demos with balance invariants under `check`/`run` |
| [`programs/snake/`](../examples/programs/snake/) | pure-Anubis Snake (`play.sh` caches native binary for instant board) |

## Domain collections

| Directory | What it holds |
|---|---|
| [`examples/industry/`](../examples/industry/) | industrial / infrastructure programs — see its [README](../examples/industry/README.md) |
| [`examples/physics/`](../examples/physics/) | physics and simulation programs — see its [README](../examples/physics/README.md) |
| [`examples/proof/`](../examples/proof/) | zero-knowledge proving inputs and guests |
| [`examples/security/`](../examples/security/) | the security fixture corpus (accept/reject pairs) |

---

**Next:** [what each capability is and how far it goes](CAPABILITIES.md) ·
[the current open-issue list](CLAIMS.md) · [full documentation map](README.md)
