# Solver model inventory — test-only integration

Code commit `da19222aed7695e6eaa3d5f4cab6d05933e27d77` adds a bounded
declaration/model inventory behind `cfg(test)`. It checks symbol identity,
declared sort and model arity, duplicate/missing/extra entries, command framing,
and the reference closure of supported array helpers. It retains raw model
bodies. The production replay function does not call this module.

The [verification record](verification.json) binds the source files, applied
patch, toolchain, commands' exit codes and original/published log digests.
The [independent final-diff review](INDEPENDENT_REVIEW.md) approves only this
test-only integration. Final focused tests reported **8 passed**, and final
format and strict compiler all-targets Clippy checks exited successfully.
These are local focused witnesses, not a whole-workspace or release result.

Earlier failures are retained alongside the final logs. A synthetic bare
`as-array absent` control expected a missing-helper error even though the
bounded grammar rejects that spelling first. The corrected controls retain
that rejection and separately test an admitted quoted missing helper.
Formatting and equivalent-branch/literal Clippy corrections followed observed
failures; no lint suppression or gate change was used. Published logs replace
the recorded machine-local path prefixes, terminal formatting controls and
trailing whitespace;
the record retains both original and published digests.

Inventory success does not validate a value, pin a witness, authenticate the
solver, prove the encoded formula matches source, or establish a disproof.
Some synthetic inventory inputs deliberately contain ill-sorted bodies for a
later semantic checker. Alternate quoted/bare formal spelling and warning
normalization remain integration dependencies. Resource bounds are present,
but no hostile-input robustness claim follows from these tests.

The next code unit must check and pin every declared BV, floating-point,
string and array value, reject missing or free values, and propagate unsupported
replay separately from a checked mismatch. A-EVID-1 remains open until that
production path, supported-model positives and tampering controls are reviewed
and verified on source-bound artifacts.
