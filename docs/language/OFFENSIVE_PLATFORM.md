# Anubis Offensive Platform (AOP)

**Goal:** engagement-scoped, evidence-native red-team / exploit platform.  
**Not:** unscoped malware.

## Isolation (permanent; offensive power preserved inside VZ)

**AOP red-team platform execution** (C2, inject, lateral, engagement packer, …)
runs **only** inside Apple Virtualization (tart + Virtualization.framework,
golden base `anubis-xcode`, SSH `admin` + `~/.ssh/tart_anubis`).

**Bounty PoC kit** (packing + gold `poc_kit/bin/vuln_local` + fuzz) keeps the same
packing, crash, and mutation capabilities, but crash-capable execution is now mandatory inside the
same disposable VZ/tart boundary. The host orchestrates and collects evidence; it is not a fallback
runner.

| Host allowed | VZ guest required |
|---|---|
| engage-init / status, doctor, catalogs, plans | **listen**, agent-generate, task-queue |
| PoC source editing, static `check`, reports, receipt verification | `run --allow-research`, all fuzz, inject-plan, lateral-*, exploit-run |
| purple-report on guest loot, receipt-verify | pack-xor, persist-launchagent, recon-scan |
| pattern/gadget math, vz status/doctor/* | PoC kit crash target and mutation fuzz |
| | string-scramble (AOP packer helper) |

AOP host attempts → **`ANUBIS_OFFENSIVE_HOST_FORBIDDEN`**.
Host research execution → **`ANUBIS_RESEARCH_HOST_FORBIDDEN`**.
Any host fuzz → **`ANUBIS_FUZZ_HOST_FORBIDDEN`**.

Guest markers: `ANUBIS_VZ_GUEST=1`, `ANUBIS_OFFENSIVE_GATE_IN_GUEST=1`,
`ANUBIS_ISOLATION=*tart*`, `$HOME/.anubis-vz-guest`, `kern.hv_vmm_present=1`.

Protocol default: **`aop-2`** (AES-256-GCM encrypted beacons).

## T1–T7 status (this tree)

| Tranche | Capability | Status |
|---|---|---|
| **T1** | AES-GCM encrypt, PSK, agent key_id, jitter, mTLS cert material, **rustls mTLS handshake (opt-in)** | **REAL** |
| **T2** | LaunchAgent plist + install helper; inject **plan-only by default**; **live inject under double authorization** | **REAL** |
| **T3** | **DNS/DoH C2 codec** (`aop-dns-v1`) + Unix domain socket C2 | **REAL** |
| **T4** | Lateral SSH (scoped); external host denied; SMB **plan-only** | **REAL** (SSH); SMB **PLAN_ONLY** (never executes) |
| **T5** | pattern-create/offset, gadget-search, browser harness | **REAL** |
| **T6** | XOR packer + C unpack stub + string-scramble | **REAL** |
| **T7** | Operator web console + RBAC + **multi-operator token auth** | **REAL** |
| **T8** | Apple VZ sandbox (`vz exploit|fuzz|c2-cycle|…`) | **REAL** |
| **T9** | ATT&CK kill-chain, OPSEC score, recon, malleable C2, campaign playbook, purple-team report, phish PLAN_ONLY, LOLBAS catalog | **REAL** |

Gate: `bash scripts/run_offensive_platform_gate.sh` (host entrypoint is VZ-isolated by default).
Host clones a disposable tart guest from `anubis-xcode`, runs the full gate inside the guest,
pulls the report back, and deletes the guest unless explicitly kept. **34/34** includes T9.

**Sealed T9 evidence (2026-07-24):** `out/a15_offensive_t9_20260724-152746/report.json` →
`overall_verdict=PASS`, `total=34`, `passed=34`, `isolation=tart-disposable-guest`. Host entrypoint
strips a stale `$HOME/.anubis-vz-guest` marker before orchestration (leftover marker was a host
fail-open); guest hops always `cargo build --release -p anubis` so T9 CLI surfaces cannot be
skipped by a stale guest binary.

### Industry control plane (wired, not dead scaffolding)

| Surface | How it is real |
|---|---|
| `role_can_queue` / `role_can_admin` | HTTP `POST /task` and `task-queue --operator`; `GET /admin/status` requires Admin |
| Multi-operator tokens | `operator-token-issue` stores SHA-256 only; privileged routes require `X-Anubis-Token` / `--token` when hash set |
| rustls mTLS | `listen --mtls` requires client cert signed by engagement CA; plain HTTP remains default |
| DNS/DoH codec | QNAME `seq.total.kind.<b32>.aop.c2` + TXT answers; `POST /doh` + RFC 8484 `/dns-query` |
| Live inject (double auth) | `--allow-research-inject` **and** `program=red_team` or `allow_live_inject=true`; default PLAN_ONLY |
| `lateral_smb_plan` | CLI `lateral-smb` — structured plan, `executed=false`, no SMB sockets |
| `scramble_string` | CLI `string-scramble` + embedded in `pack-xor` as `name_scramble` |
| `TargetKind` / `AllowedTarget` | `engage-status` emits `allowed_targets`; `assert_host`/`assert_path` route through `target_in_scope` |
| Security fixture contract | `offensive-doctor --json` includes fail-closed needle honesty matrix |
| Agent build | Nested agent `Cargo.toml` has empty `[workspace]` (no parent workspace collision) |

## Quick start

The operational commands below run inside an Anubis-managed guest. Use `anubis vz exec` for an
interactive guest shell or the sealed `run_offensive_platform_gate.sh` lifecycle; do not execute
them directly on the host.

```bash
cargo build --release -p anubis
BIN=./target/release/anubis

$BIN engage-init --dir out/engagements/lab --authorization local-lab-charter
# engagement.json includes psk_hex, operators, jitter
# certs/: ca + server + client PEMs (mTLS-ready)

# Terminal A — multi transport (HTTP + DNS + UDS); HTTP default
$BIN listen --engage out/engagements/lab
# open console: http://127.0.0.1:4444/
# DoH: POST http://127.0.0.1:4444/doh  {"qname":"0.1.p.x.aop.c2"}

# Optional full rustls mTLS (client cert required)
$BIN listen --engage out/engagements/lab --mtls
# curl --cert certs/client.crt.pem --key certs/client.key.pem --cacert certs/ca.crt.pem https://127.0.0.1:4444/health

# Terminal B — encrypted agent
$BIN agent-generate --engage out/engagements/lab --name agent0 --sleep-ms 1000
out/engagements/lab/agents/agent0 &
$BIN task-queue --engage out/engagements/lab --module whoami --operator operator

# Multi-operator token auth
$BIN operator-token-issue --engage out/engagements/lab --operator operator --json
# → prints cleartext once; engagement.json stores token_hash only
$BIN task-queue --engage out/engagements/lab --module whoami --operator operator --token <TOKEN>

# Inject: PLAN_ONLY by default
$BIN inject-plan --engage out/engagements/lab --pid 1 --shellcode /tmp/sc.bin
# Live under double authorization (lab victim when pid=0):
#   1) --allow-research-inject
#   2) engagement.program=red_team OR allow_live_inject=true
$BIN inject-plan --engage out/engagements/lab --pid 0 --shellcode /tmp/sc.bin --allow-research-inject

# Operator tools
$BIN pattern-create --len 100
$BIN pattern-offset --len 200 --needle abcd
$BIN pack-xor --engage out/engagements/lab --input ./some.bin
$BIN string-scramble --text lab_note
$BIN persist-launchagent --engage out/engagements/lab --agent out/engagements/lab/agents/agent0
$BIN lateral-ssh --engage out/engagements/lab --host 127.0.0.1 --cmd hostname
$BIN lateral-smb --engage out/engagements/lab --host 127.0.0.1   # PLAN_ONLY
$BIN browser-harness --out out/engagements/lab/modules/browser --url http://127.0.0.1:8000/
$BIN offensive-doctor --json
$BIN engage-status --dir out/engagements/lab --json
$BIN receipt-verify --engage out/engagements/lab --json

# T9 elite control plane (host-safe plans/catalogs; recon-scan is VZ-only)
$BIN attck-catalog --json
$BIN attck-map --action inject-plan --json
$BIN opsec-score --engage out/engagements/lab --json
$BIN malleable-init --engage out/engagements/lab
$BIN campaign-init --engage out/engagements/lab
$BIN phish-plan --engage out/engagements/lab --theme password_reset
$BIN lolbas-catalog --json
$BIN purple-report --engage out/engagements/lab --out out/engagements/lab/loot/purple
$BIN recon-hostinfo --engage out/engagements/lab
# in guest: $BIN recon-scan --engage out/engagements/lab --host 127.0.0.1
```

### Action receipts (tamper-evident)

Every sealed operator action appends to `evidence/receipts/chain.jsonl` with
`prev_hash` → `receipt_hash` binding. Tip at `evidence/receipts/tip.json`.

```bash
anubis receipt-verify --engage out/engagements/lab --json
# tamper tip or rewrite a line → ANUBIS_RECEIPT_* fail-closed
```

## Architecture

```
engagement.json     scope + PSK + RBAC + transport + jitter + kill date + token hashes
certs/              CA + server + client PEMs (full mTLS material)
agents/             cargo-built aop-2 agents
listeners/          meta (mtls_active, dns_codec, doh)
tasks/inbox.jsonl   operator queue
loot/inject/        live inject artifacts + reports
evidence/           actions.jsonl + receipts/
persistence/        LaunchAgent plists
packs/              XOR packs
modules/            exploit JSON + browser harness
```

### Transports

| Transport | Bind | Notes |
|---|---|---|
| HTTP/JSON | `c2_bind` default `127.0.0.1:4444` | primary + `/` console; **default** |
| HTTPS mTLS | same bind with `listen --mtls` | rustls; client cert required |
| DNS C2 | `dns_bind` default `127.0.0.1:5353` | production codec `aop-dns-v1` |
| DoH | `POST /doh`, `POST/GET /dns-query` | JSON convenience + RFC 8484 wire |
| UDS | `uds_path` default `/tmp/anubis-aop.sock` | local pipe C2 |

### DNS/DoH codec (`aop-dns-v1`)

- Payload: base32 (no pad) of aop-2 JSON/envelope bytes, split into ≤60-char labels.
- QNAME: `seq.total.kind.<labels…>.aop.c2` where `kind` ∈ `b` (beacon), `r` (result), `p` (poll).
- Response: TXT RDATA with base32 response chunks (or `OK` / `ACK.N` / `DENY`).
- Fragment reassembly server-side; empty `0.1.p.x.aop.c2` is a heartbeat poll.

### Protocol aop-2

Wire body:

```json
{"protocol":"aop-2","engagement_id":"…","agent_id":"…","blob":"<base64 nonce||aes-gcm-ct>"}
```

Inner JSON is Beacon / BeaconResponse / TaskResult.

### RBAC + tokens

Operators in engagement: `admin`, `operator`, `readonly`.  
Console/`POST /task` requires Operator+.  
When an operator has a non-empty `token_hash`, privileged routes require a matching token
(`X-Anubis-Token` header or `task-queue --token`). Cleartext is never stored.

### Live inject — double authorization

| Gate | Requirement |
|---|---|
| Default | `status: PLAN_ONLY`, `executed: false` |
| CLI half | `--allow-research-inject` |
| Engagement half | `program == "red_team"` **or** `allow_live_inject: true` |
| Live path | Lab victim loader (pid `0`) or cooperative remote (payload drop + SIGUSR1); honest boundary — no silent SIP-bypass claim |

## Policy (product, not optional)

- Fail closed on scope, kill date, missing auth, lateral host list, invalid tokens.
- Default loopback C2; non-loopback needs explicit flags.
- HTTP remains the default listener; mTLS is opt-in.
- Live process injection is plan-only until **both** authorization halves are present.
- Evidence JSONL + hash-chained receipts for beacons/results/injects.

## Still deeper (future polish)

- Windows SMB/WinRM lateral **execution** (still PLAN_ONLY by design)
- Arbitrary remote RWX thread inject under platform entitlements (lab path is cooperative/loader)

These are residual widenings on a **working platform**, not greenfield.
