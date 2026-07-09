# Anubis Bounty-Grade PoC Kit

**Status: REAL (local lab process harness + packing + mutation fuzz).**  
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

# 2) Run the gold crash PoC (must print crashed=1 first)
./target/release/anubis run examples/security/poc_local_overflow.anb \
  --allow-research --out out/poc_local

# 3) Mutation-fuzz the same local target
./target/release/anubis fuzz \
  --target poc_kit/bin/vuln_local \
  --runs 500 --max-len 128 --seed 42 \
  --out out/fuzz_vuln

# 4) Full gate
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
    let payload = cyclic(80) + p64(0);
    let r = target_run("poc_kit/bin/vuln_local", payload);
    // r[0]=crashed, r[1]=signal, r[2]=exit_code, r[3]=payload_len
    print(r[0]);
}
```

### Builtins

| Builtin | Meaning |
|---|---|
| `p8(n)` / `p16(n)` / `p32(n)` / `p64(n)` | little-endian pack → list of byte ints |
| `cyclic(n)` | de Bruijn-style a..z pattern of length `n` |
| `flat(x)` | normalize value to byte list |
| `target_run(path, payload)` | run **local** binary with stdin=`payload`; returns crash tuple |
| list `+` list | concatenate (payload assembly) |

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

## Honest boundary

This kit proves **impact in a lab** (reliable crash + packing + fuzz + evidence).  
It does **not** claim automatic exploitation of arbitrary remote software, gadget farming, or post-exploitation.

Gate: `bash scripts/run_poc_kit_gate.sh`
