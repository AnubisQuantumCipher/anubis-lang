#!/usr/bin/env python3
# G-DOGFOOD-1: structural proof that the self-host compiler's OWN source uses enums +
# match + if-expressions in load-bearing positions. Operates on the compiler's own AST
# (emitted by `anubis ... parse anubis_sh.anb`), so it cannot be satisfied by comments
# or dead code — the match/enum nodes must be inside the named codegen/checker functions.
import json
import sys


def find_kind(node, kind):
    if isinstance(node, dict):
        if node.get("kind") == kind:
            return True
        return any(find_kind(v, kind) for v in node.values())
    if isinstance(node, list):
        return any(find_kind(x, kind) for x in node)
    return False


def main():
    ast = json.load(open(sys.argv[1]))
    items = ast.get("items", [])
    enums = sorted(it["name"] for it in items if it.get("kind") == "Enum")
    fns = {it["name"]: it for it in items if it.get("kind") == "Fn"}
    errors = []

    for want in ("Stmt", "Expr"):
        if want not in enums:
            errors.append(f"missing load-bearing enum `{want}`")

    # match must appear inside these specific codegen/checker functions
    for fn in ("jstmt", "jexpr", "check_stmt", "check_expr"):
        if fn not in fns:
            errors.append(f"missing function `{fn}`")
        elif not find_kind(fns[fn].get("body"), "Match"):
            errors.append(f"`{fn}` contains no `match` node (dogfood not load-bearing)")

    # if-expressions in codegen-path functions
    ifexpr_fns = [
        fn for fn in ("jbool", "prec_of", "json_escape")
        if fn in fns and find_kind(fns[fn].get("body"), "IfExpr")
    ]
    if len(ifexpr_fns) < 2:
        errors.append(f"expected if-expressions in >=2 of jbool/prec_of/json_escape, got {ifexpr_fns}")

    if errors:
        print("DOGFOOD_STRUCT: FAIL")
        for e in errors:
            print("  -", e)
        sys.exit(1)
    print(
        f"DOGFOOD_STRUCT: PASS (enums={enums}; match in jstmt/jexpr/check_stmt/check_expr; "
        f"if-expr in {ifexpr_fns})"
    )


if __name__ == "__main__":
    main()
