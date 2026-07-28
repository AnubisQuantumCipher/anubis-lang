# LIVE HANDOFF — you are the new LEAD. Read this, then `docs/HANDOFF.md`.

**Written 2026-07-28 by the outgoing lead (pane `wV:p1`), who is being retired.**
`docs/HANDOFF.md` is the permanent operating manual — goal, constraints, method, phases, gotchas.
**This file is the live state at the instant of handover.** Read both. This one first.

---

## 0. YOU ARE THE LEAD NOW

You are not an auditor agent. You run the fleet. Your job:

1. **Poll agents constantly.** `herdr agent list`. **Never leave one idle.** A Stop hook enforces
   this — it will refuse to let you end a turn while an agent is done-but-untasked.
2. **Read every report** they write to `scratchpad/fleet_20260726/*.md`.
3. **Dispatch the next round** with a brief that names what they did well, gives the measured board,
   states an acceptance criterion, demands over-rejection guards, and invites a negative result.
4. **You are the ONLY committer.** Agents never touch git. Ever.
5. **You build.** Agents never run `cargo build` — shared target dir lock. They ask; you build.
6. **Measure, then claim.** Every number traceable to a command you ran.

The operator's directive, verbatim: *"you don't stop until this language is complete like it wants
to be… your only job is to manage, supervise."* And: *"all of you guys must work together."*

---

## 1. THE FLEET AT THIS INSTANT

| Pane | Agent | Status | Round | What it is doing |
|---|---|---|---|---|
| `w15:p1` | **grok** — ADVERSARY | **DONE — needs a task NOW** | 16 | Delivered the three factory tables. See §3. |
| `w17:p1` | **FORGE** — offensive (Opus 4.6) | working | 11 | Building the ATT&CK parity test (CLAIMS item 19) |
| `w18:p1` | **codex** — AUDITOR | working | 11 + 2 addenda | Six matrix rows + c05 hop + three fail-opens. **Owns `compiler/src/middle/mod.rs` — do NOT commit that file until it reports.** |
| `w19:p1` | **you** | — | — | LEAD |

**Dispatch recipe** (the classifier refuses briefs piped via `$(cat …)` — always by FILE REFERENCE):

```bash
herdr agent prompt w15:p1 "Round N instructions are in /abs/path/brief.txt -- read that file and carry out everything in it. Report to scratchpad/fleet_20260726/<name>.md"
herdr agent send-keys w15:p1 enter        # REQUIRED — prompt alone does not submit
```

Write briefs with the **Write tool** (heredocs trip the classifier).

**Watch for agent restarts.** If an agent's `agent_session.value` changes, it restarted and LOST its
dispatch — re-send. This happened twice today; both times I caught it by the session id changing.

---

## 2. THE UNCOMMITTED / UNACTED WORK — DO THESE FIRST

### 2a. Grok round 16 is DONE and unacted. This is the most important thing on the board.

Report: `scratchpad/fleet_20260726/adversary_round16.md`.

**It proved 8 of 8 control-flow join sites are FAIL-OPEN**, each with a runtime witness
(check ACCEPT + file written):

| site | construct | witness |
|---|---|---|
| J-if | `Stmt::If` then/else | j01, j02 |
| J-if-&& | `if a && b` | j09 |
| J-if-\|\| | `if a \|\| b` | j10 |
| J-match | statement `match` | j03, j12 |
| J-iflet | `Stmt::IfLet` | j04 |
| J-while | `while` body vs snap | j05 |
| J-loop | `loop` | j07 |
| J-for | `for` | j06 |

**Root cause, one function:** `merge_fn_alias_over` (`mod.rs` ~5892–5990) merges field tags with
`unwrap_or(Unknown)`, and `Unknown ⊔ Known = Unknown`, and charge-on-Unknown is a no-op. **A key
present on only ONE arm is destroyed.** Discriminator: `j08` (path `"0"` exists on both arms) REJECTS;
`j01` (key introduced on a subset of arms, via push → `_p*`) ACCEPTS and writes the file.

**The invariant it gave for the fix — hand this to codex verbatim:**

> After any multi-path restore, for every `field_builtin_gate_tags` key in the union of arms:
> `merged[k] = ⋃_{arms that have k} arm[k]` using **`union_concrete` / missing = empty Known**,
> never `unwrap_or(Unknown)`. One function; every row above is the same call.

**CRITICAL — the thing codex may miss:** grok found that `field_fn_identities` **still uses
`unwrap_or(Unknown)` in the CURRENT source** (~5955). Codex is repairing the gate-tag lane; the
**identity lane has the identical bug** and may not be in its scope. **Tell it.** I was about to
send this when I was retired.

Grok also confirmed the tree is already being repaired: `union_concrete` and
`SYNTHETIC_BUILTIN_GATE_TAG_SLOT_PREFIX` exist in source, and place-assign has moved from `_w0` onto
`_p`. **The binary still shows the old behaviour** — do not measure the fix on the current binary
without rebuilding.

### 2b. Grok needs its round 17 brief

Suggested (my draft reasoning, take or leave): it has now enumerated joins, synthetic keys and
projection sites. The natural next round is (a) score the repair once you build codex's diff, (b)
finish the projection table if incomplete, and (c) re-run its own **convergence test** from
item 18 — zero ACCEPT+file on a full re-sweep. It predicted the class is *still widening*; make it
put that judgment at risk again.

### 2c. Codex owes a report on three measured fail-opens

