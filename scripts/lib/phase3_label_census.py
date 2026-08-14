#!/usr/bin/env python3
"""Completion Blueprint Phase 3 label-site census.

Enumerates every direct read/write and struct-literal initialization of the
security-label fields of `ScopeBinding` in `compiler/src/middle/mod.rs`,
grouped by enclosing `fn` name. The four tracked fields (mission §92) are:

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

Field-access matching uses complete identifiers with a trailing word-boundary
so that identifiers such as `.secret_fns`, `.secret_present`,
`.secret_source`, `.tainted_call`, or `.taint_source_of` do NOT contaminate the
census. `ScopeBinding { ... }` and nested `BindingInfo { ... }` label-field
initializers are writes, including shorthand initializers. Every occurrence is
counted independently, and each field access is classified as `writes`
(standalone assignment, `=` but not `==` or `=>`) or `reads` (anything else,
including reference/`&mut` usages and expression positions). A line like
`b.info.taint_source = b.info.taint_source.take().or(source);` therefore
contributes one `write` and one `read` in the `taint_source` bucket.

## Stability

The census is line-count-agnostic — adjacent unrelated edits do not perturb
it. A real drift is one of: a new enclosing `fn` writing/reading a tracked
field, a new field kind, or a change in the (writes, reads) shape of an
existing bucket.

Output on stdout: one TSV row per bucket, sorted by `(fn, field)`, plus a
`__totals__` trailer. Format:

    <fn>\t<field>\t<writes>\t<reads>

Exit 0 on a valid census; exit 2 when the source is missing or cannot be parsed.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Dict, List, Optional, Tuple

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

# Function declarations may be top-level, methods inside an `impl`, or tests
# inside a module. Local helper functions are legal Rust too, so ownership
# cannot be inferred from the nearest preceding `fn` token.
FN_RE = re.compile(r"(?<![A-Za-z0-9_])fn\s+([A-Za-z_][A-Za-z0-9_]*)")
RAW_STRING_RE = re.compile(r'(?:br|r)(?P<hashes>#{0,255})"')
STRUCT_LITERAL_RE = re.compile(r"\b(ScopeBinding|BindingInfo)\s*\{")
STRUCT_LITERAL_FIELDS = {
    "ScopeBinding": frozenset({"secret"}),
    "BindingInfo": frozenset({"tainted", "taint_source", "declassified"}),
}
NON_LITERAL_PREDECESSORS = frozenset({"struct", "impl", "for", "let"})
LITERAL_FIELD_RE = re.compile(
    r"\b(tainted|taint_source|declassified|secret)\b\s*(?::|(?=[,}]))"
)


@dataclass
class _LiteralContext:
    type_name: str
    paren_depth: int
    bracket_depth: int
    expect_field: bool = True


def _char_literal_end(line: str, start: int) -> Optional[int]:
    """Return the exclusive end of a Rust char/byte-char literal, if present."""
    quote = start + 1 if line.startswith("b'", start) else start
    if quote >= len(line) or line[quote] != "'" or quote + 1 >= len(line):
        return None

    content = quote + 1
    if line[content] != "\\":
        closing = content + 1
    else:
        escape = content + 1
        if escape >= len(line):
            return None
        if line[escape] == "u" and escape + 1 < len(line) and line[escape + 1] == "{":
            end_brace = line.find("}", escape + 2)
            if end_brace < 0:
                return None
            closing = end_brace + 1
        elif line[escape] == "x":
            closing = escape + 3
        else:
            closing = escape + 1

    if closing < len(line) and line[closing] == "'":
        return closing + 1
    return None


def mask_non_code(src_lines: List[str]) -> List[str]:
    """Mask comments and literals while preserving code positions and braces."""
    masked_lines: List[str] = []
    block_comment_depth = 0
    string_mode: Optional[str] = None
    raw_hashes = ""

    for line in src_lines:
        out = [" "] * len(line)
        i = 0
        while i < len(line):
            if block_comment_depth:
                if line.startswith("/*", i):
                    block_comment_depth += 1
                    i += 2
                elif line.startswith("*/", i):
                    block_comment_depth -= 1
                    i += 2
                else:
                    i += 1
                continue

            if string_mode == "normal":
                if line[i] == "\\":
                    i += min(2, len(line) - i)
                elif line[i] == '"':
                    string_mode = None
                    i += 1
                else:
                    i += 1
                continue

            if string_mode == "raw":
                terminator = '"' + raw_hashes
                if line.startswith(terminator, i):
                    string_mode = None
                    raw_hashes = ""
                    i += len(terminator)
                else:
                    i += 1
                continue

            if line.startswith("//", i):
                break
            if line.startswith("/*", i):
                block_comment_depth = 1
                i += 2
                continue

            raw = RAW_STRING_RE.match(line, i)
            if raw:
                string_mode = "raw"
                raw_hashes = raw.group("hashes")
                i = raw.end()
                continue

            if line.startswith('b"', i):
                string_mode = "normal"
                i += 2
                continue
            if line[i] == '"':
                string_mode = "normal"
                i += 1
                continue

            char_end = _char_literal_end(line, i)
            if char_end is not None:
                i = char_end
                continue

            out[i] = line[i]
            i += 1
        masked_lines.append("".join(out))

    if block_comment_depth or string_mode is not None:
        raise ValueError("unterminated Rust comment or string literal")
    return masked_lines


def enclosing_functions(code_lines: List[str]) -> List[List[Tuple[int, str]]]:
    """Return owner-change segments for masked Rust source lines."""
    owners: List[List[Tuple[int, str]]] = []
    stack: List[Tuple[str, int]] = []
    pending_fn: Optional[Tuple[str, int, int]] = None
    brace_depth = 0
    paren_depth = 0
    bracket_depth = 0

    for code in code_lines:
        segments = [(0, stack[-1][0] if stack else "<toplevel>")]
        declarations = {m.start(): m.group(1) for m in FN_RE.finditer(code)}


        for position, ch in enumerate(code):
            if position in declarations:
                if pending_fn is not None:
                    raise ValueError("function declaration before prior declaration ended")
                pending_fn = (declarations[position], paren_depth, bracket_depth)

            if ch == "(":
                paren_depth += 1
            elif ch == ")":
                paren_depth -= 1
                if paren_depth < 0:
                    raise ValueError("unbalanced closing parenthesis in Rust source")
            elif ch == "[":
                bracket_depth += 1
            elif ch == "]":
                bracket_depth -= 1
                if bracket_depth < 0:
                    raise ValueError("unbalanced closing bracket in Rust source")
            elif ch == "{":
                brace_depth += 1
                if (
                    pending_fn is not None
                    and paren_depth == pending_fn[1]
                    and bracket_depth == pending_fn[2]
                ):
                    owner = pending_fn[0]
                    stack.append((owner, brace_depth))
                    pending_fn = None
                    if segments[-1][1] != owner:
                        segments.append((position + 1, owner))
            elif ch == "}":
                brace_depth -= 1
                if brace_depth < 0:
                    raise ValueError("unbalanced closing brace in Rust source")
                while stack and stack[-1][1] > brace_depth:
                    stack.pop()
                owner = stack[-1][0] if stack else "<toplevel>"
                if segments[-1][1] != owner:
                    segments.append((position + 1, owner))
            elif (
                ch == ";"
                and pending_fn is not None
                and paren_depth == pending_fn[1]
                and bracket_depth == pending_fn[2]
            ):
                # Trait/extern declaration without a body. Semicolons inside a
                # fixed-size array type (`[T; N]`) remain inside brackets.
                pending_fn = None
        owners.append(segments)

    if (
        brace_depth != 0
        or paren_depth != 0
        or bracket_depth != 0
        or stack
        or pending_fn is not None
    ):
        raise ValueError("unbalanced Rust source or function declaration")
    return owners


def owner_at(segments: List[Tuple[int, str]], position: int) -> str:
    """Return the active function owner at one source-column position."""
    owner = segments[0][1]
    for start, candidate in segments:
        if start > position:
            break
        owner = candidate
    return owner


def struct_literal_writes(code_lines: List[str]) -> List[Tuple[int, int, str]]:
    """Return `(line, column, field)` writes in tracked struct literals."""
    sites: List[Tuple[int, int, str]] = []
    brace_stack: List[Optional[_LiteralContext]] = []
    paren_depth = 0
    bracket_depth = 0

    for line_index, code in enumerate(code_lines):
        openers: Dict[int, str] = {}
        for match in STRUCT_LITERAL_RE.finditer(code):
            prefix = code[: match.start()]
            previous = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*$", prefix)
            previous_word = previous.group(1) if previous else None
            if (
                prefix.rstrip().endswith("->")
                or previous_word in NON_LITERAL_PREDECESSORS
            ):
                continue
            openers[match.end() - 1] = match.group(1)

        field_matches = {
            match.start(): match for match in LITERAL_FIELD_RE.finditer(code)
        }
        for position, ch in enumerate(code):
            context = brace_stack[-1] if brace_stack else None
            field_match = field_matches.get(position)
            if (
                context is not None
                and context.expect_field
                and context.paren_depth == paren_depth
                and context.bracket_depth == bracket_depth
                and field_match is not None
            ):
                field = field_match.group(1)
                if field in STRUCT_LITERAL_FIELDS[context.type_name]:
                    sites.append((line_index, position, field))

            if ch == "(":
                paren_depth += 1
            elif ch == ")":
                paren_depth -= 1
            elif ch == "[":
                bracket_depth += 1
            elif ch == "]":
                bracket_depth -= 1
            elif ch == "{":
                type_name = openers.get(position)
                brace_stack.append(
                    _LiteralContext(type_name, paren_depth, bracket_depth)
                    if type_name is not None
                    else None
                )
            elif ch == "}":
                brace_stack.pop()
            elif (
                context is not None
                and context.paren_depth == paren_depth
                and context.bracket_depth == bracket_depth
            ):
                if ch == ":":
                    context.expect_field = False
                elif ch == ",":
                    context.expect_field = True

    return sites

def role_of_occurrence(line: str, occ_end: int) -> str:
    """Classify a single field-access occurrence as `writes` or `reads`.

    An occurrence is a write iff the first non-whitespace character after the
    match is a single `=` (i.e. an assignment), *not* `==`, `=>`, `!=`, `<=`,
    `>=`, or a compound-assign. Everything else (including borrow, method-call,
    `.take()`, `.as_ref()`, tuple pattern binding, etc.) is a read.
    """
    tail = line[occ_end:].lstrip()
    if tail.startswith("="):
        # Rule out equality/comparison operators.
        after = tail[1:2]
        if after in {"=", ">"}:
            return "reads"
        return "writes"
    return "reads"


def enumerate_sites(src_path: Path) -> Dict[Tuple[str, str], Dict[str, int]]:
    with src_path.open() as fh:
        lines = fh.readlines()
    code_lines = mask_non_code(lines)
    owners = enclosing_functions(code_lines)

    buckets: Dict[Tuple[str, str], Dict[str, int]] = defaultdict(
        lambda: {"writes": 0, "reads": 0}
    )
    for idx, code in enumerate(code_lines):
        for field_name, field_re in FIELD_PATTERNS:
            for m in field_re.finditer(code):
                role = role_of_occurrence(code, m.end())
                fn = owner_at(owners[idx], m.start())
                buckets[(fn, field_name)][role] += 1
    for line_index, column, field_name in struct_literal_writes(code_lines):
        fn = owner_at(owners[line_index], column)
        buckets[(fn, field_name)]["writes"] += 1
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

    try:
        buckets = enumerate_sites(src_path)
    except ValueError as exc:
        print(f"phase3_label_census: cannot parse {src_path}: {exc}", file=sys.stderr)
        return 2
    total_w = sum(b["writes"] for b in buckets.values())
    total_r = sum(b["reads"] for b in buckets.values())

    for (fn, field) in sorted(buckets.keys()):
        counts = buckets[(fn, field)]
        print(f"{fn}\t{field}\t{counts['writes']}\t{counts['reads']}")
    print(f"__totals__\t-\t{total_w}\t{total_r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
