# Independent static review: binding inventory v1

**Verdict: HOLD.** Frozen patch SHA-256:
`5b9f04280a03c3a50f226f781c410b410bd7952396f1a3c9d4f6c83de08cd13f`.
The integration base `compiler/src/middle/mod.rs` was SHA-256
`6abfc5e3359adb535a42a64a84de78b746d5df93ffea81ca4a9aa9ed0f4a981c`.
The review examined the scratch patch and candidate Rust source against the source-matched
frontend and registered fixture files in the lead checkout. It made no source edits and ran no
git, Rust build, Clippy, tests, Anubis parser/check/run, or Safe corpus grading. Type and test
claims below are therefore static assessments, not executed results.

## Findings requiring revision

1. **An empty Or pattern silently omits enclosed code.**
   `candidate/compiler/src/middle/binding_sites.rs:397-424` invokes its body callback once per
   alternative and never invokes it for `Pattern::Or(vec![])`. A constructed `Expr::Match` then
   loses both guard and body uses (`:474-493`); a constructed `Expr::IfLet` loses the then branch
   (`:544-557`). The parser's `parse_pattern` seeds a nonempty alternative list
   (`compiler/src/frontend/mod.rs:2335-2356` in the source-matched checkout), so this is a
   malformed-AST boundary, but the public AST can represent it. Mark that pattern site opaque or
   invalid and still traverse the enclosed expressions in the old lexical scope. Test both forms.

2. **The context field is written without a production read.**
   `candidate/compiler/src/middle/mod.rs:4834` adds `binding_inventory`, which is assigned at
   `:7894-7897`; no other use exists in the candidate. The new tests call the pure builder
   directly, so they cannot detect removal or breakage of the `analyze_function` hook. An unread
   private field also risks the repository's denied-warning Clippy gate
   (`scripts/audit_unified.sh:184-188`, `cargo clippy --all-targets -- -D warnings`). A meaningful
   read-only observation path through the real analysis entry point is needed before this can be
   called a production-linked inventory. Clippy was not run.

3. **The required lexical controls are incomplete.**
   Tests at `binding_sites.rs:624-740` cover simple shadowing, a second builder visit, an early
   lambda capture, and registered `r2arm` fixtures. They do not exercise the registered D9 shadow
   and deferred-lambda twins, a binder in a value-position block, nested destructuring, if-let,
   or an Or alternative without an outer binding. The registered files are present in the
   source-matched checkout; add direct inventory assertions over their identities and the missing
   language forms.

4. **Future AST field coverage is unprotected.**
   This is a new independent walker, and it is not in the registered walker list in
   `scripts/run_walker_completeness_gate.sh:33-74`. Its arms use `..` around fields that may later
   hold code, including `binding_sites.rs:251,255,372,377,474,496,544`. Enum-variant
   exhaustiveness alone cannot detect a newly added expression field on an existing variant.
   Bind every current field explicitly and register this walker in the static completeness check
   when that checker can inspect the new module.

## Static checks that passed inspection

The new match arms have plausible types against the frozen `Expr`, `Stmt`, `Pattern`, and
`ForSource` definitions. Structural paths distinguish declarations, and `declare` reuses an ID
when the same path is visited again (`binding_sites.rs:172-194`). Let initializers and loop
headers are visited before their binders (`:249-318`). Or alternatives retain separate lexical
scopes and union uses at a shared source site (`:335-424`). Lambda capture candidates come from
the definition scope (`:197-225,522-543`), and direct calls prefer known user functions
(`:197-205`), matching the existing runtime dispatch rule. The patch only adds an observational
module and assignment; no Safe verdict branch is directly changed.

An unconsumed inventory must never be interpreted as a closed identity certificate. In
particular, `opaque_sites`, unresolved uses, and multiple candidates need explicit treatment by
any later place/effect consumer. Resource cost and denied-warning behavior remain unmeasured.
