# IFC2 first-class zero-argument `exit` — local source and checker receipt

Status: **scoped code-generation repair; native parity EXTERNAL / NOT RUN**.

The registered fixture commit is `feef1586c07ccc3f2fdd8ebc30fff7695321bc9b`.
The implementation commit is `c3bf72e2b49f697fa9462dbf0482c71f2174264f`.
This receipt follows the earlier
[terminal checker receipt](IFC2_TERMINAL_EGRESS_LOCAL_2026-09-27.md)
without changing its historical evidence.

Direct `exit()` is documented and lowered as a no-op. Before this code unit,
using builtin `exit` as a first-class value produced a closure with a status
argument arm but no zero-argument arm. A zero-argument alias therefore reached
the generated `ANUBIS_ARITY` fallback according to source inspection, although
the saved Safe checkers accepted that program. The implementation now derives
the alias's zero-argument arm from the direct builtin lowerer
(`compiler/src/backends/run.rs::var_as_value`). It keeps the status-argument
arm and invalid-arity fallback. The structural Rust test also verifies that
the lowered alias program calls the closure with an empty argument vector.
No Anubis program was executed to confirm the predicted pre-fix trap or
post-fix native behavior.

The matrix now registers alias calls under public and secret control, including
both taken and untaken secret branches. A direct `exit()` case with the same
taken secret guard is the parity control. For the taken alias/direct pair, the
frozen intended behavior is exit status `0`, stdout `continued` followed by a
newline, and empty stderr. The registry retains `ACCEPT` as the checker intent;
it does not treat checker acceptance as native parity evidence. An authorized
disposable-guest run must compare the exact paired native behavior under pinned
source, compiler, and target identities before this parity issue can close.

The private retained evidence bundle is named `anubis-ifc2-terminal-20260927`.
The paths below are relative to that bundle; raw logs and binaries are not
copied into the repository. A digest identifies retained bytes and is not an
independent proof of source correspondence.

| Bundle-relative artifact | SHA-256 |
|---|---|
| `alias-zero-proposal/source-manifest-before-build.json` and `alias-zero-proposal/source-manifest-after-build.json` | `9b063d8b2eadf350bfeb893d7f4b495eb99b52a000e715ecd60d16b1cbafdf44` |
| `alias-zero-proposal/source-bound-build-result.json` | `7cadb9e20cf962c100d348b1a3e94ddafdd7be2d4353500b9326fe604f7ad1a4` |
| `pins/anubis-c3bf72e2b49f697fa9462dbf0482c71f2174264f` | `d0bfda764576c6c996290e3360b4e53c602f29cab8ac8d48009cef3d25245daa` |
| `alias-zero-proposal/targeted-unit-final.log` | `782213e8d49b13a2371e9bf7f9e3b01d336d9a0a3e2307195aee9363a77f9fbd` |
| `alias-zero-proposal/fmt-final-result.json` | `4b3841e60583e004082d431a258d6b12113bf25159e8e667e5fa4f57609c69b9` |
| `alias-zero-proposal/run_format_check.py` | `d0279bdb929ecb64ec01639679b2646d6c07cee140785478df0df5075c7e51e0` |
| `alias-zero-proposal/clippy-all-targets.log` | `1f4991003ddb7e08936e4a63db36eaaff27ad155904b49cb91f8753f3a3e994c` |
| `alias-zero-proposal/registered-checker-prepatch-20260927/receipt.json` | `7d14f83385d200f36c661c744b3aa344d6dfaced7bee99cf1efd1377a5dd07f6` |
| `alias-zero-proposal/registered-checker-postpatch-20260927/receipt.json` | `3734ce0ad0d9b1e69629ad916d4d6d7bb0602434b3318191ae6c7665adca2501` |
| `alias-zero-proposal/registered-checker-comparison.json` | `e4ea491c04ba55c60d85c715ed0354591df68bff2d6a6eee31e417eb6c72bbf9` |
| `alias-zero-proposal/final-terminal-controls-20260927/receipt.json` | `b2a5e192b1c28e9d5f7309a26df9a376177a4c753ce3c159e4b77dd474d3f16a` |
| `alias-zero-proposal/run_registered_checker.py` | `3b01f1cad4129ece3c4d76a028b0d3c001ea0a0ea2c2d1f001c4b2ac4a42417e` |
| `alias-zero-proposal/compare_registered_checker.py` | `3298058b0fc2e565d6c866c38e8e5d701ccffa69210318a62b4a93961945aa28` |
| `alias-zero-proposal/run_final_terminal_controls.py` | `309be7ac759d85b3f6df8ae5f2884fdce9b93ee74dc44af79b6017d8bd38573a` |
| `alias-zero-proposal/ALIAS_ZERO_ADVERSARIAL_READONLY_REVIEW_2026-09-27.md` | `bebc3a4f290a8810e6c600d07532860fe5457458bfd41544799e30cc932a6629` |
| `alias-zero-proposal/ALIAS_ZERO_SOURCE_BOUND_EVIDENCE_AUDIT_2026-09-27.md` | `b57ec4ae46f5dd1565ed6b78c2ff739f37a3d5dcb9b9b7bdd2ccff5a6e2e204b` |
| `alias-zero-proposal/docs-drift-code/docs_drift_report.txt` | `f547d39c096a1cf30472d5230fe3a681324af3ce007d2f65511739773e7b3218` |

The bounded offline release build exited successfully with identical
before/after source manifests and a clean tracked worktree. The new read-only
checker pin matches its build output; the previous pin remained byte-identical.
The source manifest includes pre-existing ignored generated inventory, so this
is a stable local source-bound build, not a tracked-only clean-room witness.
The focused Rust code-generation test passed; `cargo fmt --all -- --check`,
strict all-targets Clippy, and the documentation-drift gate passed. At the
final implementation bytes, the source manifest matched after the focused
test, Clippy, build, and postpatch accepted-case checker probe. Independent
source review found no blocking issue in this narrow arm,
while explicitly withholding native parity closure.

The registered checker comparison reports `FOCUSED_MATCH`: both old and new
pins accepted every selected alias/direct control without diagnostics. That
outcome protects checker acceptance and confirms no checker verdict change; it
cannot detect a native closure trap. The checker probes ran with output confined
to the private Work bundle and did not execute an Anubis program.

An independent evidence audit found that the accepted-case comparison alone
did not repeat a negative IFC control on the final pin. A separate focused
Safe-check run then compared the previous and final pins on secret-controlled
status-argument `exit` and `panic` aliases, tainted `exit(status)`, and valid
public/user-defined/local-shadow controls. Both pins produced the registry's
exact typed refusals and valid acceptances, reported as `FOCUSED_MATCH`.
That receipt binds source bytes before and after each check; it remains a
selected checker result, not a native or full-matrix result.

The full soundness matrix is not a compliant local Linux-VM witness because
its crash-capable cases require the repository's disposable-guest lane. Native
alias parity, named proof-journal program-counter enforcement, file-device
egress, the final workspace suite, hosted CI, formal correspondence,
self-hosting, and release acceptance remain open. Neither the product finish
line nor the full trust-chain finish line is claimed.
