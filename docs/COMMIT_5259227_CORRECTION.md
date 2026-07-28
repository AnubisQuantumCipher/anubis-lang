# Correction: commit `5259227` contains two authors' work, not one

**What the message says:** it describes only the lead's fix to the non-literal-index place
assignment — the one where wiping a container's tag paths to `Unknown` deleted evidence instead of
widening it.

**What the commit actually contains:** that fix, *plus* the auditor's round-11 implementation of six
container/tag parity rows, which landed in `compiler/src/middle/mod.rs` in the window between the
lead's `git status` check (clean) and its `git add`. The auditor was authorized to write to that
file for exactly this work, so the code belongs in the tree — it was the attribution that was wrong,
not the content.

Identifiable in the diff by:

| symbol | matrix row |
|---|---|
| `builtin_gate_tags_carried_by_value_d` | rows 6–8, the single conservative pass-through rule |
| `collect_pattern_binding_paths` | row 2, pattern-destructuring bind |
| `returned_container_keeps_exact_paths` (test) | row 3, container returned from a function |
| `scan_applied_param_stmts` / `scan_applied_param_expr` | row 4, container passed as an argument |
| `BuiltinGateTags::union_concrete` | shared helper |

**The verification is sound even though the attribution was not.** Every number quoted in
`5259227` — security 311/311, language 244/244, compiler lib 736/736, the adversary's a/c/g battery
— was measured AFTER both sets of edits were already in the working tree, on the binary built from
that combined state. Nothing was graded against a tree that differed from what was committed. That
is the property the lead's own rule exists to protect, and it held here by accident rather than by
care.

**Not amended, deliberately.** The branch auto-pushes on commit and force-pushing is forbidden in
this repo, so rewriting the message would mean rewriting pushed history. A correction that adds
information beats a rewrite that destroys it.

**The process lesson, which is the reusable part.** A clean `git status` is a statement about one
instant, not a lock. With agents authorized to write to the same file concurrently, `git add <path>`
stages whatever is on disk *at add time*, not what was inspected. Either stage a reviewed diff
(`git add -p`, or `git diff` immediately before `git add` on the same line), or do not authorize a
concurrent writer to a file the lead is about to commit. The lead did the second thing wrong: it
told the auditor `middle/mod.rs` was write-allowed and then committed that file itself in the same
round.
