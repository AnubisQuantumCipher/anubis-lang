# NEXUS — Networked EXecution Under Sealed policy

Built 2026-07-11 as a production-shaped **Anubis industry kernel**, then hardened under a **strict external gate**.

## Purpose

Close the gap between software that **acts** (agents, CI bots, authorized red-team modules) and software that can **prove what it was allowed to do**.

## Artifacts

| File | Role |
|------|------|
| `nexus_execution_kernel.anb` | Full L0–L6 action kernel + strict scenario battery |
| `nexus_zk_decision.anb` | Public decision-function companion (6 cases) |
| `nexus_zk_decision_proof.anb` | Minimal disclose decision (`decision=3`) |
| `../../scripts/run_nexus_gate.sh` | **External oracle gate** (independent of soft claims) |
| `NEXUS.md` | This note |

## Verified locally (strict)

```bash
cd /Users/sicarii/anubis-lang
./target/release/anubis run examples/industry/nexus_execution_kernel.anb
# → NEXUS_KERNEL_CERTIFIED · seals 10/10 · strict 15/15 · neg 10/10

bash scripts/run_nexus_gate.sh out/nexus_gate
# → NEXUS_GATE_OVERALL=PASS
```

### Exact demo journal (external oracle)

```text
ALLOW,ALLOW,ALLOW,DENY,ALLOW,HOLD,ALLOW,DENY,DENY,HOLD,ALLOW,ABORT,ABORT,ABORT,ABORT
counts: ALLOW=6 HOLD=2 DENY=3 ABORT=4 WATCH=0
fuel=34  receipt=6  frozen=1
PUBLIC_ROOT chain=345888153
```

### Hardening fixes applied during thorough test

1. Removed soft Allow↔Watch / Deny↔Abort matching (false-green risk)
2. HashOnly/Redacted disclose is **governed ALLOW** (not elevated Watch)
3. Expanded negative controls **6 → 10** (auditor shell, low trust secret, exploit outside lab, unknown actor; strict Abort for prod escape)
4. External gate: dual-run determinism, journal parser, Python decide-oracle, evidence truth block, chain pin
5. Proof mini now prints `decision=3` so the gate can assert it (Anubis process exit is not the program return code)

## Honesty

- Offline policy kernel only
- Hybrid lane markers are **plan-only** (`executed=0`)
- Not a live sandbox, not malware, not unscoped C2
- Dual-use exploit path requires charter `allow_offensive` + lab path + witness for agents
