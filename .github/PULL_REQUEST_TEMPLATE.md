<!-- Anubis is evidence-native: a change lands with its proof, not a promise. -->

## What this changes

<!-- One paragraph. Scope it to a single idea. -->

## Evidence

<!-- Paste the passing gate output, or the exact commands + artifacts. -->

- [ ] `bash scripts/audit_a_plus.sh --out out/gate` is green (the 15-gate front door)
- [ ] `cargo test` passes (attach the count)
- [ ] `cargo fmt --check` and `cargo clippy` are clean

If this touches the **solver or the checker**:

- [ ] I ran an adversarial soundness hunt and found no new false accept
- [ ] New rejection power landed **shadow-first** (zero unexpected corpus rejections)

If this touches **`selfhost/`**:

- [ ] The self-host fixpoint was re-sealed and `EXPECTED_FIXPOINT_VM` re-baselined **deliberately** (with a logged reason)

## Boundary

<!-- If the feature is partial, state exactly what it proves and what it does NOT.
     A modeled lane must fail closed on everything outside it — never fabricate a
     discharge. Mark unfinished third-party commitments [NEEDS-HUMAN]. -->

## Checklist

- [ ] No claim in the code or docs is unbacked by a test, command, or artifact
- [ ] Docs/CHANGELOG updated if user-facing
