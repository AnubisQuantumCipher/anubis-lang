#!/usr/bin/env bash
# Emit the convergence metrics for a phase-completion report (blueprint Part II.5 §7).
#
# WHY THIS EXISTS: every one of these numbers has been hand-typed into a status document at least
# once, and every one of them rotted. Worse, one of them was measured WRONG and published: walker
# sizes were computed as "distance to the next walker's start line", which swept in 186 unrelated
# top-level items and produced a confident 8,323-line / 29-of-29 figure for a 215-line / 3-of-29
# function. This script brace-matches each function body instead, and prints the tree and commit
# beside every number, because the same file legitimately measured 28,801 and 31,298 on one day.
#
# FAIL CLOSED: if a target function cannot be located, the metric prints UNMEASURED and the script
# exits non-zero. A metric that silently reports 0 because its regex missed is worse than no metric.
#
# Usage:  bash scripts/phase_metrics.sh [--json]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 2

MID="compiler/src/middle/mod.rs"
CAP="compiler/src/middle/capability.rs"
FRONT="compiler/src/frontend/mod.rs"
for f in "$MID" "$CAP" "$FRONT"; do
  [ -f "$f" ] || { echo "FATAL: missing $f"; exit 2; }
done

echo "═══ PHASE METRICS ═══"
echo "tree      : $ROOT"
echo "commit    : $(git rev-parse HEAD 2>/dev/null || echo UNKNOWN)"
echo "branch    : $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo UNKNOWN)"
echo "dirty     : $(git status --porcelain 2>/dev/null | wc -l | tr -d ' ') entries"
echo

python3 - "$MID" "$CAP" "$FRONT" <<'PY'
import re, sys, pathlib
mid_p, cap_p, front_p = sys.argv[1], sys.argv[2], sys.argv[3]
mid = pathlib.Path(mid_p).read_text().splitlines()
cap = pathlib.Path(cap_p).read_text()
front = pathlib.Path(front_p).read_text()
WC = re.compile(r'_\s*=>')
bad = 0

def variants(name):
    m = re.search(r'pub enum ' + name + r'\s*\{', front)
    if not m: return []
    i = m.end(); d = 1; j = i
    while j < len(front):
        if front[j] == '{': d += 1
        elif front[j] == '}':
            d -= 1
            if d == 0: break
        j += 1
    return sorted(set(re.findall(r'^\s{4}([A-Z]\w*)', front[i:j], re.M)))

E, S = variants('Expr'), variants('Stmt')

def body(pat):
    """Brace-matched body of the first fn whose line starts with pat. None if absent."""
    for n, l in enumerate(mid):
        if l.startswith(pat):
            d = 0; started = False
            for m in range(n, len(mid)):
                d += mid[m].count('{') - mid[m].count('}')
                if '{' in mid[m]: started = True
                if started and d <= 0:
                    return n + 1, m + 1, '\n'.join(mid[n:m + 1])
    return None

LABEL_FNS = ['fn walk_block_taint(', 'fn walk_block_secret(',
             'fn expr_taint_source_m(', 'fn expr_secret_source_m(']

print(f"{'metric':40s} {'value':>10s}   target")
print("-" * 74)
print(f"{'middle/mod.rs lines':40s} {len(mid):>10d}   strictly decreasing (Phase 2+)")

# ── duplicated lane pairs: structural similarity of the two source walkers ──
def norm(text):
    for a, b in [('taint_source','LBL'),('secret_source','LBL'),('tainting_fns','FNS'),
                 ('secret_fns','FNS'),('method_tainting_fns','MFNS'),('method_secret_fns','MFNS'),
                 ('expr_taint_source_m','SRC'),('expr_secret_source_m','SRC'),
                 ('tainted','LBLD'),('secret','LBLD')]:
        text = text.replace(a, b)
    return text.split('\n')

t = body('fn expr_taint_source_m('); s = body('fn expr_secret_source_m(')
if t and s:
    import difflib
    a, b = norm(t[2]), norm(s[2])
    ratio = difflib.SequenceMatcher(None, a, b).ratio()
    print(f"{'source-walker pair similarity':40s} {ratio*100:>9.0f}%   0% (one implementation)")
    print(f"{'  ^ lines in the pair':40s} {len(a)+len(b):>10d}   ~half")
else:
    print(f"{'source-walker pair similarity':40s} {'UNMEASURED':>10s}   <- fn not found"); bad += 1

# ── fused cross-lane joins ──
joins = sum(1 for l in mid if 'merge_taint_over(' in l and 'fn merge_taint_over' not in l)
has_fused = 1 if any('fn merge_taint_over' in l for l in mid) else 0
print(f"{'fused cross-lane joins':40s} {has_fused:>10d}   0 (per-lane joins)")
print(f"{'  ^ call sites':40s} {joins:>10d}   -")

