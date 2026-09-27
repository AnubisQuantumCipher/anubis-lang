# Independent D9 fixture registration review

Decision: **GO for registration of the named fixture sources and their appended registry rows.** This is not approval of a production direct-field implementation, a closed D9 defect, a full matrix run or a runtime witness.

The registered files match both the frozen design sources and baseline-check copies byte for byte. Their intended outcomes match the original design manifest. Each ID occurs uniquely in `tests/soundness/matrix/registry.tsv`, under `D9-QUALIFIER-PRECISION`, `shared`, `carrier`; the selected rows form the registry tail. The exact observed registry SHA-256 is `b56321497a6ef58339b0ce6911824399d8b0ebf7625d6bdd170e79aca320b03d`. This review did not compare unrelated prior rows to a git revision; the lead must stage only the reviewed fixture additions and registry append.

| Registered case | Source SHA-256 | Required intent |
|---|---|---|
| `d9q_loop_carried_secret_guard_egress_invalid` | `e9b7fe1ac325692ab0c178e3ba938d33e157641cd0dfa9e46d3c554adf0b0afc` | REJECT |
| `d9q_loop_carried_public_guard_egress_valid` | `0604d7738a848d0b69725454fb73df234dfed9946e28fb9aafcf4283c6a0a91f` | ACCEPT |
| `d9q_literal_false_loop_under_secret_pc_egress_valid` | `3aa16c607df7bd061438c92424b0d6492d7d23bff269185731025b3f379383ec` | ACCEPT |
| `d9q_secret_field_write_public_projection_valid` | `f450c8a02d976e4cacc1966c38a145575497a4c030c2efb708ce552b06cdae57` | ACCEPT |

## Independent semantic assessment

The secret-loop negative assigns public `Row.n` under a loop condition whose subsequent value depends on protected input. The loop's recurrence changes the eventual public projection with that input; a direct writer-attributed control-flow finding is required. Merely retaining whole-Row contamination or a separate IFC2 refusal does not demonstrate the repaired writer mechanism.

The public-loop twin carries only public `public_choice` into the guard and public `Row.n` update. Its protected `Row.k` sibling is never projected to the sink. Its main varies the public choice alongside the secret argument, so a future printed trace alone would not demonstrate noninterference. A separate runtime comparison should hold public inputs fixed while varying protected inputs; preserve this frozen acceptance control unchanged.

The literal-false loop has no reached field write. Its enclosing secret branch does not change the later unconditional public projection. The secret-field-write control mutates only protected `Row.k`; it does not alter `Row.n` or condition the later egress. Their ACCEPT oracles follow the selected field/value semantics and stated timing/termination boundary; they are not permission to suppress reached secret-PC writes to public fields.

This assessment uses the language's assignable field places and ordinary loop/short-circuit semantics (`LANGUAGE.md:94`, `:144`, `:150`), value semantics (`:205`), field-sensitive declared labels and constant-condition behavior (`docs/mission/IFC_V2.md:31`, `:66`, `:85`), and the explicit confidentiality scope (`docs/mission/DELEGATED_DECISIONS_2026-09-25.md:10`). These selected sources contain no declassification, capability expansion or weakened contract. No execution was used to establish this review's semantic reasoning.

## Baseline identity and recorded outcomes

The baseline result digest is `32562aa426756b04cf84f5f7c9d7ac38967a60c997a871cdda47639935f384cd`. Its build manifest matches retained bytes with digest `731a490efea318d27508ec5430afeaa4f065f61112c1b15e490f7dfb663d902c`, identifies code commit `2683669c2ae87ceea66d19ebb0e6995578c289eb`, and records successful build output. The retained immutable CLI bytes match full digest `d5b452c21c2a92d39199a8e93b2a5e9d1fc642fbbda493f45ef88a871448bcfd`. The baseline harness bytes match `7b8ff8a8ba24e5d18fbb11945d3f1b01d6eee049b3fce15a2a670916706f6fc9`.

I read the harness and independently parsed the retained diagnostic JSON streams without invoking the harness or compiler. Every stream's typed projection agrees with the recorded summary, has no parser errors, and reports exit `1`, summary `fail`, and `ANUBIS_SECRET_EXFILTRATION` with status `refused`. The invalid secret-loop source also has the explicitly prefixed IFC2 exfiltration finding. The valid controls have only the generic whole-Row exfiltration finding. No stream contains a writer-located direct `ANUBIS_IMPLICIT_FLOW` finding.

Accordingly, the negative is already refused in aggregate but lacks the required direct writer evidence. The ACCEPT controls remain precision defects. These are typed security refusals, not syntax failures, checked contract disproofs, silent accepts or evidence of a completed fix. The full compiler build provenance is retained by the lead; this read-only review checks its supplied manifest and binary identity without independently attesting git ancestry or rebuilding the source closure.

`FIXTURE_REVIEW_CHECKS.json` retains exact source, registry, binary, harness and record comparisons. No git command, build, CLI check, native execution, shared-source edit or external write was performed. Next acceptance requires a reviewed production patch, source-bound candidate comparisons preserving the invalid writer finding and IFC2 enforcement, and acceptance of the valid controls for their original semantics.
