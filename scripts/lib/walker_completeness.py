#!/usr/bin/env python3
"""Assert a security-critical walker reaches every expression-holding AST field.

THE DEFECT CLASS THIS EXISTS FOR

A `..` in a match arm silently discards fields. In `body_has_mode_elevator` alone that has now
happened FOUR times, each shipping before it was found:

    Stmt::While    { cond, body, .. }     dropped `invariant: Vec<Expr>`
    Stmt::WhileLet { expr, body, .. }     dropped the scrutinee
    Stmt::For      { source, body, .. }   dropped the source
    Pattern::Struct { fields, .. }        dropped the struct NAME

The last one let a mode elevator hide inside a loop invariant — `@research` elevating a program out
of Safe mode with NO authorization, caught only because an unrelated check happened to fail first.

The project already uses total matches with no wildcard arm, and that is necessary and NOT
sufficient: an exhaustive match stops a new VARIANT. It does nothing about an arm that EXISTS and
ignores a field. This closes that second half.

HOW

Parse the `Expr` / `Stmt` enum definitions, find every field whose type mentions `Expr` or `Stmt`
(i.e. can hold code), then parse the walker's match arms and check each such field is either bound
by name or reachable via a bare-tuple pattern. A field that is neither is reported.

Deliberately a source-level check rather than a Rust lint: it needs the AST definition and the
walker body together, it must run without a nightly toolchain, and its failure message names the
exact field a new AST change forgot.

LIMITS, stated rather than implied. This proves a field is BOUND, not that it is correctly used —
an arm could bind `invariant` and ignore it. It is a floor, not a ceiling, and the fixtures remain
the real evidence.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

AST = Path("compiler/src/frontend/mod.rs")
# Overridable so the self-test can plant its defect in a SCRATCH COPY instead of the live file.
# The self-test used to `cp` mod.rs aside, mutate it in place, and restore it on exit — which
# silently destroys any concurrent writer's work if they save inside that window. This project has
# already lost an agent's in-progress work to exactly that shape once
# (docs/COMMIT_5259227_CORRECTION.md); a *test* is not worth risking it a second time.
MID = Path(os.environ.get("ANUBIS_WALKER_MID", "compiler/src/middle/mod.rs"))
# Sibling modules that own their own `walk_expr`. The census found 26 value-flow walkers; three of
# them live outside mod.rs, and the gate could not register them for the simple reason that it only
# ever read one file. A registry that cannot reach a walker is not a judgement about that walker.
SIBLINGS = [
    Path("compiler/src/middle/effects.rs"),
    Path("compiler/src/middle/capability.rs"),
    Path("compiler/src/middle/trifecta.rs"),
]


def enum_variants(src: str, enum: str) -> dict[str, list[tuple[str, str]]]:
    """{VariantName: [(field, type)]} for brace-struct variants of `enum`."""
    m = re.search(r"pub enum " + enum + r" \{(.*?)\n\}", src, re.S)
    if not m:
        return {}
    out: dict[str, list[tuple[str, str]]] = {}
    for vm in re.finditer(r"\n    (\w+) \{(.*?)\n    \},", m.group(1), re.S):
        fields = re.findall(r"\n\s+(?:pub )?(\w+): ([^,\n]+)", vm.group(2))
        out[vm.group(1)] = [(f, t.strip()) for f, t in fields]
    return out


def holds_code(ty: str) -> bool:
    """A field that can contain an expression or statement — i.e. somewhere code can hide."""
    return "Expr" in ty or "Stmt" in ty or "MatchArm" in ty or "ForSource" in ty


def source_for(fn: str) -> str:
    """The file that defines `fn` — MID first, then the sibling modules.

    A walker is identified by NAME, not by file, so the registry does not have to encode where a
    function happens to live today.
    """
    # Qualified form `module::fn` disambiguates. THREE sibling modules each define `walk_expr`,
    # so an unqualified lookup would silently pick whichever file happened to be searched first —
    # a gate quietly grading a different walker than the registry names. Refuse instead.
    if "::" in fn:
        modname, _, bare = fn.rpartition("::")
        for cand in [MID, *SIBLINGS]:
            if cand.is_file() and cand.stem == modname and f"fn {bare}(" in cand.read_text():
                return cand.read_text()
        raise SystemExit(f"walker `{fn}`: no module `{modname}` defining `fn {bare}(`")

    hits = [c for c in [MID, *SIBLINGS] if c.is_file() and f"fn {fn}(" in c.read_text()]
    if len(hits) > 1:
        names = ", ".join(f"{h.stem}::{fn}" for h in hits)
        raise SystemExit(
            f"walker `{fn}` is AMBIGUOUS across {len(hits)} modules — qualify it: {names}"
        )
    if not hits:
        raise SystemExit(f"walker `{fn}` not found in {MID} or {[str(x) for x in SIBLINGS]}")
    return hits[0].read_text()


def walker_body(src: str, fn: str) -> str:
    i = src.find(f"fn {fn}(")
    if i < 0:
        raise SystemExit(f"walker `{fn}` not found")
    depth, j, started = 0, i, False
    while j < len(src):
        if src[j] == "{":
            depth += 1
            started = True
        elif src[j] == "}":
            depth -= 1
            if started and depth == 0:
                return src[i : j + 1]
        j += 1
    raise SystemExit(f"could not delimit `{fn}`")


def check(fn: str, scope: str = "all") -> list[str]:
    """`scope` selects which enums this walker is RESPONSIBLE for.

    Registering a second walker used to be impossible, and this is why: the check demanded every
    walker bind every code-holding field of BOTH `Stmt` and `Expr`. An expression-only query like
    `expr_taint_source_m` does not walk statements — that is its caller's job — so it scored 11
    `Stmt::* is never matched` problems that are not defects, drowning the one that was
    (`Expr::If never binds cond`). A gate whose output is mostly false positives gets one walker
    registered and then abandoned, which is exactly what happened.

    Scope is a claim about the walker's contract, not a way to silence it: `expr` still demands
    TOTAL coverage of every code-holding `Expr` field.
    """
    ast = AST.read_text()
    # `source_for` resolves a qualified `module::fn`; the body lookup needs the BARE name.
    body = walker_body(source_for(fn), fn.rpartition('::')[2])
    variants: dict = {}
    base = scope[len("partial-"):] if scope.startswith("partial-") else scope
    if base in ("all", "stmt"):
        variants.update({f"Stmt::{k}": v for k, v in enum_variants(ast, "Stmt").items()})
    if base in ("all", "expr"):
        variants.update({f"Expr::{k}": v for k, v in enum_variants(ast, "Expr").items()})
    if not variants:
        raise SystemExit(
            f"unknown scope `{scope}` for `{fn}` (use all|expr|stmt, optionally partial- prefixed)"
        )
    # `partial-` = the contract for a SPECIALISED walker: it need not match every variant, but
    # every variant it DOES match must bind all that variant's code-holding fields.
    #
    # This exists because of a defect found 2026-07-28 in the fix for another defect.
    # `stmt_value_secret` — the helper written to extract a block's value — matched
    # `Stmt::If { then, else_, .. }` and threw `cond` away, so a secret CONDITION selecting between
    # two clean constants stayed invisible. That is the same `..` shape that had just been fixed in
    # `Expr::If`, reproduced inside its own repair.
    #
    # A total-coverage demand cannot express that walker's contract: it deliberately handles only
    # the last statement, so it scored ten "never matched" non-defects and the one real finding was
    # unreachable. `partial-` says the thing that is actually true of it — match what you like, but
    # do not half-read what you matched.
    partial = scope.startswith("partial-")

    problems: list[str] = []
    for vname, fields in variants.items():
        code_fields = [f for f, t in fields if holds_code(t)]
        if not code_fields:
            continue
        # every arm in this walker that matches this variant
        pat = re.escape(vname) + r"\s*\{([^}]*)\}"
        arms = re.findall(pat, body)
        if not arms:
            if partial:
                # A specialised walker is allowed not to match a variant; it is not allowed to
                # match one and read it partially. Skip, do not report.
                continue
            # Variant never matched by name. Only safe if a catch-all covers it, and these walkers
            # are deliberately total — so an unmatched code-holding variant is itself the finding.
            problems.append(f"{fn}: {vname} is never matched (holds {code_fields})")
            continue
        # An arm whose entire body is the literal `true` has ALREADY answered the walker's
        # question — `Stmt::ResearchBlock { .. } => true` has found the elevator, so descending
        # into its body is pointless, not an omission. Deliberately narrow: only a bare `true`
        # qualifies. Anything else (a call, a variable, a conjunction) could depend on a field it
        # never bound, which is exactly the defect being hunted.
        terminal = re.findall(re.escape(vname) + r"\s*\{[^}]*\}[^=]*=>\s*true\s*,", body)
        if terminal:
            continue

        # a field is OK if ANY arm binds it (arms may specialise)
        for f in code_fields:
            bound = any(re.search(r"\b" + re.escape(f) + r"\b", a) for a in arms)
            if not bound:
                problems.append(
                    f"{fn}: {vname} never binds `{f}` — code can hide there "
                    f"(a `..` is discarding it)"
                )
    return problems


def main() -> int:
    # Each argument is `name` or `name:scope` (scope = all|expr|stmt, default all).
    walkers = sys.argv[1:] or ["body_has_mode_elevator"]
    all_problems: list[str] = []
    for spec in walkers:
        # Split on a trailing SCOPE only. `partition(":")` split inside the `::` of a qualified
        # name like `effects::walk_expr`, silently turning the module into the walker name.
        SCOPES = {"all", "expr", "stmt", "partial-expr", "partial-stmt", "partial-all"}
        w, scope = spec, "all"
        if ":" in spec:
            head, _, tail = spec.rpartition(":")
            if tail in SCOPES and head:
                w, scope = head, tail
        p = check(w, scope)
        all_problems += p
        label = w if scope == "all" else f"{w} [{scope}]"
        print(f"{label}: {'OK' if not p else str(len(p)) + ' PROBLEM(S)'}")
        for x in p:
            print(f"  {x}")
    if all_problems:
        print(f"\nWALKER_COMPLETENESS: FAIL ({len(all_problems)})")
        return 1
    print("\nWALKER_COMPLETENESS: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
