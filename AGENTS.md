# AGENTS.md — Anubis A+ Maturity

This repo is now under Grok Build autonomous compiler-lab execution per the approved plan.

**Primary document:** `.grok/sessions/.../plan.md` (or the copy in root if synced) + `ROADMAP_A_PLUS.md` + `A_PLUS_ACCEPTANCE_CRITERIA.md`.

## Roster (A0–A15)
See plan for full responsibilities. Critical rule:

> No agent work merges until A15 (Red Team / Independent Auditor) reproduces the claimed improvement with commands + artifacts.

## Safety
- Run `bash tools/grok-safety-check.sh` before destructive commands.
- Respect `.grok/hooks/pretool-safety.json`.
- Only work on `a-plus-maturity/*` or isolated worktrees (`../anubis-worktrees/`).

## Process
1. Re-run PHASE 0 diagnostics on every session start.
2. Update todo list.
3. Produce evidence for every claim.
4. A15 hostile verification after major phases.
5. Final sealed run in `implementer/a_plus_audit_run/<STAMP>/`.

Current branch baseline commit recorded in plan. All future work cites post-baseline evidence.

**Do not make claims that are not backed by code, test, command output, generated artifact, or sealed evidence.**
