# Anubis Offensive Platform (AOP)

**Goal:** engagement-scoped, evidence-native red-team / exploit platform.  
**Not:** unscoped malware.

Protocol default: **`aop-2`** (AES-256-GCM encrypted beacons).

## T1–T7 status (this tree)

| Tranche | Capability | Status |
|---|---|---|
| **T1** | AES-GCM encrypt, PSK, agent key_id, jitter, mTLS cert material | **REAL** |
| **T2** | LaunchAgent plist + install helper; inject **plan-only** | **REAL** |
| **T3** | DNS lab transport + Unix domain socket C2 | **REAL** |
| **T4** | Lateral SSH (scoped); external host denied; SMB **plan-only** | **REAL** (SSH); SMB **PLAN_ONLY** (never executes) |
| **T5** | pattern-create/offset, gadget-search, browser harness | **REAL** |
| **T6** | XOR packer + C unpack stub + string-scramble | **REAL** |
| **T7** | Operator web console + RBAC (`role_can_queue` / `role_can_admin`) | **REAL** |

Gate: `bash scripts/run_offensive_platform_gate.sh` → **20/20 PASS**.

### Industry control plane (wired, not dead scaffolding)

| Surface | How it is real |
|---|---|
| `role_can_queue` / `role_can_admin` | HTTP `POST /task` and `task-queue --operator`; `GET /admin/status` requires Admin |
| `lateral_smb_plan` | CLI `lateral-smb` — structured plan, `executed=false`, no SMB sockets |
| `scramble_string` | CLI `string-scramble` + embedded in `pack-xor` as `name_scramble` |
| `TargetKind` / `AllowedTarget` | `engage-status` emits `allowed_targets`; `assert_host`/`assert_path` route through `target_in_scope` |
| Security fixture contract | `offensive-doctor --json` includes fail-closed needle honesty matrix |
| Agent build | Nested agent `Cargo.toml` has empty `[workspace]` (no parent workspace collision) |

## Quick start

```bash
cargo build --release -p anubis
BIN=./target/release/anubis

$BIN engage-init --dir out/engagements/lab --authorization local-lab-charter
# engagement.json includes psk_hex, operators, jitter, mtls certs under certs/

# Terminal A — multi transport (HTTP + DNS + UDS)
$BIN listen --engage out/engagements/lab
# open console: http://127.0.0.1:4444/

# Terminal B — encrypted agent
$BIN agent-generate --engage out/engagements/lab --name agent0 --sleep-ms 1000
out/engagements/lab/agents/agent0 &
$BIN task-queue --engage out/engagements/lab --module whoami --operator operator

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
$BIN engage-status --dir out/engagements/lab --json   # includes allowed_targets + receipt tip
$BIN receipt-verify --engage out/engagements/lab --json  # hash-chained action receipts
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
engagement.json     scope + PSK + RBAC + transport + jitter + kill date
certs/              self-signed lab server cert (mTLS-ready)
agents/             cargo-built aop-2 agents
listeners/          meta
tasks/inbox.jsonl   operator queue
loot/               results
evidence/           actions.jsonl
persistence/        LaunchAgent plists
packs/              XOR packs
modules/            exploit JSON + browser harness
```

### Transports

| Transport | Bind | Notes |
|---|---|---|
| HTTP/JSON | `c2_bind` default `127.0.0.1:4444` | primary + `/` console |
| DNS lab | `dns_bind` default `127.0.0.1:5353` | presence / lab channel |
| UDS | `uds_path` default `/tmp/anubis-aop.sock` | local pipe C2 |

### Protocol aop-2

Wire body:

```json
{"protocol":"aop-2","engagement_id":"…","agent_id":"…","blob":"<base64 nonce||aes-gcm-ct>"}
```

Inner JSON is Beacon / BeaconResponse / TaskResult.

### RBAC

Operators in engagement: `admin`, `operator`, `readonly`.  
Console/`POST /task` requires Operator+.

## Policy (product, not optional)

- Fail closed on scope, kill date, missing auth, lateral host list.
- Default loopback C2; non-loopback needs explicit flags.
- Live process injection is **plan-only** until explicit red-team enablement.
- Evidence JSONL for beacons/results.

## Still deeper (future polish, not claimed done)

- Full rustls mTLS handshake on listener (certs are generated; HTTP remains default)
- Production DNS/DoH C2 codec
- Windows SMB/WinRM lateral
- Live process injection under double authorization
- Multi-operator token auth (hashes fields exist)

These are incremental hardenings on a **working platform**, not greenfield.
