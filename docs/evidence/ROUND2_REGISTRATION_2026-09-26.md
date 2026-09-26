# Round-2 leak registration and match-arm precondition witness

Source commit: `2ab0f01f9d738b10faa6df64aed8ba1f156f24cc`. This is a matrix-source and
registry change, not a compiler correction or a full-matrix pass.

The [recovered round-2 inventory](IFC2_ROUND2_2026-09-26/REPORT.md) contains 41
historically confirmed leak findings. `r2p_13` was already registered as
`ifc2r2_proof_bool_method_fallback`. This commit adds the other 40 carrier cases,
including both `min_by`/`max_by` findings, plus eight archived direct comparisons.
Each new carrier was byte-compared with the inventory's `source_sha256`; each direct
comparison was byte-compared with its archived source. An independent read-only
review found no missing source, duplicate ID, or private machine path in the added
files. The historical `REJECT` intents have not been changed to match a current
compiler result. The original inventory remains the provenance for historical
runtime observations and isolation caveats.

The same commit adds the `M-ARM-BINDER` negative cases and acceptance controls.
A sibling match arm binding `x` causes the statement-match walker to remove the
violated `f(x)` precondition in another arm. The stronger constant-call case shows
that an unrelated `x` in the obligation's assumptions can also trigger the drop.
These are source-level `requires(x > 0)` violations; no Anubis program was run.

| Focused `check` source | Pinned CLI result |
|---|---|
| `ifc2r2_minmax_callback_argument` | exit 0, accepted |
| `ifc2r2_minmax_callback_args_decided_by_results` | exit 0, accepted |
| `r2arm_sibling_binder_drops_requires` | exit 0, accepted |
| `r2arm_sibling_binder_drops_requires.direct` | exit 1, `ANUBIS_ASSERTION_DISPROVED` |
| `r2arm_unrelated_assumption_drops_requires` | exit 0, accepted |
| `r2arm_unrelated_assumption_drops_requires.direct` | exit 1, `ANUBIS_ASSERTION_DISPROVED` |
| `r2arm_renamed_binder_preserves_requires` | exit 1, `ANUBIS_ASSERTION_DISPROVED` |
| `r2arm_satisfied_requires_remains_valid` | exit 0, accepted |

The checker was immutable `anubis-proof-bool-c91445cdb04faab8`, SHA-256
`c91445cdb04faab827925289a1c426ac358d4491ae6732f63d1bfb752a4713e7`.
Comparison of commit `b835b94b` to this registration commit found no changes in
`compiler`, `solver`, `tools/anubis`, `Cargo.lock`, `rust-toolchain.toml`, or `vendor`;
that comparison is a source-scope check, not a claim about every build input.
Every listed result is from `anubis check` with a bounded local timeout. No native
execution, guest replay, or full matrix was run for this receipt.

Static registry validation found 2369 unique IDs, no missing form source, and no
malformed required fields after registration. The corrected docs-drift gate passed
with 37 stamps and zero drift under its documented floor; the earlier floor change from 53
is under separate audit and this pass does not validate the removed stamps.
The matrix runner currently accepts several rejection classes for a `REJECT` intent,
so future closure requires case-specific diagnostics and valid twins. Trap, crash,
device, and exploit-kit witnesses retain their guest-only runtime requirement.
Current verdicts for the other round-2 cases remain unmeasured.
