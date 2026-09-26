# Review of the extension design

A separate reviewer approved the bounded extension design for implementation,
subject to fresh implementation review and the listed safety/precision tests.
It checked isolated helper stores/returns, exact direct-call arity, typed/generic
guard exclusions, statement `push` precedence, expression resolution, stable
tagged unary/comparison terms and shared work/cycle limits.

Required qualification: full typechecking runs
`infer_unannotated_struct_params` before IFC2, while IFC2-only analysis reads
the original AST. An originally unannotated helper can therefore satisfy admission
only in the latter path. Conservative exclusion is safe, but metadata must not
be described as identical to runtime annotations. Add a parity control and keep
any precision failure open.

Mutation/early-return tests using directly constructed block ASTs must be
distinguished from source tests using `if`, because branches remain unsupported
by this extension. Design approval is not compatibility or integration approval.

The earlier final-code review found no actionable soundness defect after its
continuation correction, but it had not established absence of acceptance
regressions. The later comparative measurements exposed those regressions and
the reviewer recommended withholding integration. Neither review performed builds
or runtime execution; lead observations are retained in the adjacent logs.
