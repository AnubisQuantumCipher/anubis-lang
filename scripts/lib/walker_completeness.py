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
(i.e. can hold code), then parse the walker's match arms. Every such field must be bound and either
used in that arm body (DIRECT) or named by an exact DEFERRED contract whose alternate consumers and
fail-closed fallbacks still exist. Missing, broken, and stale deferred contracts are reported.

Deliberately a source-level check rather than a Rust lint: it needs the AST definition and the
walker body together, it must run without a nightly toolchain, and its failure message names the
exact field a new AST change forgot.

LIMITS, stated rather than implied. DIRECT proves that a non-discard use exists, not that an
arbitrary use is semantically correct. DEFERRED proves only the named source contracts. This is a
mechanized floor; semantic fixtures and mutation poisons remain the evidence for actual behavior.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

AST = Path("compiler/src/frontend/mod.rs")
# Canonical product runs always grade the repository middle module. Scratch self-tests assign MID
# explicitly inside their isolated Python process; ambient environment is not source authority.
MID = Path("compiler/src/middle/mod.rs")
# Sibling modules that own their own `walk_expr`. The census found 26 value-flow walkers; three of
# them live outside mod.rs, and the gate could not register them for the simple reason that it only
# ever read one file. A registry that cannot reach a walker is not a judgement about that walker.
SIBLINGS = [
    Path("compiler/src/middle/effects.rs"),
    Path("compiler/src/middle/capability.rs"),
    Path("compiler/src/middle/trifecta.rs"),
]


@dataclass(frozen=True)
class DeferredDisposition:
    kind: str
    rationale: str
    consumer_patterns: tuple[tuple[str, str, str], ...]
    fallback_patterns: tuple[tuple[str, str, str], ...]


DEFERRED_KINDS = frozenset({"DEFERRED_AT_DEFINITION"})


# A DEFERRED entry is an exception to direct arm-body consumption, not an allow-list for omission.
# `check()` rejects missing requirements and entries that become stale after a field gains direct
# use. The table is mutable only so scratch self-tests can inject a synthetic contract.
DEFERRED_FIELDS: dict[tuple[str, str, str], DeferredDisposition] = {
    (
        "effects::walk_expr",
        "Expr::Lambda",
        "body",
    ): DeferredDisposition(
        kind="DEFERRED_AT_DEFINITION",
        rationale=(
            "a closure literal performs no effect at definition; known applications consume its "
            "body and unresolved calls open the transitive effect row"
        ),
        consumer_patterns=(
            (
                "known_hof_consumer",
                "walk_expr",
                r"for\s+&i\s+in\s+higher_order_closure_args\s*\(\s*callee\s*\)\s*\{.*?"
                r"if\s+let\s+Some\s*\(\s*Expr::Lambda\s*\{\s*params\s*,\s*body\s*\}\s*\)\s*="
                r"\s*args\.get\s*\(\s*i\s*\)\s*\{.*?walk_expr\s*\(\s*body\s*,\s*cx\s*,"
                r"\s*scope\s*,\s*row\s*\)",
            ),
        ),
        fallback_patterns=(
            (
                "local_callable_fallback",
                "walk_expr",
                r"if\s+scope\.contains\s*\(\s*callee\s*\)\s*\{.*?row\.open\s*=\s*true\s*;",
            ),
            (
                "unknown_bare_fallback",
                "walk_expr",
                r"\}\s*else\s*\{\s*row\.open\s*=\s*true\s*;\s*\}\s*\}\s*Expr::CallExpr",
            ),
            (
                "call_expr_fallback",
                "walk_expr",
                r"Expr::CallExpr\s*\{\s*callee\s*,\s*args\s*\}\s*=>\s*\{.*?"
                r"row\.open\s*=\s*true\s*;",
            ),
        ),
    ),
    (
        "analyze_expr_effect",
        "Expr::Lambda",
        "body",
    ): DeferredDisposition(
        kind="DEFERRED_AT_DEFINITION",
        rationale=(
            "definition-site traversal over-rejects inert closures; direct, builtin-HOF, and "
            "user-function application paths consume the body, while unknown callees are rejected"
        ),
        consumer_patterns=(
            (
                "direct_local_consumer",
                "analyze_expr_effect",
                r"for\s+lam\s+in\s+applied_closure_candidates\s*\(\s*callee\s*,\s*scope\s*\)\s*\{.*?"
                r"if\s+let\s+Expr::Lambda\s*\{\s*params\s*,\s*body\s*\}\s*=\s*lam\.as_ref\s*\(\s*\)"
                r"\s*\{.*?analyze_expr_effect\s*\(\s*body\s*,\s*mode\s*,\s*&local\s*,\s*effects\s*,"
                r"\s*ctx\s*\)",
            ),
            (
                "known_hof_consumer",
                "analyze_expr_effect",
                r"let\s+resolved\s*:\s*Option\s*<\s*&Expr\s*>\s*=\s*match\s+args\.get\s*\(\s*i\s*\)"
                r".*?if\s+let\s+Some\s*\(\s*Expr::Lambda\s*\{\s*params\s*,\s*body\s*\}\s*\)\s*="
                r"\s*resolved\s*\{(?:(?!\}\s*else\s+if).)*?analyze_expr_effect\s*\(\s*body\s*,"
                r"\s*mode\s*,\s*&local\s*,\s*effects\s*,\s*ctx\s*\)",
            ),
            (
                "applied_param_consumer",
                "analyze_expr_effect",
                r"if\s+let\s+Some\s*\(\s*applied\s*\)\s*=\s*ctx\.fn_applies_param\.get\s*\(\s*callee"
                r"\s*\)\.cloned\s*\(\s*\)\s*\{.*?if\s+let\s+Expr::Lambda\s*\{\s*params\s*,\s*body"
                r"\s*\}.*?analyze_expr_effect\s*\(\s*body\s*,\s*mode\s*,\s*&local\s*,\s*effects\s*,"
                r"\s*ctx\s*\)",
            ),
        ),
        fallback_patterns=(
            (
                "unknown_call_fallback",
                "check_calls_expr_nc",
                r"Expr::Call\s*\{\s*callee\s*,\s*args\s*\}\s*=>\s*\{"
                r".*?if\s+!fns\.contains\s*\(\s*callee\s*\).*?&&\s*!bound\.contains\s*\(\s*callee\s*\)"
                r".*?&&\s*!crate::backends::run::is_builtin_name\s*\(\s*callee\s*\).*?"
                r"ctx\.diagnostics\.push",
            ),
        ),
    ),
}


