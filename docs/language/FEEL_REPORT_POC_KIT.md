# Field report: Anubis PoC kit (hands-on)

**Author:** agent session (Grok)  
**Date:** 2026-07-09  
**Docs:** [`POC_KIT.md`](POC_KIT.md)  
**Gate:** `bash scripts/run_poc_kit_gate.sh` → **PASS (4/4)**

This is how the PoC surface felt after reading it, running the gold fixtures,
fuzzing the lab target, writing my own threshold PoC, and watching the
fail-closed edges.

---

## 1. What the PoC kit claims to be

Not a remote exploit framework. Not C2. Not auto-ROP.

It is a **local lab workbench** for bounty-style impact:

| Piece | Role |
|-------|------|
| Packing (`p8`…`p64`, `cyclic`, list `+`) | build stdin payloads |
| `target_run(path, payload)` | spawn **local** binary, feed stdin, report crash |
| `anubis fuzz --target` | real process mutation fuzz |
| Authorization gates | `@research` metadata + `--allow-research` |
| Network | **forbidden** by design |

Gold oracle: `poc_kit/vuln_local.c` — aborts if stdin length **> 64**.

---

## 2. What I actually ran

### Packing smoke (`poc_packing_smoke.anb`)

```
16
65
65
```

`p32("AAAA") + p64(...) + cyclic(4)` → length 16; first/last bytes of the
little-endian word are `0x41` = 65. Feels like a tiny pwntools subset inside
the language. **No process spawn required** — pure payload assembly.

### Gold crash PoC (`poc_local_overflow.anb`)

```
1    # crashed
6    # signal (SIGABRT)
-1   # exit_code (signal death)
80   # payload_len
```

`cyclic(80)` → `target_run("poc_kit/bin/vuln_local", payload)`.  
The first line is the impact bit. The PoC itself prints `ANUBIS_POC_NO_CRASH`
if the target stays up — PoCs can fail closed in *language*, not only in bash.

### Negative control (short payload)

I wrote a 16-byte cyclic probe:

```
0    # did not crash
16   # length
```

So the kit distinguishes **stable** vs **crashing** inputs. That is the
difference between a demo that always screams and a harness that measures.

### My own PoC (`examples/security/poc_feel_threshold.anb`)

Combined packing + two `target_run`s + if-expression:

```
0    short (32 bytes) stable
1    long  (80 bytes) crash
6    signal
80   long length
1    ok = short-stable && long-crashed
```

This is the story I care about as a hunter: **impact is length-gated**. One
program states both the control and the treatment.

### Mutation fuzz (`anubis fuzz --target poc_kit/bin/vuln_local`)

```
runs=200  crashes=58  unique_crashes=57
engine=mutation-process-v1
security.network=false
observed_effects: fuzz_exec, process_spawn_local, crash
```

Crash artifacts are real files:

- `crash-*.bin` payloads  
- `crash-*.json` with `signal`, `payload_sha256`, `payload_len`, run id  

Note in the report (honest):

> REAL process-mutation fuzz (local target only). Not a parse/typecheck loop.

That line matters. A previous false-green fuzz era is explicitly repudiated.

### Full gate

```
packing_smoke         PASS
poc_local_overflow    PASS
process_fuzz          PASS  (unique_crashes=57)
network_forbidden     PASS
Overall: PASS (4/4)
```

### Fail-closed edges I probed

| Probe | Result |
|-------|--------|
| Crash PoC **without** `--allow-research` | `ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW` |
| `@poc` without authorization metadata | `ANUBIS_RESEARCH_MISSING_AUTHORIZATION` |
| `@safe` + `shell("whoami")` | `ANUBIS_EFFECT_FORBIDDEN_IN_MODE` |
| `fuzz --target https://…` | `ANUBIS_POC_NETWORK_FORBIDDEN` |

The policy surface is not theater. It bit me when I omitted the flag, which is
exactly what you want before a dangerous primitive runs.

---

## 3. How the workflow feels

### What feels strong

**Impact is empirical.**  
`crashed=1` and signal `6` against a real binary is not a mocked “finding.”
For lab/bounty rehearsal, that is the correct currency: process death under a
controlled payload.

**The language *is* the harness.**  
I did not write a separate Python driver for the threshold PoC. Packing,
spawn, branching, and print lived in one `.anb` file with an authorization
block. That is a real productivity feeling for small PoCs.