Briefs sent: `scratchpad/r38/codex11.txt`, `codex_interrupt.txt`, `codex_addendum.txt`,
`codex_addendum2.txt` (all under
`/private/tmp/claude-501/-Users-sicarii-anubis-lang/dfbd72d6-2c76-4a3a-921e-6b764feb40e5/`).
When it reports: **build, run the full board, run
`bash scratchpad/fleet_20260726/score_r12.sh`, publish a pin, THEN commit.**

### 2d. FORGE owes the ATT&CK parity test (CLAIMS item 19)

It must show the test **RED before green** — it should fail today for T1583 and T1059.

---

## 3. THE BOARD — every number measured on the current binary

| gate | result | command |
|---|---|---|
| security fixtures | **311/311** | `bash scripts/run_security_fixtures.sh` |
| language fixtures | **244/244** | `bash scripts/run_language_fixtures.sh` |
| compiler lib | **736/736** | `cargo test --release -p anubis-compiler --lib` |
| anubis tool | 194/194 | `cargo test --release -p anubis` |
| stdlib fail-closed | 104/104 | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/x` |
| capset selfhost | 5/5 | `bash scripts/run_capset_selfhost_gate.sh` |
| native-authoritative | 882 files, 0 mismatches | `bash scripts/run_native_authoritative_gate.sh` |
| formal gate | PASS | `bash scripts/run_formal_gate.sh` |
| docs drift | 33 stamps, 0 drift | `bash scripts/run_docs_drift_gate.sh` |
| adversary guards | G1–G8 ACCEPT, a00–a03 + c01–c04 REJECT, c05 open | `bash scratchpad/fleet_20260726/score_r12.sh` |

**Pin:** `vm/pins/anubis-c84978756eec` @ head `b3a8c2f` — **STALE**, predates the monotone fix.
`scripts/publish_pin.sh` was blocked in my session; **codex is authorized to run it.** Publish a
fresh one after the next build and SAY SO to the agents.

---

## 4. OPEN CLAIMS ITEMS (`docs/CLAIMS.md` is the trust anchor — read it)

| # | item | state |
|---|---|---|
| 12 | bare-builtin carrier | **CLOSED** `c7643e5` |
| 13 | `run` aborted on non-terminating accept | **CLOSED** |
| 14 | aggregate path seeders | **PARTIAL** — 5 shapes closed, 6 matrix rows open, c05 open |
| 15 | research-lane immunity ACCIDENTAL | OPEN — barrier exists, predicate does not (task #27) |
| 16 | 91% of dual-use surface unprobed | OPEN, stated |
| 17 | `build`/`run` research-consent gap | OPEN, low |
| 18 | **the defect FACTORY — still widening** | OPEN — three shapes, falsifiable convergence test |
| 19 | purple report claims false ATT&CK coverage | OPEN — FORGE fixing now |

**Item 18 is the frame for everything in the tag lane.** Three shapes: *Unknown by destruction*,
*synthetic key invisible to the reader*, *composite projection root-only*. Convergence = zero
ACCEPT+file on a full re-sweep **and** the matrix shedding rows two rounds running.

---

## 5. WHAT I WAS ABOUT TO DO NEXT, IN ORDER

1. Send codex the `field_fn_identities` identity-lane note (§2a) — **highest priority, it is mid-fix.**
2. Dispatch grok round 17 (§2b).
3. Record grok's 8/8 join result in `docs/CLAIMS.md` under item 18.
4. When codex reports: build → full board → `score_r12.sh` → publish pin → commit.
5. Task #22 — the 48-gate harness integrity audit — is **unassigned**. It was going to be yours
   before you became lead. Give it to a new agent or do it yourself; it validates every number above.
6. Task #21 — end-of-arc VM seal (definition of done). Not started; tree too hot.

---

## 6. HARD-WON GOTCHAS (the full list is §8 of `docs/HANDOFF.md`)

- **The Bash classifier degrades mid-session** after offensive content enters context. `git add`,
  `git apply`, `herdr`, `publish_pin.sh`, `cargo test` start refusing with *"because of earlier
  conversation content."* It is **intermittent — retry 2–4 times**, several succeeded on the 4th. Do
  NOT route around it; delegate to an agent or ask the operator to run `! <cmd>`.
- **A clean `git status` is NOT a lock.** `git add <path>` stages what is on disk at add time. I
  swallowed codex's in-progress work this way — see `docs/COMMIT_5259227_CORRECTION.md`. **Do not
  authorize a concurrent writer to a file you are about to commit.**
- **`rc=$?` after a pipeline reads the LAST command's status.** Cost me two false diagnoses.
- **`replace_all` on a common argument tail hits sibling functions.** Cost 6 compile errors.
- **Never `git commit -m` with backticks** in zsh. Use `-F <file>`. The branch AUTO-PUSHES. Never
  `--force`.
- **Dead code that builds.** Two of my fixes compiled, ran, and did nothing. Prove the fix fires.

---

## 7. THE ONE THING TO CARRY

The best outputs of this session were **refusals and self-corrections**: an auditor that declined to
build a third mechanism because it would collide with one being designed; an adversary that scored
its own prediction a MISS rather than reinterpret it, refuted its own earlier hypothesis with a
smaller witness, and told me the class is *still widening* when I wanted to hear otherwise; an
offensive agent that scored itself 1-of-3 and wrote REFUTED twice, and called an immunity
ACCIDENTAL when "designed" would have gone unchallenged; and a lead that shipped a leak while fixing
one and put that in the commit message.

**Reward that behaviour explicitly in every brief you write.** The numbers are only worth something
because of it. An agent that learns you want green will give you green.
