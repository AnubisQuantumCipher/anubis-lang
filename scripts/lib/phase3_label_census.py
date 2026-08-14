#!/usr/bin/env python3
"""Completion Blueprint Phase 3 label-site census.

Enumerates every direct read/write of the security-label fields of
`ScopeBinding` in `compiler/src/middle/mod.rs`, grouped by enclosing top-level
`fn` name. The four tracked fields (mission §92) are:

- `ScopeBinding.info.tainted`      (`bool`)
- `ScopeBinding.info.taint_source` (`Option<String>`)
- `ScopeBinding.info.declassified` (`bool`)
- `ScopeBinding.secret`            (`bool`)

The Phase 3 mission (§115) requires that these sites be enumerated before any
migration to the explicit `SecurityLabel` lattice, and that new AST variants
or unclassified constructors fail the gate. The classified expected inventory
lives in `docs/phase3/label_census.tsv`; this tool produces the *current*
inventory from source, and the wrapper `scripts/run_phase3_label_census.sh`
compares them.

## Precision

Field matching uses complete identifiers with a trailing word-boundary so that
identifiers such as `.secret_fns`, `.secret_present`, `.secret_source`,
`.tainted_call`, or `.taint_source_of` do NOT contaminate the census. Every
occurrence on a line is counted independently via `re.finditer`, and each is
classified as `writes` (assignment on the RHS, `=` not `==`) or `reads`
(anything else, including reference/`&mut` usages and expression positions).
A line like `b.info.taint_source = b.info.taint_source.take().or(source);`
therefore contributes one `write` and one `read` in the `taint_source` bucket.

## Stability

The census is line-count-agnostic — adjacent unrelated edits do not perturb
it. A real drift is one of: a new enclosing `fn` writing/reading a tracked
field, a new field kind, or a change in the (writes, reads) shape of an
existing bucket.

Output on stdout: one TSV row per bucket, sorted by `(fn, field)`, plus a
`__totals__` trailer. Format:

    <fn>\t<field>\t<writes>\t<reads>

Exit 0 always; comparison is the wrapper's job.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Dict, List, Tuple

# Tracked field patterns. Each is a full-word match: the trailing negative
# lookahead `(?![A-Za-z0-9_])` prevents `.secret_fns`, `.tainted_call`,
# `.taint_source_of`, `.declassified_call` from being counted as ScopeBinding
# label accesses. The `\.` prefix keeps the match anchored on a real field
# access (also excludes plain identifiers such as `let secret = ...`).
FIELD_PATTERNS: List[Tuple[str, re.Pattern[str]]] = [
    ("tainted",      re.compile(r"\.info\.tainted(?![A-Za-z0-9_])")),
    ("taint_source", re.compile(r"\.info\.taint_source(?![A-Za-z0-9_])")),
    ("declassified", re.compile(r"\.info\.declassified(?![A-Za-z0-9_])")),
    ("secret",       re.compile(r"\.secret(?![A-Za-z0-9_])")),
]

# `fn NAME(...)` on a line that starts with (optional) attributes / visibility.
FN_RE = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")


def enclosing_fn(src_lines: List[str], line_no: int) -> str:
    """Return the name of the innermost `fn X(` at or above `line_no`.

    `line_no` is 1-indexed. Falls back to `"<toplevel>"` if no `fn` is found.
    """
    for i in range(line_no - 1, -1, -1):
        m = FN_RE.match(src_lines[i])
        if m:
            return m.group(1)
    return "<toplevel>"


def role_of_occurrence(line: str, occ_end: int) -> str:
    """Classify a single field-access occurrence as `writes` or `reads`.

    An occurrence is a write iff the first non-whitespace character after the
    match is a single `=` (i.e. an assignment), *not* `==`, `!=`, `<=`, `>=`,
    or a compound-assign. Everything else (including borrow, method-call,
    `.take()`, `.as_ref()`, tuple pattern binding, etc.) is a read.
    """
    tail = line[occ_end:].lstrip()
    if tail.startswith("="):
        # Rule out equality/comparison operators.
        after = tail[1:2]
        if after == "=":
            return "reads"
        return "writes"
    return "reads"


def enumerate_sites(src_path: Path) -> Dict[Tuple[str, str], Dict[str, int]]:
    with src_path.open() as fh:
        lines = fh.readlines()

    buckets: Dict[Tuple[str, str], Dict[str, int]] = defaultdict(
        lambda: {"writes": 0, "reads": 0}
    )
    for idx, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        # Skip full-line comments; in-line trailing comments after code are
        # kept because the code half of the line may still carry an access.
        if stripped.startswith("//"):
            continue
        for field_name, field_re in FIELD_PATTERNS:
            for m in field_re.finditer(raw):
                role = role_of_occurrence(raw, m.end())
                fn = enclosing_fn(lines, idx)
                buckets[(fn, field_name)][role] += 1
    return buckets


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", default=".", help="repository root (default: cwd)")
    p.add_argument(
        "--source",
        default="compiler/src/middle/mod.rs",
        help="source file relative to root",
    )
    args = p.parse_args(argv)

    src_path = Path(args.root).resolve() / args.source
    if not src_path.exists():
        print(f"phase3_label_census: source not found: {src_path}", file=sys.stderr)
        return 2

    buckets = enumerate_sites(src_path)
    total_w = sum(b["writes"] for b in buckets.values())
    total_r = sum(b["reads"] for b in buckets.values())

    for (fn, field) in sorted(buckets.keys()):
        counts = buckets[(fn, field)]
        print(f"{fn}\t{field}\t{counts['writes']}\t{counts['reads']}")
    print(f"__totals__\t-\t{total_w}\t{total_r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
