#!/usr/bin/env python3
"""Re-check an Anubis evidence bundle's refutations WITHOUT the Anubis binary.

The solver already verifies every RUP step in-process before it will return `Unsat` — that is the
fail-closed rule. But an evidence bundle that keeps only the verdict asks an auditor to trust the
component under audit. This script closes that gap: it reads the DIMACS CNF and the DRAT refutation
the bundle publishes and re-derives the proof from scratch, in a different language, sharing no code
with the solver.

It implements ONE thing, deliberately: reverse unit propagation. For each derived clause C, assume
every literal of C is false, unit-propagate over the accumulated clause set, and require a conflict.
That is what makes C a logical consequence. The final clause must be empty — the refutation.

Usage:
    python3 scripts/verify_proof_bundle.py <bundle-dir>
    python3 scripts/verify_proof_bundle.py --self-test

Exit codes: 0 all published refutations verified - 1 a refutation failed - 2 usage/IO error.
"""

from __future__ import annotations

import json
import os
import sys


def parse_dimacs(path):
    """Return (num_vars, [clause, ...]); a clause is a list of nonzero ints."""
    clauses, num_vars = [], 0
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line[0] == "c":
                continue
            if line[0] == "p":
                parts = line.split()
                num_vars = int(parts[2])
                continue
            lits = [int(t) for t in line.split()]
            if lits and lits[-1] == 0:
                lits.pop()
            clauses.append(lits)
    return num_vars, clauses


def parse_drat(path):
    """Return [clause, ...] in derivation order. Deletion lines ('d ...') are skipped."""
    steps = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line[0] == "c":
                continue
            if line[0] == "d":
                continue
            lits = [int(t) for t in line.split()]
            if lits and lits[-1] == 0:
                lits.pop()
            steps.append(lits)
    return steps


def propagate_conflicts(clauses, assign):
    """Unit-propagate `clauses` under `assign` (var -> bool). True iff a conflict is derived."""
    changed = True
    while changed:
        changed = False
        for cl in clauses:
            unassigned, satisfied = [], False
            for lit in cl:
                v, want = abs(lit), lit > 0
                cur = assign.get(v)
                if cur is None:
                    unassigned.append(lit)
                elif cur == want:
                    satisfied = True
                    break
            if satisfied:
                continue
            if not unassigned:
                return True  # every literal false -> conflict
            if len(unassigned) == 1:
                lit = unassigned[0]
                assign[abs(lit)] = lit > 0
                changed = True
    return False


def check_rup(clauses, steps):
    """Verify each derived clause is RUP over the accumulated set; require an empty terminal."""
    acc = [list(c) for c in clauses]
    for i, step in enumerate(steps):
        assign = {abs(l): (l < 0) for l in step}  # negate every literal of the candidate
        if not propagate_conflicts(acc, assign):
            return False, f"step {i} (clause {step}) is NOT RUP"
        acc.append(list(step))
    if not steps or steps[-1] != []:
        return False, "refutation does not terminate in the empty clause"
    return True, f"{len(steps)} steps verified"


def verify_bundle(bundle):
    idx_path = os.path.join(bundle, "analysis", "proofs.json")
    if not os.path.exists(idx_path):
        print(f"no analysis/proofs.json in {bundle}", file=sys.stderr)
        return 2
    with open(idx_path, encoding="utf-8") as fh:
        index = json.load(fh)

    checked = failed = 0
    for ob in index.get("obligations", []):
        if ob.get("proof") != "rup_refutation":
            continue
        cnf = os.path.join(bundle, ob["cnf_dimacs"])
        drat = os.path.join(bundle, ob["proof_drat"])
        if not (os.path.exists(cnf) and os.path.exists(drat)):
            print(f"FAIL {ob['obligation'][:60]}: published files missing")
            failed += 1
            continue
        _, clauses = parse_dimacs(cnf)
        steps = parse_drat(drat)
        ok, detail = check_rup(clauses, steps)
        checked += 1
        if ok:
            print(f"OK   {ob['obligation'][:60]}  ({detail})")
        else:
            print(f"FAIL {ob['obligation'][:60]}  ({detail})")
            failed += 1

    if checked == 0:
        # A bundle with no published refutation is not a pass. Saying "0 failed" over an empty set
        # is the vacuous-green shape every gate here exists to refuse.
        print("PROOF_BUNDLE: FAIL (no refutations published to check)")
        return 1
    print(f"PROOF_BUNDLE: {'PASS' if failed == 0 else 'FAIL'} ({checked} checked, {failed} failed)")
    return 1 if failed else 0


def self_test():
    """Watch it accept a real refutation AND reject a forged one."""
    import tempfile

    d = tempfile.mkdtemp()
    cnf = os.path.join(d, "t.cnf")
    good = os.path.join(d, "good.drat")
    bad = os.path.join(d, "bad.drat")
    # (x) & (-x) is unsatisfiable; the empty clause follows by one propagation.
    with open(cnf, "w", encoding="utf-8") as fh:
        fh.write("p cnf 1 2\n1 0\n-1 0\n")
    with open(good, "w", encoding="utf-8") as fh:
        fh.write("0\n")
    # Forged: claims the empty clause over a SATISFIABLE set.
    cnf2 = os.path.join(d, "sat.cnf")
    with open(cnf2, "w", encoding="utf-8") as fh:
        fh.write("p cnf 2 1\n1 2 0\n")
    with open(bad, "w", encoding="utf-8") as fh:
        fh.write("0\n")

    _, c1 = parse_dimacs(cnf)
    ok1, d1 = check_rup(c1, parse_drat(good))
    _, c2 = parse_dimacs(cnf2)
    ok2, d2 = check_rup(c2, parse_drat(bad))

    if not ok1:
        print(f"SELF-TEST FAIL: valid refutation rejected ({d1})")
        return 1
    if ok2:
        print("SELF-TEST FAIL: forged refutation ACCEPTED over a satisfiable formula")
        return 1
    print("PROOF_BUNDLE_SELF_TEST: PASS (valid accepted, forged rejected)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        raise SystemExit(self_test())
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(verify_bundle(sys.argv[1]))
