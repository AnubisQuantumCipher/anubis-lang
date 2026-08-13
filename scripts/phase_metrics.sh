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
# Usage:
#   bash scripts/phase_metrics.sh
#   bash scripts/phase_metrics.sh --append-ledger
#
# The ledger mode runs the read-only measurement first, then atomically appends its VERBATIM output
# and return code to docs/evidence/PHASE_METRICS_LEDGER.md. Failed/unmeasured runs are recorded too;
# a convergence ledger that keeps only green observations would be an evidence filter, not a ledger.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 2

LEDGER="$ROOT/docs/evidence/PHASE_METRICS_LEDGER.md"

ledger_mode() {
  stat -f '%Lp' "$1" 2>/dev/null || stat -c '%a' "$1" 2>/dev/null
}

append_fatal() {
  echo "FATAL: $*" >&2
  exit 2
}

if [[ $# -gt 0 ]]; then
  case "$1" in
    --append-ledger)
      [[ $# -eq 1 ]] || { echo "FATAL: --append-ledger takes no argument"; exit 2; }
      ledger_dir="$(dirname "$LEDGER")"
      if [[ -e "$LEDGER" || -L "$LEDGER" ]]; then
        [[ -f "$LEDGER" && ! -L "$LEDGER" ]] \
          || append_fatal "ledger path is not a regular file: $LEDGER"
      fi
      mkdir -p "$ledger_dir" || append_fatal "cannot create ledger directory: $ledger_dir"
      [[ -d "$ledger_dir" && ! -L "$ledger_dir" ]] \
        || append_fatal "ledger directory is not a real directory: $ledger_dir"

      # mkdir is the portable macOS/Linux exclusive lock. Keep it outside the worktree so the
      # measurement does not count its own lock as a dirty path. Contenders fail closed instead of
      # each reporting APPENDED after a last-writer-wins replacement. A stale lock is intentionally
      # operator-visible; removing it without checking for a live writer would reopen the race.
      lock_key="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:24])' "$LEDGER")" \
        || append_fatal "cannot derive ledger lock key"
      lock_root="${TMPDIR:-/tmp}/anubis-phase-metrics-locks-$(id -u)"
      (umask 077 && mkdir -p "$lock_root") \
        || append_fatal "cannot create ledger lock root: $lock_root"
      [[ -d "$lock_root" && ! -L "$lock_root" ]] \
        || append_fatal "ledger lock root is not a real directory: $lock_root"
      lock_dir="$lock_root/$lock_key.lock"
      out=""
      next=""
      lock_owned=0
      cleanup_append() {
        [[ -z "$out" ]] || rm -f "$out"
        [[ -z "$next" ]] || rm -f "$next"
        if [[ "$lock_owned" -eq 1 ]]; then
          rmdir "$lock_dir" 2>/dev/null || true
        fi
      }
      trap cleanup_append EXIT
      trap 'exit 130' INT
      trap 'exit 143' TERM
      trap 'exit 129' HUP
      if ! mkdir "$lock_dir" 2>/dev/null; then
        append_fatal "ledger append lock is held: $lock_dir"
      fi
      lock_owned=1

      # Recheck under the lock. A read-only ledger is a deliberate freeze, not permission to replace
      # it with a newly writable inode. Writable custom modes are preserved across the atomic move.
      if [[ -e "$LEDGER" || -L "$LEDGER" ]]; then
        [[ -f "$LEDGER" && ! -L "$LEDGER" ]] \
          || append_fatal "ledger path changed and is not a regular file: $LEDGER"
        prior_mode="$(ledger_mode "$LEDGER")" \
          || append_fatal "cannot read ledger mode: $LEDGER"
        [[ "$prior_mode" =~ ^[0-7]{3,4}$ ]] \
          || append_fatal "unrecognized ledger mode '$prior_mode': $LEDGER"
        mode_bits=$((8#$prior_mode))
        (( (mode_bits & 0222) != 0 )) \
          || append_fatal "ledger is read-only; refusing to unfreeze it: $LEDGER"
      else
        prior_mode="644"
      fi

      out="$(mktemp "${TMPDIR:-/tmp}/anubis_phase_metrics_out.XXXXXX")" \
        || append_fatal "cannot create observation temporary outside the worktree"

      set +e
      bash "$0" >"$out" 2>&1
      measure_rc=$?
      set -u

      # The replacement must live beside the ledger for an atomic rename, but create it only AFTER
      # measurement so this script's machinery cannot inflate its own recorded dirty count.
      next="$(mktemp "$ledger_dir/.phase_metrics_ledger.XXXXXX")" \
        || append_fatal "cannot create ledger temporary in $ledger_dir"

      if [[ -f "$LEDGER" ]]; then
        cp "$LEDGER" "$next" || exit 2
      else
        printf '# Anubis phase-metrics ledger\n\n' >"$next" || exit 2
        printf 'Append-only observations emitted by `bash scripts/phase_metrics.sh --append-ledger`. ' >>"$next" || exit 2
        printf 'Each block is bound to the tree, commit, branch, and dirty count printed inside it.\n' >>"$next" || exit 2
      fi
      {
        printf '\n## %s\n\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        printf 'Command: `bash scripts/phase_metrics.sh` · exit: `%s`\n\n' "$measure_rc"
        printf '```text\n'
        cat "$out"
        printf '```\n'
      } >>"$next" || exit 2
      chmod "$prior_mode" "$next" || append_fatal "cannot preserve ledger mode $prior_mode"
      mv -f "$next" "$LEDGER" || append_fatal "cannot atomically replace ledger: $LEDGER"
      next=""
      [[ -f "$LEDGER" && ! -L "$LEDGER" ]] \
        || append_fatal "ledger replacement did not create a regular file: $LEDGER"
      actual_mode="$(ledger_mode "$LEDGER")" \
        || append_fatal "cannot verify ledger mode after append: $LEDGER"
      [[ "$actual_mode" == "$prior_mode" ]] \
        || append_fatal "ledger mode changed: expected $prior_mode, observed $actual_mode"

      cat "$out"
      rm -f "$out" || append_fatal "cannot remove observation temporary"
      out=""
      rmdir "$lock_dir" || append_fatal "cannot release ledger append lock: $lock_dir"
      lock_owned=0
      trap - EXIT INT TERM HUP
      echo "PHASE_METRICS_LEDGER: APPENDED $LEDGER (measurement rc=$measure_rc)"
      exit "$measure_rc"
      ;;
    --help|-h)
      echo "usage: bash scripts/phase_metrics.sh [--append-ledger]"
      exit 0
      ;;
    *)
      echo "FATAL: unknown argument: $1"
      exit 2
      ;;
  esac
fi

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

# ── duplicated lane pairs ──
# Count the four named pairs as PAIRS, not just one similarity percentage. A pair disappears only
# when BOTH lane-specific implementations disappear. One missing sibling is an asymmetric state and
# is UNMEASURED rather than credited as convergence.
PAIR_SPECS = [
    ('source walkers', 'expr_taint_source_m', 'expr_secret_source_m'),
    ('pattern seeders', 'seed_taint_pattern', 'seed_secret_pattern'),
    ('return summaries', 'body_returns_taint', 'body_returns_secret'),
    ('block walkers', 'walk_block_taint', 'walk_block_secret'),
]
dup_pairs = 0
dup_pair_lines = 0
pair_bodies = {}
# `block walkers` (walk_block_taint / walk_block_secret) are structurally REQUIRED to exist by
# `walker_shared_registration` in scripts/test_walker_completeness.sh — that gate rejects the tree
# if either sibling is missing or if either bypasses `walk_block_labels`. Credit the pair when both
# siblings are the thin-adapter shape that check enforces: exactly one `walk_block_labels(` call,
# exactly one self-mention (their own declaration), and no local Stmt/Expr AST matching. That is
# the same three-part shape the walker-families count above already uses, and it lines up 1:1 with
# the shared-registration invariant so this metric cannot silently disagree with G19. Any regain of
# local matching drops back to the raw pair count.
_thin_shape_re = re.compile(r'\b(?:for|while|loop|match|if\s+let|matches!)\b')
_variant_re_pair = re.compile(
    r'\b(?:' + '|'.join(re.escape(v) for v in sorted({v for v in (E + S)
                                                       if re.fullmatch(r'[A-Z]\w*', v)},
                                                      key=len, reverse=True)) + r')\s*(?:\(|\{|=>)'
) if (E or S) else None
def _is_thin_delegate(fn_body_tuple, self_name):
    if not fn_body_tuple:
        return False
    text = fn_body_tuple[2]
    # `body()` returns a span starting at the declaration line, so `self_name(` legitimately
    # appears exactly once (the declaration itself). Strip the declaration before probing.
    stripped = re.sub(r'\Afn\s+\w+\s*\([^)]*\)\s*(?:->[^{]*)?\{', '', text, count=1, flags=re.S)
    return (
        stripped.count('walk_block_labels(') == 1
        and self_name + '(' not in stripped
        and not (_variant_re_pair and _variant_re_pair.search(stripped))
        and not _thin_shape_re.search(stripped)
    )
for label, taint_name, secret_name in PAIR_SPECS:
    taint_body = body(f'fn {taint_name}(')
    secret_body = body(f'fn {secret_name}(')
    pair_bodies[label] = (taint_body, secret_body)
    if bool(taint_body) != bool(secret_body):
        print(f"{'pair parity: ' + label:40s} {'UNMEASURED':>10s}   <- exactly one sibling exists")
        bad += 1
    elif taint_body and secret_body:
        thin_delegated_pair = (
            label == 'block walkers'
            and _is_thin_delegate(taint_body, taint_name)
            and _is_thin_delegate(secret_body, secret_name)
        )
        if thin_delegated_pair:
            print(f"{'  pair: ' + label:40s} {'delegated':>10s}   thin adapter over walk_block_labels")
        else:
            dup_pairs += 1
            lines = (taint_body[1] - taint_body[0] + 1) + (secret_body[1] - secret_body[0] + 1)
            dup_pair_lines += lines
            print(f"{'  pair: ' + label:40s} {lines:>10d}   lines across both siblings")
    else:
        print(f"{'  pair: ' + label:40s} {'removed':>10s}   shared implementation expected")
print(f"{'duplicated lane pairs':40s} {dup_pairs:>10d}   0")
print(f"{'  ^ lines in duplicated pairs':40s} {dup_pair_lines:>10d}   decreasing")

# Preserve the source pair's normalised similarity as a diagnostic, not as the pair-count proxy.
def norm(text):
    for a, b in [('taint_source','LBL'),('secret_source','LBL'),('tainting_fns','FNS'),
                 ('secret_fns','FNS'),('method_tainting_fns','MFNS'),('method_secret_fns','MFNS'),
                 ('expr_taint_source_m','SRC'),('expr_secret_source_m','SRC'),
                 ('tainted','LBLD'),('secret','LBLD')]:
        text = text.replace(a, b)
    return text.split('\n')

t, s = pair_bodies['source walkers']
if t and s:
    import difflib
    a, b = norm(t[2]), norm(s[2])
    ratio = difflib.SequenceMatcher(None, a, b).ratio()
    print(f"{'source-walker pair similarity':40s} {ratio*100:>9.0f}%   diagnostic; pair count decides")
    print(f"{'  ^ lines in the source pair':40s} {len(a)+len(b):>10d}   decreasing")
elif not t and not s:
    print(f"{'source-walker pair similarity':40s} {'removed':>10s}   shared implementation expected")
else:
    # Pair parity above already marks this asymmetric state UNMEASURED.
    print(f"{'source-walker pair similarity':40s} {'UNMEASURED':>10s}   <- exactly one sibling exists")

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
for name in re.findall(r'^fn (walk_block_\w+)\(', '\n'.join(mid), re.M):
    r = body(f'fn {name}(')
    # The taint/secret entry points cease to be independent walker families once they become
    # structure-free domain adapters over the one shared statement traversal. Do not hide future
    # drift by subtracting them unconditionally: if either wrapper regains AST matching or control
    # flow, count it as an independent family again.
    thin_label_adapter = (
        name in {'walk_block_taint', 'walk_block_secret'}
        and r is not None
        and r[2].count('walk_block_labels(') == 1
        and r[2].count(name + '(') == 1
        and not re.search(r'\b(?:Stmt|Expr)::', r[2])
        and not re.search(r'\b(?:for|while|loop|match)\b', r[2])
    )
    fams += 0 if thin_label_adapter else 1
fams += 1 if re.search(r'fn walk_expr', cap) else 0
eff = pathlib.Path('compiler/src/middle/effects.rs')
fams += 1 if (eff.exists() and 'fn walk_expr' in eff.read_text()) else 0
print(f"{'walker families':40s} {fams:>10d}   non-increasing, → 1")

# ── general ExprStmt arm present in both label lanes? ──
# The taint/secret entry points are thin adapters over the single shared statement traversal
# `walk_block_labels`. Once they no longer own AST matching, the "general ExprStmt arm" question
# is answered by whichever function carries the traversal. Credit the adapter for delegating,
# subject to a strict thin-adapter shape AND a positive check that `walk_block_labels` itself
# carries the general ExprStmt arm. If either lane regains local AST matching in ANY form the
# check recognises (qualified or imported variant name, `match`, `if let`, or a `matches!(...)`
# macro), the check falls back to grading the local body and reports UNMEASURED if the arm is
# neither delegated nor locally present.
shared_walker_body = body('fn walk_block_labels(')
shared_has_arm = bool(
    shared_walker_body
    and re.search(r'\bStmt::ExprStmt\(\s*(?:expr|e)\s*\)', shared_walker_body[2])
)
# Every declared Expr/Stmt variant name — matches both qualified (`Stmt::Let { .. }`) and
# imported (`Let { .. }`) forms a walker might use after `use frontend::Stmt::*;`. Kept in one
# alternation so a new variant automatically enters the check.
_variant_names = sorted({v for v in (E + S) if re.fullmatch(r'[A-Z]\w*', v)}, key=len, reverse=True)
_variant_re = re.compile(
    r'\b(?:' + '|'.join(re.escape(v) for v in _variant_names) + r')\s*(?:\(|\{|=>)'
) if _variant_names else None
_local_ast_patterns = re.compile(r'\b(?:for|while|loop|match|if\s+let|matches!)\b')
for fn in ['fn walk_block_taint(', 'fn walk_block_secret(']:
    r = body(fn)
    if not r: continue
    name = fn[3:-1]
    local_arm = bool(re.search(r'\bStmt::ExprStmt\(\s*(?:expr|e)\s*\)', r[2]))
    # Strip the fn declaration and signature before checking for local matching; body() returns
    # a span starting at the `fn` line, so `count(name + '(') == 1` would otherwise be counting
    # the declaration itself and treat any additional self-mention as a violation.
    body_after_sig = re.sub(r'\Afn\s+\w+\s*\([^)]*\)\s*(?:->[^{]*)?\{', '', r[2], count=1, flags=re.S)
    has_local_variant = bool(_variant_re and _variant_re.search(body_after_sig))
    has_local_ast_form = bool(_local_ast_patterns.search(body_after_sig))
    delegates_shared = (
        body_after_sig.count('walk_block_labels(') == 1
        and name + '(' not in body_after_sig
        and not has_local_variant
        and not has_local_ast_form
    )
    gen = local_arm or (delegates_shared and shared_has_arm)
    detail = 'local' if local_arm else ('via walk_block_labels' if gen else 'missing')
    verdict = 'yes' if gen else 'UNMEASURED'
    print(f"{'general ExprStmt arm: '+name:40s} {verdict:>10s}   {detail}")
    if not gen:
        bad += 1

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
