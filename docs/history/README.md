# Archive — nothing here is a current claim

Every file in this directory is a **dated record of a past effort**. It is kept because this project
does not delete its own history: a plan that was wrong, an audit that was superseded, and a seal that
has since gone stale are all evidence about how the work was done.

**None of it describes the tree you have checked out.** For current status, read
[`docs/CLAIMS.md`](../CLAIMS.md) — the project's declared single source of truth — and start from
[`docs/README.md`](../README.md) if you are looking for anything else.

These files were moved here from the repository root on 2026-07-28. They were not edited beyond
repointing their internal links; their content is preserved exactly as sealed.

---

## What each file is

| File | Dated | What it was for | Why it is archived |
|---|---|---|---|
| [`ANUBIS_BUILD_MISSION.md`](ANUBIS_BUILD_MISSION.md) | 2026-07-05 | The task brief handed to the autonomous build agent: ground-truth baseline, finish line, phase contracts. | **Its §0.2 states as "VERIFIED GROUND TRUTH" that Anubis is NOT Turing complete — "No iteration exists anywhere", "Recursion does not execute".** That was true of the tree it was written against. It is the *starting* state, and the work it commissioned has since landed. [`LANGUAGE.md`](../../LANGUAGE.md) is correct and this file is not. |
| [`ANUBIS_REALITY_AUDIT.md`](ANUBIS_REALITY_AUDIT.md) | 2026-07-05 | A single adversarial audit that graded the repo **C** and enumerated real / partial / scaffolding / overclaimed. | The project's baseline measurement. Never updated since the day it was written. |
| [`ANUBIS_CAPABILITY_CLAIM_MATRIX.md`](ANUBIS_CAPABILITY_CLAIM_MATRIX.md) | 2026-07-05 | The same audit run in table form, one verdict per claim. | Duplicates the audit above; its rows seed the head of [`MATURITY_CLAIM_MATRIX.md`](../../MATURITY_CLAIM_MATRIX.md). Self-labelled "Historical baseline, not current state." |
| [`ROADMAP_A_PLUS.md`](ROADMAP_A_PLUS.md) | 2026-07-05 → 07-24 | The 11-phase plan for driving the repo from that C-grade baseline to an "A+" label. | Every phase it tracks is closed, and it hands live status to [`docs/language/ROADMAP.md`](../language/ROADMAP.md) in its own header. |
| [`A_PLUS_FINAL_REPORT.md`](A_PLUS_FINAL_REPORT.md) | 2026-07-24 | The verdict blocks of the A15 re-seal. | A snapshot of one seal. Its own header flags a test count it admits is wrong. |
| [`A_PLUS_CLOSEOUT.md`](A_PLUS_CLOSEOUT.md) | 2026-07-24 | Declares the A+ phases closed, lists the evidence package and findings F1–F4. | Same seal as the report above — the two are one document split in half. Self-labelled "not a current completion claim." |

---

## Reading a sealed document safely

Three of these files carry `DONE` / `PASS` / `CLAIMED` / `COMPLETE` language. That language was
accurate **on its seal date** and is not a statement about today. The project's own status
vocabulary, from [`docs/CLAIMS.md`](../CLAIMS.md):

> A claim is (a) re-runnable with command + observation, (b) sealed under a dated artifact path,
> (c) **partial** with the gap named, or (d) **not claimed**.

Everything in this directory is category (b). Any seal dated **2026-07-24 or earlier** predates every
item in the current open-issues list — so where a file here says something is closed and
[`docs/CLAIMS.md`](../CLAIMS.md) says it is open, **the claims file is correct**.