# ── wildcards in label-lane walkers ──
tot_wc = 0; missing = []
for fn in LABEL_FNS:
    r = body(fn)
    if not r: missing.append(fn); continue
    tot_wc += len(WC.findall(r[2]))
if missing:
    print(f"{'_ => in label-lane walkers':40s} {'UNMEASURED':>10s}   <- {', '.join(missing)}"); bad += 1
else:
    print(f"{'_ => in label-lane walkers':40s} {tot_wc:>10d}   0 (as in capability.rs)")

# ── lane facts with no join ──
noj = 1 if any('effects: &mut Vec<String>' in l for l in mid) else 0
print(f"{'lane facts with no join':40s} {noj:>10d}   0 (every lane a lattice)")

# ── totality reference: capability.rs's WALKERS only, not the whole 4.4k-line file ──
# First cut of this script counted `_ =>` across all of capability.rs and printed 24, which reads as
# "the reference walker is full of wildcards" — it is not. The zero-wildcard property belongs to
# walk_expr/walk_stmt specifically. Measuring the enclosing file instead of the function is the same
# mistake that produced the 8,323-line walker; it is fixed here rather than explained away.
# capability.rs defines walk_expr MORE THAN ONCE. A first-match lookup silently grades whichever
# copy appears first (:532, a 100-line helper) instead of the 200-line variant dispatcher at :2489 —
# the same class of error as measuring the enclosing file instead of the function. So: enumerate
# EVERY definition, report each with its line number, and separate TOP-LEVEL variant-dispatch arms
# (the ones that decide Expr/Stmt totality) from nested arms in inner matches, which do not.
cap_lines = cap.splitlines()
def all_bodies(pat):
    out = []
    for n, l in enumerate(cap_lines):
        if l.strip().startswith(pat):
            d = 0; started = False
            for m in range(n, len(cap_lines)):
                d += cap_lines[m].count('{') - cap_lines[m].count('}')
                if '{' in cap_lines[m]: started = True
                if started and d <= 0:
                    out.append((n + 1, m + 1, cap_lines[n:m + 1])); break
    return out

# Grade the VARIANT DISPATCHER for each name — the largest body, not the first match. capability.rs
# has a 100-line walk_expr helper at :532 and the real 200-line dispatcher at :2489; grading the
# helper produced a spurious "REGRESSION". Helpers are printed for transparency but not graded.
regress = 0
for pat in ('fn walk_expr(', 'fn walk_stmt('):
    defs = all_bodies(pat)
    if not defs:
        print(f"{'  ' + pat[3:-1] + ' (reference)':40s} {'UNMEASURED':>10s}   <- not found"); bad += 1
        continue
    dispatcher = max(defs, key=lambda d: d[1] - d[0])
    for (a, b, blines) in defs:
        depths = [len(l) - len(l.lstrip()) for l in blines if WC.search(l)]
        top = sum(1 for d in depths if d <= 12)
        nested = len(depths) - top
        role = 'dispatcher' if (a, b) == (dispatcher[0], dispatcher[1]) else 'helper'
        flag = ''
        if role == 'dispatcher' and top:
            flag = '  <- REGRESSION'; regress += 1
        print(f"{'  ' + pat[3:-1] + ' @' + str(a) + ' (' + role + ')':40s} "
              f"{('top=' + str(top) + ' nest=' + str(nested)):>10s}   dispatcher top must be 0{flag}")

# ── walker families ──
fams = 0
fams += len(re.findall(r'^fn walk_block_\w+\(', '\n'.join(mid), re.M))
fams += 1 if re.search(r'fn walk_expr', cap) else 0
eff = pathlib.Path('compiler/src/middle/effects.rs')
fams += 1 if (eff.exists() and 'fn walk_expr' in eff.read_text()) else 0
print(f"{'walker families':40s} {fams:>10d}   non-increasing, → 1")

# ── general ExprStmt arm present in both label lanes? ──
for fn in ['fn walk_block_taint(', 'fn walk_block_secret(']:
    r = body(fn)
    if not r: continue
    gen = bool(re.search(r'Stmt::ExprStmt\(\s*(?:expr|e)\s*\)', r[2]))
    print(f"{'general ExprStmt arm: '+fn[3:-1]:40s} {('yes' if gen else 'NO'):>10s}   yes")

print()
print(f"Expr variants: {len(E)}   Stmt variants: {len(S)}")
sys.exit(2 if bad else (1 if regress else 0))
PY
rc=$?
echo
case $rc in
  0) echo "PHASE_METRICS: OK" ;;
  1) echo "PHASE_METRICS: REGRESSION — a reference dispatcher grew a top-level wildcard." ;;
  2) echo "PHASE_METRICS: UNMEASURED — a metric could not be located; it must not be reported as zero." ;;
  *) echo "PHASE_METRICS: ERROR (rc=$rc)" ;;
esac
exit $rc