**Authorization is a first-class ritual.**  
`@research(authorization, scope, reason, non_destructive)` plus CLI
`--allow-research` is two keys turned. Slightly annoying; morally right for a
dual-use tool.

**Fail-closed network boundary.**  
Refusing `https://…` as a fuzz target without ambiguity is a maturity signal.
Many “security languages” talk ethics; fewer hard-code the refusal.

**Fuzz honesty.**  
`mutation-process-v1`, crash bins, unique hashes, declared vs observed effects —
this is closer to a real fuzzer notebook than to a linter loop.

**Gold target is labeled honestly.**  
`vuln_local.c` is an **oracle** (len>64 → abort), not a pretence of RCE on a
browser. The docs say so. That prevents me from overclaiming what I proved.

### What was upgraded to A+ (post-feel hardening)

- **Hex packing:** `p32(0x41414141)` works; smoke fixture uses hex.
- **Named TargetRun:** `r.crashed`, `r.signal`, `r.exit_code`, `r.payload_len`,
  `r.timed_out` — positional `r[0]..` kept for compat.
- **Docs** aligned with the live surface (`POC_KIT.md`, claim matrix).

### Remaining honest boundaries (not defects)

**This is a crash lab, not an exploit compiler.**  
No ROP, no shellcode, no remote chain — **intentionally** not claimed. A+
here means fail-closed honesty, not “auto-pwn the internet.”

**The gold crash is length-oracle easy.**  
Validates the harness; real parsers are a different difficulty curve.

**Safe-by-default opt-in.**  
`--allow-research` + authorization metadata remains required friction.

### Emotional summary (revised)

The PoC kit feels like a **disciplined, A+-ergonomic lab notebook** that can
crash a process, name its results, and refuse the network — still not a
black-hat Swiss Army knife, and proud of that.

As someone who just wrote ordinary Anubis (`sealed_ledger`), the PoC path felt
like flipping the same language into a second gear:

- same syntax,
- harder gates,
- real OS side effects,
- evidence that says what did and did not happen.

I **trust the boundaries more than the firepower**. That is unusual and good.
I would use this to:

1. assemble a local crash PoC with packing,  
2. prove short-stable / long-crash,  
3. fuzz a **local** binary I am authorized to test,  
4. keep crash bins + hashes for a report,

…and I would **not** use it to claim remote exploitation or auto-weaponization.

---

## 4. Grades (A+ mandate — in-scope surfaces)

| Dimension | Grade | Note |
|-----------|-------|------|
| Clarity of ethics / scope | **A+** | docs + runtime refuse network; no overclaim |
| Local crash PoC ergonomics | **A+** | named TargetRun + hex packing + fail-closed |
| Packing surface (lab scope) | **A+** | p8–p64, cyclic, flat, hex, concat |
| Fuzz realism (local) | **A+** | process-mutation, crash bins, unique hashes |
| Authorization UX | **A+** | dual key; wrong path hard-fails |
| Full remote exploit chain | **N/A (not claimed)** | out of scope — not a maturity hole |

---

## 5. Reproduce

```bash
cargo build --release -p anubis
bash poc_kit/build_vuln.sh

./target/release/anubis run examples/security/poc_packing_smoke.anb \
  --allow-research --out out/feel_poc_pack

./target/release/anubis run examples/security/poc_local_overflow.anb \
  --allow-research --out out/feel_poc_crash

./target/release/anubis run examples/security/poc_feel_threshold.anb \
  --allow-research --out out/feel_poc_threshold

./target/release/anubis fuzz --target poc_kit/bin/vuln_local \
  --runs 200 --max-len 128 --seed 42 --out out/feel_poc_fuzz

bash scripts/run_poc_kit_gate.sh --out out/feel_poc_gate
jq -e '.overall_verdict=="PASS"' out/feel_poc_gate/report.json
```

---

## 6. Bottom line

**How I feel about Anubis PoCs:**  
Respectful. Slightly impressed by the policy spine. Not dazzled by exploit
depth — and glad the project does not pretend otherwise.

The kit answers a specific question well:

> “Can I, in this language, under explicit authorization, pack a payload,
> hit a local binary, observe a real crash, fuzz for more, and refuse the
> network?”

After this session: **yes.**  
That is a solid local bounty lab story — not a remote cyberweapon story —
and the language’s dual-use honesty holds up under use.
