#!/usr/bin/env python3
# G-DOGFOOD-3 helper: mechanically neuter one load-bearing match arm in a COPY of the
# compiler source. The gate then rebuilds and asserts the self-build now fails — proving
# the enum/match construct carries the output (removal breaks it → not cosmetic).
#
# Usage: dogfood_ablate.py <src.anb> <out.anb> <target: var|let>
import sys

ABLATIONS = {
    # jexpr's Expr::Var arm: emit a constant broken variable name for every variable
    # reference. Any program that reads a variable then produces wrong output.
    "var": (
        'Expr::Var(nm) => "{\\"kind\\":\\"Var\\",\\"name\\":" + q(nm) + "}",',
        'Expr::Var(nm) => "{\\"kind\\":\\"Var\\",\\"name\\":\\"__ABLATED__\\"}",',
    ),
    # jstmt's Stmt::Let arm: emit a broken statement kind so `let` bindings never bind.
    "let": (
        'Stmt::Let { name: nm, ty: t, init: ini } =>\n            "{\\"kind\\":\\"Let\\",',
        'Stmt::Let { name: nm, ty: t, init: ini } =>\n            "{\\"kind\\":\\"LetABLATED\\",',
    ),
}


def main():
    src_path, out_path, target = sys.argv[1], sys.argv[2], sys.argv[3]
    src = open(src_path).read()
    old, new = ABLATIONS[target]
    if old not in src:
        print(f"ABLATE_ERROR: target `{target}` pattern not found in source", file=sys.stderr)
        sys.exit(2)
    src = src.replace(old, new, 1)
    open(out_path, "w").write(src)
    print(f"ablated `{target}`")


if __name__ == "__main__":
    main()
