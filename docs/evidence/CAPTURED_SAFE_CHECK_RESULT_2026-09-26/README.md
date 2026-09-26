# Captured Safe check result — scoped local receipt

Code commit `365da05dbd4637323f646f95ec204d571a7ddcb1` adds a typed
compiler-side check result for one captured compilation input. The frontend
summarizes mode intent from each module's parsed items **before** trait
desugaring can drop an unused declaration or an overridden default. The
captured resolver retains those source-keyed summaries beside the saved
source graph and refuses Research/Exploit intent before Safe-default
typechecking or lowering. The shared mode-elevator walk also covers
`requires` and `ensures` expressions and uses the parser's Research attribute
mapping. After typechecking, the compiler applies taint, checks the solver
stream, and lowers the same captured AST with the checked IR's
monomorphization inventory. This is a private compiler API; it produces Rust
source for comparison, not a native artifact or verified seal.

The [machine-readable gate receipt](gate-receipt.json) binds the code commit,
source file hashes, exact commands, and complete path-normalized logs. On the
final source, the captured-resolver tests passed with `26 passed; 0 failed`,
the focused evidence and frontend controls each passed, compiler Clippy with
`-D warnings` passed, and workspace format and diff checks passed. All Cargo
commands ran in a lead-owned memory-capped user scope with an isolated target
directory. Research and Exploit test sources were only parsed and classified;
none was typechecked, lowered, compiled, or executed by those controls.

The first captured-resolver run **failed**: its test used stacked
`@exploit(authorization: ...) @safe`, which this frontend cannot parse. The
complete [failed log](initial-captured-fail.log) is retained. The test now
uses parser-valid Research alias forms and a separate parser-valid
`@exploit fn` classification control. It does not count a syntax error as a
security refusal. The [independent final-diff review](INDEPENDENT_REVIEW.md)
approved that correction and the scoped compiler-side change; the reviewer
did not run the gates. The parser's parameterized/stacked `@exploit` spelling
remains an explicit language-surface limitation, not a proven Safe guard.

The public CLI does not consume this result yet. In particular, malformed
solver streams still create a synthetic wire `FAIL`; a future public status
adapter must classify that condition as a typed tool error, not a disproved
contract. The captured graph covers only the visited compilation closure;
it does not establish package or independent evidence admission. Staged
native output publication, source-to-executable correspondence, matched
imported contract twins, full workspace and matrix gates, hosted witnesses,
and disposable-guest seals remain unrun for this code commit.
