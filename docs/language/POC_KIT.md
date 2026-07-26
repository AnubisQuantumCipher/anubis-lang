# Anubis Bounty-Grade PoC Kit

**Status: under command (local lab process harness + packing + mutation fuzz in disposable VZ).**
Authorized dual-use only. Not a remote exploit framework, C2, or implant kit.

## What this is

A **bounty-grade weaponized PoC workbench** for *authorized local research*:

| Capability | Status | How |
|---|---|---|
| Packing (`p8`/`p16`/`p32`/`p64`, `cyclic`, list concat) | REAL | `anubis run --allow-research` |
| Local process harness (`target_run`) | REAL | spawn local binary, stdin payload, capture signal/crash |
| Mutation process fuzz | REAL | `anubis fuzz --target <local-bin>` |
| Crash evidence (payloads + JSON + optional bundle) | REAL | `out/fuzz/crashes/`, `--evidence` |
| Network / remote targets | **FORBIDDEN** | fail-closed (`ANUBIS_POC_NETWORK_FORBIDDEN`) |
| Auto ROP / shellcode / C2 | **NOT CLAIMED** | out of scope by design |

## Quick start

```bash
cd anubis-lang
cargo build --release -p anubis

# 1) Build the intentionally vulnerable local gold target
bash poc_kit/build_vuln.sh

# 2) Run the gold crash PoC in a disposable guest (must print crashed=1 first)
./target/release/anubis vz exploit --base anubis-xcode \
  examples/security/poc_local_overflow.anb --allow-research

# 3) Mutation-fuzz the same target in a disposable guest
./target/release/anubis vz fuzz --base anubis-xcode \
  poc_kit/bin/vuln_local --iterations 500 --allow-research

# 4) Full gate (the host entrypoint clones/runs/collects/deletes the guest)
bash scripts/run_poc_kit_gate.sh --out out/poc_kit
jq -e '.overall_verdict=="PASS"' out/poc_kit/report.json
```

## PoC language surface (`--allow-research`)

```anubis
@research(
  authorization: "local-lab-or-bug-bounty-program",
  scope: "local-fixture",
  reason: "crash PoC",
  non_destructive: true
)
fn main() {
    let payload = cyclic(80) + p64(0x0);
    let r = target_run("poc_kit/bin/vuln_local", payload);
    // A+: named TargetRun fields
    print(r.crashed);
    print(r.signal);
    print(r.payload_len);
    // list-compat still works: r[0] == r.crashed
}
```

### Builtins

| Builtin | Meaning |
|---|---|
| `p8(n)` / `p16(n)` / `p32(n)` / `p64(n)` | little-endian pack → list of byte ints (hex ints OK: `p32(0x41414141)`) |
| `cyclic(n)` | de Bruijn-style a..z pattern of length `n` |
| `flat(x)` | normalize value to byte list |
| `target_run(path, payload)` | run **local** binary with stdin=`payload`; returns **TargetRun** struct |
| list `+` list | concatenate (payload assembly) |

### TargetRun result (A+)

| Field | Meaning |
|---|---|
| `r.crashed` | `1` if killed by signal, else `0` |
| `r.signal` | Unix signal number, or `-1` |
| `r.exit_code` | process exit code, or `-1` |
| `r.payload_len` | byte length of payload fed to stdin |
| `r.timed_out` | `1` if harness timeout, else `0` |

Positional index `r[0]..r[4]` follows the same field order for backward compatibility.

## Fuzz

```bash
anubis fuzz --target ./my_local_parser --runs 1000 --max-len 256 --seed 1 --out out/fuzz
```

- Engine: **mutation-process-v1** (real `spawn` + stdin + signal detection)
- Writes `fuzz_report.json` and unique `crashes/crash-*.bin`
- Optional harness `.anb` may supply `@fuzz` authorization metadata; **engine always needs `--target`**
- The old parse/typecheck “1000 crashes” loop is **removed** (it was false-green)

## Policy

1. `--allow-research` required for PoC kit builtins and research/exploit blocks.
2. `@research` / `@poc` / `@fuzz` still require authorization metadata (typecheck).
3. Targets must be local filesystem paths. No `http://`, no raw network sinks in this kit.
4. Gold fixtures use an intentionally vulnerable binary under `poc_kit/` — not third-party production hosts.
5. **Isolation (mandatory, capability-preserving):**
   - Packing, the gold crash PoC, and mutation fuzz retain their implementation and evidence, but
     crash-capable execution runs only inside a disposable Apple VZ guest.
   - Official direct lanes:
     `anubis vz exploit --allow-research --base anubis-xcode examples/security/poc_local_overflow.anb`  
     `anubis vz fuzz --allow-research --base anubis-xcode poc_kit/bin/vuln_local`
   - `ANUBIS_POC_LAB_HOST` is not an override. Host research execution and fuzz fail closed.
   - **AOP C2 / inject / lateral** are separate and also VZ-only
     (`ANUBIS_OFFENSIVE_HOST_FORBIDDEN` on host).

## Honest boundary

This kit proves **impact in a lab** (reliable crash + packing + fuzz + evidence).  
It does **not** claim automatic exploitation of arbitrary remote software, gadget farming, or post-exploitation.

Gate: `bash scripts/run_poc_kit_gate.sh` (host orchestrator; disposable guest evidence)
