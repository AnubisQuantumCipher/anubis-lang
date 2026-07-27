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

import re
import sys
from pathlib import Path

AST = Path("compiler/src/frontend/mod.rs")
MID = Path("compiler/src/middle/mod.rs")


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


def check(fn: str) -> list[str]:
    ast, mid = AST.read_text(), MID.read_text()
    body = walker_body(mid, fn)
    variants = {
        **{f"Stmt::{k}": v for k, v in enum_variants(ast, "Stmt").items()},
        **{f"Expr::{k}": v for k, v in enum_variants(ast, "Expr").items()},
    }

    problems: list[str] = []
    for vname, fields in variants.items():
        code_fields = [f for f, t in fields if holds_code(t)]
        if not code_fields:
            continue
        # every arm in this walker that matches this variant
        pat = re.escape(vname) + r"\s*\{([^}]*)\}"
        arms = re.findall(pat, body)
        if not arms:
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
    walkers = sys.argv[1:] or ["body_has_mode_elevator"]
    all_problems: list[str] = []
    for w in walkers:
        p = check(w)
        all_problems += p
        print(f"{w}: {'OK' if not p else str(len(p)) + ' PROBLEM(S)'}")
        for x in p:
            print(f"  {x}")
    if all_problems:
        print(f"\nWALKER_COMPLETENESS: FAIL ({len(all_problems)})")
        return 1
    print("\nWALKER_COMPLETENESS: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
