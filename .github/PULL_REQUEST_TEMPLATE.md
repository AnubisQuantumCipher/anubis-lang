<!-- One bounded slice = one short-lived branch + one PR. Every Phase 2 slice must use this contract. Never force-push. -->

## Slice identity and scope

- Linked issue/private false-accept record (required for Phase 2/security enforcement; otherwise N/A with reason):
- Base SHA:
- Full head SHA:
- Owned paths:
- Out of scope:
- [ ] This branch contains one coherent slice and has not been force-pushed.
- [ ] Code/config and docs are separate commits; fixtures stay with code.
- [ ] No unrelated dirty-tree work was imported.

## Immutable evidence identity

- `ANUBIS_BIN` absolute path:
- Binary SHA-256 / mtime:
- `scripts/publish_pin.sh --verify` output and immediate rc:
- Evidence roots and artifact hashes:

## Required evidence

- [ ] Every command below records its exit code on the immediately following line.
- [ ] For an enforcing/security change, baseline DIRECT is rejected and baseline
      LAUNDERED/alternate-carrier is accepted, with exact output. If the negated twin also accepts,
      this is a symmetric blind spot rather than a padded finding. Otherwise: N/A with reason.
- [ ] For an enforcing/security change, post-fix LAUNDERED and DIRECT forms both reject with the
      intended diagnostic. Otherwise: N/A with reason.
- [ ] An accept-side over-rejection guard exists for every enforcing change, or N/A is justified.
- [ ] Named targeted tests report exact cardinality; no substring-only test claim.
- [ ] Full current corpus old/new verdict diff has zero unexpected flips and zero timeouts.
- [ ] The exact hosted roster is derived at this head SHA (29 gates at this template epoch); G9 is
      `EXTERNAL`, never PASS or silently skipped.
- [ ] Source-current host seal and any required VZ/offensive receipt bind guest, roster, fixpoint,
      source identity, validator, teardown, and artifact hashes.
- [ ] Claim/docs impact is stated and points to the living `docs/CLAIMS.md` residual.
- [ ] Independent read-only review is bound to these exact commits and artifacts.

## Boundary and merge discipline

<!-- State exactly what this proves and does not prove. Use [NEEDS-HUMAN] for unperformed scope. -->

- [ ] `hosted-gate-witness` is green on this exact head SHA; no required check is absent or stale.
- [ ] Review threads are resolved; no required check or CODEOWNER control is bypassed.
- [ ] I will not self-merge while a required check is red, missing, stale, or attached elsewhere.
- [ ] Final merge SHA/default-branch result will be recorded; green PR head is not reused as proof.
- [ ] For a Phase 2 slice, the mandatory 12-section slice report will be written, then work STOPS
      pending operator `GO`; otherwise the applicable phase/release stop contract is stated.
