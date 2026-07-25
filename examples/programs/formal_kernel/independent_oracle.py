#!/usr/bin/env python3
"""Independent brute-force oracle for the small CNFs used by formal_kernel.

Does NOT use Anubis. Used to cross-check SAT/UNSAT labels.
"""
from __future__ import annotations

import itertools
from typing import List

Lit = int
Clause = List[Lit]
CNF = List[Clause]


def sat(cnf: CNF, nvars: int) -> bool:
    for bits in itertools.product([False, True], repeat=nvars):
        assign = {i + 1: bits[i] for i in range(nvars)}
        ok = True
        for cl in cnf:
            if not cl:  # empty clause
                ok = False
                break
            if any((lit > 0 and assign[lit]) or (lit < 0 and not assign[-lit]) for lit in cl):
                continue
            ok = False
            break
        if ok:
            return True
    return False


def main() -> None:
    cases = {
        "empty_cnf": ([], 1, True),
        "empty_clause": ([[]], 1, False),
        "unit_conflict": ([[1], [-1]], 1, False),
        "square": ([[1, 2], [-1, 2], [1, -2], [-1, -2]], 2, False),
        "chain": ([[1, 2], [-1, 3], [-2, -3], [1]], 3, True),
        "horn": ([[-1, 2], [-2, 3], [1]], 3, True),
        "one_hot": ([[1, 2, 3], [-1, -2], [-1, -3], [-2, -3]], 3, True),
        "three_neg": ([[1, 2, 3], [-1], [-2], [-3]], 3, False),
        "unit_chain4": ([[-1, 2], [-2, 3], [-3, 4], [1]], 4, True),
        "taut": ([[1, -1]], 1, True),
        "pure": ([[1, 2], [1, -2]], 2, True),
        "sat4": ([[1, 2], [-2, 3], [-3, 4], [-1, -4, 3]], 4, True),
    }
    failed = 0
    for name, (cnf, n, expect) in cases.items():
        got = sat(cnf, n)
        status = "PASS" if got == expect else "FAIL"
        if got != expect:
            failed += 1
        print(f"{status:4}  {name:14}  oracle={got} expect_sat={expect}")
    print(f"oracle_summary failed={failed}/{len(cases)}")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