def _scrub_rust(src: str) -> str:
    """Replace comments and literals with spaces while preserving offsets/newlines."""
    out = list(src)
    i = 0
    n = len(src)
    while i < n:
        if src.startswith("//", i):
            end = src.find("\n", i + 2)
            if end < 0:
                end = n
            for j in range(i, end):
                out[j] = " "
            i = end
            continue
        if src.startswith("/*", i):
            start = i
            depth = 1
            i += 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise SystemExit("unterminated Rust block comment")
            for j in range(start, i):
                if out[j] != "\n":
                    out[j] = " "
            continue
        raw = re.match(r"(?:br|r)(?P<hashes>#{0,255})\"", src[i:])
        if raw:
            start = i
            hashes = raw.group("hashes")
            i += raw.end()
            terminator = '"' + hashes
            end = src.find(terminator, i)
            if end < 0:
                raise SystemExit("unterminated Rust raw string")
            i = end + len(terminator)
            for j in range(start, i):
                if out[j] != "\n":
                    out[j] = " "
            continue
        quote_at = i + 1 if src.startswith('b"', i) else i
        if quote_at < n and src[quote_at] == '"':
            start = i
            i = quote_at + 1
            escaped = False
            while i < n:
                char = src[i]
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    i += 1
                    break
                i += 1
            else:
                raise SystemExit("unterminated Rust string")
            for j in range(start, i):
                if out[j] != "\n":
                    out[j] = " "
            continue
        # Scrub character/byte-character literals, but not lifetimes such as `'a`.
        char_match = re.match(r"(?:b)?'(?:\\.|[^\\'\n])'", src[i:])
        if char_match:
            end = i + char_match.end()
            for j in range(i, end):
                out[j] = " "
            i = end
            continue
        i += 1
    return "".join(out)


@lru_cache(maxsize=64)
def scrub_rust(src: str) -> str:
    """Scrub small AST fragments without evicting module-sized source entries."""
    return _scrub_rust(src)


@lru_cache(maxsize=16)
def scrub_source(src: str) -> str:
    """Scrub complete source modules in a cache isolated from fragment churn."""
    return _scrub_rust(src)


def matching_delimiter(src: str, start: int) -> int:
    pairs = {"{": "}", "(": ")", "[": "]", "<": ">"}
    opener = src[start]
    if opener not in pairs:
        raise ValueError(f"not a delimiter at {start}: {opener!r}")
    closer = pairs[opener]
    depth = 0
    for index in range(start, len(src)):
        char = src[index]
        if char == opener:
            depth += 1
        elif char == closer:
            depth -= 1
            if depth == 0:
                return index
    raise SystemExit(f"unbalanced delimiter {opener!r} at offset {start}")


def split_top_level(src: str, separator: str = ",") -> list[str]:
    parts: list[str] = []
    start = 0
    stack: list[str] = []
    pairs = {"{": "}", "(": ")", "[": "]", "<": ">"}
    closers = set(pairs.values())
    for index, char in enumerate(src):
        if char in pairs:
            stack.append(pairs[char])
        elif char in closers:
            if stack and char == stack[-1]:
                stack.pop()
        elif char == separator and not stack:
            parts.append(src[start:index])
            start = index + 1
    parts.append(src[start:])
    return parts


def top_level_colon(src: str) -> int:
    stack: list[str] = []
    pairs = {"{": "}", "(": ")", "[": "]", "<": ">"}
    closers = set(pairs.values())
    for index, char in enumerate(src):
        if char in pairs:
            stack.append(pairs[char])
        elif char in closers:
            if stack and char == stack[-1]:
                stack.pop()
        elif char == ":" and not stack:
            return index
    return -1


