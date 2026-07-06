# Minimal Stdlib Core (Gate 2/3)

Classifications for this slice only.

- print : PLANNED (use for future)
- sink : REAL (recognized in taint/safe enforcement)
- taint_source(label) : REAL
- declassify(v [, policy, reason]) : REAL (policy+reason required in safe)
- symbolic(name) : REAL
- assume(e) : REAL
- assert(e) : REAL
- hash_sha256 : PLANNED
- len : PLANNED (string/array)

All builtins lower to special Expr forms and are captured in evidence.