@lru_cache(maxsize=32)
def enum_variants(src: str, enum: str) -> dict[str, list[tuple[str, str]]]:
    """Structurally parse brace/tuple variants without first-`}` regex truncation."""
    clean = scrub_rust(src)
    match = re.search(r"\bpub\s+enum\s+" + re.escape(enum) + r"\b", clean)
    if not match:
        return {}
    opening = clean.find("{", match.end())
    if opening < 0:
        raise SystemExit(f"enum `{enum}` has no body")
    closing = matching_delimiter(clean, opening)
    body = src[opening + 1 : closing]
    clean_body = clean[opening + 1 : closing]
    out: dict[str, list[tuple[str, str]]] = {}
    offset = 0
    for clean_segment in split_top_level(clean_body):
        segment = body[offset : offset + len(clean_segment)]
        offset += len(clean_segment) + 1
        name_match = re.search(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", clean_segment)
        if not name_match:
            continue
        name = name_match.group(1)
        cursor = name_match.end()
        while cursor < len(clean_segment) and clean_segment[cursor].isspace():
            cursor += 1
        fields: list[tuple[str, str]] = []
        if cursor < len(clean_segment) and clean_segment[cursor] in "{(":
            end = matching_delimiter(clean_segment, cursor)
            inner_clean = clean_segment[cursor + 1 : end]
            inner = segment[cursor + 1 : end]
            inner_offset = 0
            entries = split_top_level(inner_clean)
            for index, clean_entry in enumerate(entries):
                entry = inner[inner_offset : inner_offset + len(clean_entry)]
                inner_offset += len(clean_entry) + 1
                if not clean_entry.strip():
                    continue
                if clean_segment[cursor] == "(":
                    fields.append((f"_{index}", entry.strip()))
                    continue
                colon = top_level_colon(clean_entry)
                if colon < 0:
                    continue
                left = clean_entry[:colon]
                field_names = re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", left)
                if not field_names:
                    continue
                fields.append((field_names[-1], entry[colon + 1 :].strip()))
        out[name] = fields
    return out


def holds_code(ty: str) -> bool:
    """A field that can contain an expression or statement — i.e. somewhere code can hide."""
    return "Expr" in ty or "Stmt" in ty or "MatchArm" in ty or "ForSource" in ty


@lru_cache(maxsize=32)
def _read_source(content: bytes) -> str:
    """Decode bytes already read from disk; cache identity is the content itself."""
    return content.decode("utf-8")


def read_source(path: Path) -> str:
    return _read_source(path.read_bytes())


@lru_cache(maxsize=128)
def function_definitions(src: str, fn: str) -> tuple[tuple[int, int], ...]:
    """Return `(fn_keyword_start, params_open)` for structural Rust definitions."""
    clean = scrub_source(src)
    marker_pattern = re.compile(r"\bfn\s+" + re.escape(fn) + r"\b")
    definitions: list[tuple[int, int]] = []
    for marker in marker_pattern.finditer(clean):
        cursor = marker.end()
        while cursor < len(clean) and clean[cursor].isspace():
            cursor += 1
        if cursor < len(clean) and clean[cursor] == "<":
            cursor = matching_delimiter(clean, cursor) + 1
            while cursor < len(clean) and clean[cursor].isspace():
                cursor += 1
        if cursor < len(clean) and clean[cursor] == "(":
            definitions.append((marker.start(), cursor))
    return tuple(definitions)


def source_for(fn: str) -> str:
    """The file that structurally defines `fn` — MID first, then sibling modules."""
    # Qualified form `module::fn` disambiguates. THREE sibling modules each define `walk_expr`,
    # so an unqualified lookup would silently pick whichever file happened to be searched first —
    # a gate quietly grading a different walker than the registry names. Refuse instead.
    if "::" in fn:
        modname, _, bare = fn.rpartition("::")
        for cand in [MID, *SIBLINGS]:
            if not cand.is_file() or cand.stem != modname:
                continue
            source = read_source(cand)
            if function_definitions(source, bare):
                return source
        raise SystemExit(f"walker `{fn}`: no module `{modname}` structurally defining `{bare}`")

    hits: list[tuple[Path, str]] = []
    for candidate in [MID, *SIBLINGS]:
        if not candidate.is_file():
            continue
        source = read_source(candidate)
        if function_definitions(source, fn):
            hits.append((candidate, source))
    if len(hits) > 1:
        names = ", ".join(f"{path.stem}::{fn}" for path, _ in hits)
        raise SystemExit(
            f"walker `{fn}` is AMBIGUOUS across {len(hits)} modules — qualify it: {names}"
        )
    if not hits:
        raise SystemExit(f"walker `{fn}` not found in {MID} or {[str(x) for x in SIBLINGS]}")
    return hits[0][1]


@lru_cache(maxsize=128)
def walker_body(src: str, fn: str) -> str:
    clean = scrub_source(src)
    depth = 0
    cursor = 0
    definitions: list[tuple[int, int, int]] = []
    for start, params_open in function_definitions(src, fn):
        for char in clean[cursor:start]:
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
        cursor = start
        definitions.append((start, params_open, depth))
    if not definitions:
        raise SystemExit(f"walker `{fn}` not found")
    top_level = [definition for definition in definitions if definition[2] == 0]
    if len(top_level) == 1:
        start, params_open, _ = top_level[0]
    elif len(top_level) > 1:
        raise SystemExit(
            f"walker `{fn}` is AMBIGUOUS: {len(top_level)} module-level definitions"
        )
    elif len(definitions) == 1:
        start, params_open, _ = definitions[0]
    else:
        raise SystemExit(
            f"walker `{fn}` is AMBIGUOUS: {len(definitions)} nested definitions and no module-level owner"
        )
    params_close = matching_delimiter(clean, params_open)
    opening = clean.find("{", params_close + 1)
    if opening < 0:
        raise SystemExit(f"walker `{fn}` has no body")
    closing = matching_delimiter(clean, opening)
    return src[start : closing + 1]


def arm_arrow(clean: str, pattern_close: int) -> int | None:
    """Find this pattern's top-level `=>`, refusing constructor/body occurrences."""
    stack: list[str] = []
    pairs = {"{": "}", "(": ")", "[": "]"}
    closers = set(pairs.values())
    index = pattern_close + 1
    while index < len(clean):
        if not stack and clean.startswith("=>", index):
            return index
        char = clean[index]
        if char in pairs:
            stack.append(pairs[char])
        elif char in closers:
            if stack and char == stack[-1]:
                stack.pop()
            elif not stack:
                return None
        elif not stack and char in ",;":
            return None
        index += 1
    return None


def arm_value_end(clean: str, start: int) -> int:
    """Return the exclusive end of one match-arm value at top level."""
    if start < len(clean) and clean[start] == "{":
        return matching_delimiter(clean, start) + 1
    stack: list[str] = []
    pairs = {"{": "}", "(": ")", "[": "]"}
    closers = set(pairs.values())
    index = start
    while index < len(clean):
        char = clean[index]
        if char in pairs:
            stack.append(pairs[char])
        elif char in closers:
            if stack and char == stack[-1]:
                stack.pop()
            elif not stack:
                return index
        elif char == "," and not stack:
            return index
        index += 1
    return len(clean)


def split_top_level_alternatives(pattern: str) -> list[str]:
    """Split a Rust pattern on root-level `|`, preserving nested OR patterns."""
    parts: list[str] = []
    stack: list[str] = []
    pairs = {"{": "}", "(": ")", "[": "]"}
    start = 0
    for index, char in enumerate(pattern):
        if char in pairs:
            stack.append(pairs[char])
        elif stack and char == stack[-1]:
            stack.pop()
        elif char == "|" and not stack:
            parts.append(pattern[start:index])
            start = index + 1
    parts.append(pattern[start:])
    return parts


def root_enum_variants(pattern: str, enum_name: str) -> set[str]:
    """Return enum variants named at the root of an arm pattern."""
    variants: set[str] = set()
    enum = re.escape(enum_name)
    root = re.compile(
        rf"^\s*(?:(?:&\s*(?:mut\s+)?)|(?:ref\s+)|(?:box\s+))*"
        rf"(?:(?:[A-Za-z_][A-Za-z0-9_]*)\s*::\s*)*"
        rf"{enum}\s*::\s*([A-Za-z_][A-Za-z0-9_]*)\b"
    )
    for alternative in split_top_level_alternatives(scrub_rust(pattern)):
        match = root.match(alternative)
        if match:
            variants.add(match.group(1))
    return variants


def immediate_match_arms(
    body: str, clean: str, opening: int, closing: int
) -> list[tuple[str, str, bool, str]]:
    """Parse immediate arms of one structurally bounded match expression."""
    found: list[tuple[str, str, bool, str]] = []
    cursor = opening + 1
    while cursor < closing:
        while cursor < closing and (clean[cursor].isspace() or clean[cursor] == ","):
            cursor += 1
        if cursor >= closing:
            break
        arrow = arm_arrow(clean, cursor - 1)
        if arrow is None or arrow >= closing:
            break
        pattern = body[cursor:arrow]
        value = arrow + 2
        while value < closing and clean[value].isspace():
            value += 1
        end = min(arm_value_end(clean, value), closing)
        terminal = clean.startswith("true", value) and (
            value + 4 == len(clean)
            or not (clean[value + 4].isalnum() or clean[value + 4] == "_")
        )
        if terminal:
            after = value + 4
            while after < closing and clean[after].isspace():
                after += 1
            terminal = after >= closing or clean[after] in ",}"
        found.append(("", pattern, terminal, body[value:end]))
        if end <= cursor:
            break
        cursor = end
    return found


@lru_cache(maxsize=128)
def enum_dispatch_arms(body: str, enum_name: str) -> tuple[tuple[str, str, bool, str], ...]:
    """Return immediate arms from outermost match expressions dispatching on one enum."""
    clean = scrub_rust(body)
    matches: list[tuple[int, int, list[tuple[str, str, bool, str]]]] = []
    for marker in re.finditer(r"\bmatch\b", clean):
        cursor = marker.end()
        stack: list[str] = []
        opening: int | None = None
        while cursor < len(clean):
            char = clean[cursor]
            if char in "([":
                stack.append(")" if char == "(" else "]")
            elif stack and char == stack[-1]:
                stack.pop()
            elif char == "{" and not stack:
                opening = cursor
                break
            elif char in ";" and not stack:
                break
            cursor += 1
        if opening is None:
            continue
        closing = matching_delimiter(clean, opening)
        arms = immediate_match_arms(body, clean, opening, closing)
        matches.append((marker.start(), closing + 1, arms))

    outermost: list[tuple[int, int, list[tuple[str, str, bool, str]]]] = []
    for candidate in matches:
        start, end, _ = candidate
        if any(parent_start < start and end <= parent_end for parent_start, parent_end, _ in matches):
            continue
        outermost.append(candidate)

    owned: list[tuple[str, str, bool, str]] = []
    for _, _, arms in outermost:
        if not any(root_enum_variants(pattern, enum_name) for _, pattern, _, _ in arms):
            continue
        for arm in arms:
            variants = root_enum_variants(arm[1], enum_name)
            if variants:
                owned.append(arm)
    return tuple(owned)


@lru_cache(maxsize=512)
def variant_arms(body: str, vname: str) -> list[tuple[str, str, bool, str]]:
    """Return owned immediate match arms for one exact enum variant."""
    parts = vname.split("::")
    if len(parts) != 2:
        return []
    enum_name, variant_name = parts
    found: list[tuple[str, str, bool, str]] = []
    for _, pattern, terminal, value in enum_dispatch_arms(body, enum_name):
        if variant_name not in root_enum_variants(pattern, enum_name):
            continue
        variant_match = re.search(
            rf"(?:^|\|)\s*(?:(?:&\s*(?:mut\s+)?)|(?:ref\s+)|(?:box\s+))*"
            rf"(?:(?:[A-Za-z_][A-Za-z0-9_]*)\s*::\s*)*"
            rf"{re.escape(enum_name)}\s*::\s*{re.escape(variant_name)}\b",
            scrub_rust(pattern),
        )
        if variant_match is None:
            continue
        cursor = variant_match.end()
        while cursor < len(pattern) and pattern[cursor].isspace():
            cursor += 1
        if cursor < len(pattern) and pattern[cursor] in "{(":
            closing = matching_delimiter(scrub_rust(pattern), cursor)
            found.append((pattern[cursor], pattern[cursor + 1 : closing], terminal, value))
        else:
            found.append(("", "", terminal, value))
    return found


@lru_cache(maxsize=512)
def variant_patterns(body: str, vname: str) -> list[tuple[str, str, bool]]:
    """Backward-compatible pattern-only projection used by external self-tests."""
    return [(opener, pattern, terminal) for opener, pattern, terminal, _ in variant_arms(body, vname)]


def simple_pattern_bindings(pattern: str) -> set[str]:
    """Return the whole-value binding for a simple Rust pattern, never path/field tokens."""
    clean = scrub_rust(pattern).strip()
    if clean in {"", "_", ".."}:
        return set()
    match = re.fullmatch(
        r"(?:(?:ref\s+)?mut\s+|ref\s+|box\s+)?"
        r"([a-z_][A-Za-z0-9_]*)"
        r"(?:\s*@\s*.+)?",
        clean,
        re.S,
    )
    return {match.group(1)} if match else set()


def named_pattern_bindings(inner: str, field: str) -> set[str]:
    """Return identifiers bound to a named field; wildcards bind nothing."""
    clean = scrub_rust(inner)
    offset = 0
    for clean_entry in split_top_level(clean):
        entry = inner[offset : offset + len(clean_entry)]
        offset += len(clean_entry) + 1
        stripped = clean_entry.strip()
        if not stripped or stripped == "..":
            continue
        colon = top_level_colon(clean_entry)
        if colon < 0:
            bindings = simple_pattern_bindings(entry)
            if field in bindings:
                return bindings
            continue
        left = re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", clean_entry[:colon])
        if not left or left[-1] != field:
            continue
        return simple_pattern_bindings(entry[colon + 1 :])
    return set()


def tuple_pattern_bindings(inner: str, index: int) -> set[str]:
    entries = split_top_level(scrub_rust(inner))
    if any(entry == ".." for entry in entries) or index >= len(entries):
        return set()
    return simple_pattern_bindings(entries[index])


PATTERN_TERMINAL_DISPOSITION = "__ANUBIS_PATTERN_TERMINAL_DISPOSITION__"


def field_subpattern(opener: str, inner: str, field: str, index: int) -> str | None:
    if opener == "(":
        entries = split_top_level(scrub_rust(inner))
        return entries[index].strip() if index < len(entries) else None
    clean = scrub_rust(inner)
    offset = 0
    for clean_entry in split_top_level(clean):
        entry = inner[offset : offset + len(clean_entry)]
        offset += len(clean_entry) + 1
        if not clean_entry.strip() or clean_entry.strip() == "..":
            continue
        colon = top_level_colon(clean_entry)
        if colon < 0:
            if field in simple_pattern_bindings(entry):
                return entry.strip()
            continue
        left = re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", clean_entry[:colon])
        if left and left[-1] == field:
            return entry[colon + 1 :].strip()
    return None


def nested_pattern_requirements(pattern: str, field_type: str, ast: str) -> set[str]:
    expected = next(
        (
            name
            for name in ("Expr", "Stmt", "ForSource")
            if re.search(rf"\b{name}\b", field_type)
        ),
        None,
    )
    if expected is None:
        return set()
    clean = scrub_rust(pattern).strip()
    opener_positions = [pos for pos in (clean.find("{"), clean.find("(")) if pos >= 0]
    opening = min(opener_positions) if opener_positions else -1
    prefix = clean[:opening] if opening >= 0 else clean
    segments = re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", prefix)
    if len(segments) < 2 or segments[-2] != expected:
        return set()
    nested_fields = enum_variants(ast, expected).get(segments[-1])
    if nested_fields is None:
        return set()
    code_fields = [(name, ty) for name, ty in nested_fields if holds_code(ty)]
    if not code_fields:
        return {PATTERN_TERMINAL_DISPOSITION}
    if opening < 0:
        return set()
    closing = matching_delimiter(clean, opening)
    if clean[closing + 1 :].strip():
        return set()
    opener = clean[opening]
    inner = pattern[opening + 1 : closing]
    requirements: set[str] = set()
    for index, (name, nested_type) in enumerate(code_fields):
        # Tuple indices are positions in the full variant field list, not in the filtered code list.
        full_index = next(i for i, (field_name, _) in enumerate(nested_fields) if field_name == name)
        subpattern = field_subpattern(opener, inner, name, full_index)
        if not subpattern:
            return set()
        direct = simple_pattern_bindings(subpattern)
        if direct:
            requirements.update(direct)
            continue
        nested = nested_pattern_requirements(subpattern, nested_type, ast)
        if not nested:
            return set()
        requirements.update(nested)
    return requirements


def rust_pattern_bindings(pattern: str) -> set[str]:
    """Conservatively extract actual bindings from a Rust `let` pattern."""
    clean = scrub_rust(pattern).strip()
    if not clean or clean in {"_", ".."}:
        return set()
    alternatives = split_top_level_alternatives(clean)
    if len(alternatives) > 1:
        result: set[str] = set()
        for alternative in alternatives:
            result.update(rust_pattern_bindings(alternative))
        return result
    colon = top_level_colon(clean)
    if colon >= 0:
        # A top-level colon in a let pattern is a type annotation.
        return rust_pattern_bindings(clean[:colon])
    direct = simple_pattern_bindings(clean)
    if direct:
        return direct
    opening = next((index for index, char in enumerate(clean) if char in "{(["), -1)
    if opening < 0:
        return set()
    try:
        closing = matching_delimiter(clean, opening)
    except SystemExit:
        return set()
    if clean[closing + 1 :].strip():
        return set()
    opener = clean[opening]
    inner = clean[opening + 1 : closing]
    result: set[str] = set()
    for entry in split_top_level(inner):
        entry = entry.strip()
        if not entry or entry == "..":
            continue
        if opener == "{":
            field_colon = top_level_colon(entry)
            if field_colon >= 0:
                result.update(rust_pattern_bindings(entry[field_colon + 1 :]))
            else:
                result.update(rust_pattern_bindings(entry))
        else:
            result.update(rust_pattern_bindings(entry))
    return result


def let_shadow_span(clean: str, binding: str) -> tuple[int, int, int] | None:
    """Return `(let_start, initializer_start, statement_end)` for the first rebinding."""
    pairs = {"(": ")", "[": "]", "{": "}"}
    for marker in re.finditer(r"\blet\b", clean):
        stack: list[str] = []
        equals: int | None = None
        cursor = marker.end()
        while cursor < len(clean):
            char = clean[cursor]
            if char in pairs:
                stack.append(pairs[char])
            elif stack and char == stack[-1]:
                stack.pop()
            elif char == "=" and not stack:
                equals = cursor
                break
            elif char == ";" and not stack:
                break
            cursor += 1
        if equals is None:
            continue
        if binding not in rust_pattern_bindings(clean[marker.end() : equals]):
            continue
        stack = []
        statement_end = len(clean)
        for index in range(equals + 1, len(clean)):
            char = clean[index]
            if char in pairs:
                stack.append(pairs[char])
            elif stack and char == stack[-1]:
                stack.pop()
            elif char == ";" and not stack:
                statement_end = index
                break
        return marker.start(), equals + 1, statement_end
    return None


def binding_is_directly_used(arm_value: str, binding: str) -> bool:
    """A binding must survive removal of explicit discard-only statements."""
    clean = scrub_rust(arm_value)
    escaped = re.escape(binding)
    shadow = let_shadow_span(clean, binding)
    if shadow:
        shadow_start, initializer_start, initializer_end = shadow
        clean = clean[:shadow_start] + " " + clean[initializer_start:initializer_end]
    discard_patterns = (
        rf"(?m)(?:^|[{{;}}])\s*\b{escaped}\b\s*;",
        rf"(?m)(?:^|[{{;}}])\s*\b{escaped}\b\s*\.\s*clone\s*\(\s*\)\s*;",
        rf"\blet\s+_[A-Za-z0-9_]*(?:\s*:\s*[^=;]+)?\s*=\s*"
        rf"(?:&\s*(?:mut\s+)?|\*\s*)?\b{escaped}\b"
        rf"(?:\s*\.\s*clone\s*\(\s*\))?\s*;",
        rf"\blet\s+(?:(?:ref|mut)\s+)?\b{escaped}\b(?:\s*:\s*[^=;]+)?\s*=\s*[^;]*;",
        rf"\blet\s+[a-z_][A-Za-z0-9_]*(?:\s*:\s*[^=;]+)?\s*=\s*"
        rf"(?:[A-Z][A-Za-z0-9_]*\s*::\s*)*[A-Z][A-Za-z0-9_]*\s*\{{"
        rf"[^;]*\b{escaped}\b[^;]*\}}\s*;",
        rf"\blet\s+[a-z_][A-Za-z0-9_]*(?:\s*:\s*[^=;]+)?\s*=\s*"
        rf"[A-Z][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*\s*\("
        rf"[^;]*\b{escaped}\b[^;]*\)\s*;",
        rf"\b(?:(?:std|core)\s*::\s*mem\s*::\s*)?drop(?:\s*::\s*<[^;()]*>)?\s*\("
        rf"\s*(?:&\s*(?:mut\s+)?)?\b{escaped}\b(?:\s*\.\s*clone\s*\(\s*\))?\s*\)\s*;",
    )
    for pattern in discard_patterns:
        clean = re.sub(pattern, " ", clean)
    return re.search(rf"\b{escaped}\b", clean) is not None


def deferred_contract_problems(
    key: tuple[str, str, str], source: str
) -> list[str]:
    contract = DEFERRED_FIELDS[key]
    walker, variant, field = key
    prefix = f"walker={walker} variant={variant} field={field}"
    problems: list[str] = []
    if contract.kind not in DEFERRED_KINDS:
        problems.append(
            f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement=kind"
        )
    if not contract.rationale.strip():
        problems.append(
            f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement=rationale"
        )
    if not contract.consumer_patterns:
        problems.append(
            f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement=consumer_inventory"
        )
    if not contract.fallback_patterns:
        problems.append(
            f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement=fallback_inventory"
        )
    for label, owner, pattern in (*contract.consumer_patterns, *contract.fallback_patterns):
        try:
            owner_body = walker_body(source, owner)
        except SystemExit:
            problems.append(
                f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement={label} owner={owner}"
            )
            continue
        if not re.search(pattern, scrub_rust(owner_body), re.S):
            problems.append(
                f"WALKER_DEFERRED_CONTRACT_MISSING {prefix} requirement={label} owner={owner}"
            )
    return problems


def check(fn: str, scope: str = "all") -> list[str]:
    """`scope` selects which enums this walker is RESPONSIBLE for.

    Registering a second walker used to be impossible, and this is why: the check demanded every
    walker bind every code-holding field of BOTH `Stmt` and `Expr`. An expression-only query like
    `expr_taint_source_m` does not walk statements — that is its caller's job — so it scored 11
    `Stmt::* is never matched` problems that are not defects, drowning the one that was
    (`Expr::If never binds cond`). A gate whose output is mostly false positives gets one walker
    registered and then abandoned, which is exactly what happened.

    Scope is a claim about the walker's contract, not a way to silence it: `expr` still demands
    DIRECT or checked DEFERRED disposition of every code-holding `Expr` field.
    """
    ast = read_source(AST)
    # `source_for` resolves a qualified `module::fn`; the body lookup needs the BARE name.
    source = source_for(fn)
    body = walker_body(source, fn.rpartition('::')[2])
    variants: dict = {}
    base = scope[len("partial-"):] if scope.startswith("partial-") else scope
    if base in ("all", "stmt"):
        variants.update({f"Stmt::{k}": v for k, v in enum_variants(ast, "Stmt").items()})
    if base in ("all", "expr"):
        variants.update({f"Expr::{k}": v for k, v in enum_variants(ast, "Expr").items()})
    if base in ("all", "pattern"):
        variants.update({f"Pattern::{k}": v for k, v in enum_variants(ast, "Pattern").items()})
    if not variants:
        raise SystemExit(
            f"unknown scope `{scope}` for `{fn}` (use all|expr|stmt|pattern, optionally partial- prefixed)"
        )
    # `partial-` = the contract for a SPECIALISED walker: it need not match every variant, but
    # every variant it DOES match must dispose all that variant's code-holding fields.
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
    used_deferred: set[tuple[str, str, str]] = set()
    matched_code_arms = 0
    for vname, fields in variants.items():
        code_fields = [(f, t) for f, t in fields if holds_code(t)]
        if not code_fields:
            continue
        arms = variant_arms(body, vname)
        if not arms:
            if partial:
                continue
            problems.append(
                f"WALKER_VARIANT_UNMATCHED walker={fn} variant={vname} — "
                f"{fn}: {vname} is never matched (holds {[f for f, _ in code_fields]})"
            )
            continue
        matched_code_arms += len(arms)

        # A literal-true arm has already answered a predicate walker's question. Exclude only that
        # exact arm; a terminal specialization must not excuse a different nonterminal arm that
        # discards the same field.
        nonterminal = [arm for arm in arms if not arm[2]]
        if not nonterminal:
            continue
        for field, field_type in code_fields:
            if field.startswith("_") and field[1:].isdigit():
                index = int(field[1:])
            else:
                index = -1
            bindings = []
            for arm in nonterminal:
                if arm[0] == "(":
                    names = tuple_pattern_bindings(arm[1], index) if index >= 0 else set()
                else:
                    names = named_pattern_bindings(arm[1], field)
                if not names:
                    subpattern = field_subpattern(arm[0], arm[1], field, index)
                    if subpattern:
                        names = nested_pattern_requirements(subpattern, field_type, ast)
                bindings.append(names)
            missing = [arm for arm, names in zip(nonterminal, bindings) if not names]
            if missing:
                problems.append(
                    f"WALKER_UNBOUND_FIELD walker={fn} variant={vname} field={field} "
                    f"arms={len(missing)} — {fn}: {vname} never binds `{field}` in "
                    f"{len(missing)} nonterminal arm(s); code can hide there "
                    "(a `..` or `_` is discarding it)"
                )
                continue
            undisposed = [
                arm
                for arm, names in zip(nonterminal, bindings)
                if not all(
                    name == PATTERN_TERMINAL_DISPOSITION
                    or binding_is_directly_used(arm[3], name)
                    for name in names
                )
            ]
            if undisposed:
                key = (fn, vname, field)
                if key in DEFERRED_FIELDS:
                    used_deferred.add(key)
                    problems.extend(deferred_contract_problems(key, source))
                else:
                    problems.append(
                        f"WALKER_UNDISPOSED_FIELD walker={fn} variant={vname} field={field} "
                        f"arms={len(undisposed)} — field is bound but has no DIRECT use or DEFERRED contract"
                    )
    if matched_code_arms == 0:
        # Scope availability is not evidence that the checker has a truthful field model for it.
        # Pattern is intentionally accepted by the CLI for future registrations, but recursive
        # Pattern fields and scalar match identity are not yet classified by holds_code().  Before
        # this floor existed, a full `:pattern` registration therefore reported PASS without
        # grading a single arm, while its `:partial-pattern` twin correctly failed as vacuous.
        # Refuse both shapes until the scope has at least one code-bearing arm under the current
        # classifier; an empty registry cell must never count as coverage.
        marker = "WALKER_PARTIAL_VACUOUS" if partial else "WALKER_SCOPE_VACUOUS"
        problems.append(f"{marker} walker={fn} scope={scope} matched_code_arms=0")
    for key in sorted(DEFERRED_FIELDS):
        if key[0] == fn and key not in used_deferred:
            problems.append(
                f"WALKER_DEFERRED_CONTRACT_UNUSED walker={key[0]} variant={key[1]} field={key[2]}"
            )
    return problems


def unregistered_deferred_problems(registered_walkers: set[str]) -> list[str]:
    return [
        f"WALKER_DEFERRED_CONTRACT_UNREGISTERED walker={walker} variant={variant} field={field}"
        for walker, variant, field in sorted(DEFERRED_FIELDS)
        if walker not in registered_walkers
    ]


def main() -> int:
    # Each argument is `name` or `name:scope`; scopes cover all/expr/stmt/pattern and optional
    # `partial-` specialization. `--require-all-deferred` binds the canonical registry to every
    # deferred contract while leaving ad hoc one-walker probes usable.
    args = sys.argv[1:]
    require_all_deferred = "--require-all-deferred" in args
    walkers = [arg for arg in args if arg != "--require-all-deferred"] or [
        "body_has_mode_elevator"
    ]
    all_problems: list[str] = []
    registered_walkers: set[str] = set()
    for spec in walkers:
        # Split on a trailing SCOPE only. `partition(":")` split inside the `::` of a qualified
        # name like `effects::walk_expr`, silently turning the module into the walker name.
        # Keep Pattern scope available for truthful future registrations. The non-vacuity floor
        # rejects both full and partial scopes that match zero code-bearing arms, so availability
        # itself cannot be mistaken for effective coverage.
        SCOPES = {
            "all", "expr", "stmt", "pattern",
            "partial-all", "partial-expr", "partial-stmt", "partial-pattern",
        }
        w, scope = spec, "all"
        if ":" in spec:
            head, _, tail = spec.rpartition(":")
            if tail in SCOPES and head:
                w, scope = head, tail
        registered_walkers.add(w)
        p = check(w, scope)
        all_problems += p
        label = w if scope == "all" else f"{w} [{scope}]"
        print(f"{label}: {'OK' if not p else str(len(p)) + ' PROBLEM(S)'}")
        for x in p:
            print(f"  {x}")
    if require_all_deferred:
        registry_problems = unregistered_deferred_problems(registered_walkers)
        all_problems.extend(registry_problems)
        for problem in registry_problems:
            print(f"  {problem}")
    if all_problems:
        print(f"\nWALKER_COMPLETENESS: FAIL ({len(all_problems)})")
        return 1
    print("\nWALKER_COMPLETENESS: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
