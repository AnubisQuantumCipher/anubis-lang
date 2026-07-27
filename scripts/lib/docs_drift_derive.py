#!/usr/bin/env python3
"""Re-derive live quantitative claims for the docs drift gate.

Every quantity is produced by an explicit, pasteable method. The gate embeds
these methods so the report IS the documentation of how numbers are derived.

Builtins: LIVE five-function union over compiler/src/backends/run.rs (no cache file).
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, Dict, List, Tuple


def _run(root: Path, cmd: str) -> str:
    return subprocess.check_output(
        cmd, shell=True, text=True, cwd=str(root), stderr=subprocess.STDOUT
    ).strip()


def derive_security_fixtures(root: Path) -> Tuple[int, str]:
    cmd = "find examples/security -name '*.anb' | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def derive_language_fixtures(root: Path) -> Tuple[int, str]:
    cmd = "find tests/fixtures/language_core -name '*.anb' | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def derive_stdlib_failclosed(root: Path) -> Tuple[int, str]:
    cmd = "ls tests/fixtures/stdlib/*should_fail_closed.anb 2>/dev/null | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def derive_stdlib_doc_ok(root: Path) -> Tuple[int, str]:
    cmd = "ls tests/fixtures/stdlib/doc_ok/*.anb 2>/dev/null | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def derive_stdlib_modules(root: Path) -> Tuple[int, str]:
    cmd = "ls -1 compiler/stdlib/std/ | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def derive_native_corpus(root: Path) -> Tuple[int, str]:
    cmd = "find examples tests/fixtures -name '*.anb' | wc -l"
    return int(_run(root, cmd).split()[-1]), cmd


def _fn_body(src: str, fn_name: str) -> str:
    m = re.search(rf"(?:pub\s+)?fn {fn_name}\b", src)
    if not m:
        return ""
    brace = src.find("{", m.start())
    if brace < 0:
        return ""
    depth = 0
    j = brace
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[brace : j + 1]
        j += 1
    return src[brace:]


def _match_arm_and_matches_names(text: str) -> set:
    """Surface names only: match arms `"foo" =>` / `"a" | "b"`, and matches! lists.

    Deliberately does NOT take every quoted string (format! templates contain
    `anubis_*` helper ids that would inflate the count past 213).
    """
    names = set()
    names |= set(re.findall(r'"([a-z][a-z0-9_]*)"\s*=>', text))
    names |= set(re.findall(r'"([a-z][a-z0-9_]*)"\s*\|', text))
    names |= set(re.findall(r'\|\s*"([a-z][a-z0-9_]*)"', text))
    for block in re.findall(r"matches!\s*\([^)]*\)", text, re.S):
        names |= set(re.findall(r'"([a-z][a-z0-9_]*)"', block))
    return names


def _all_quoted_surface(text: str) -> set:
    """For small helper predicates: every "snake_case" string is a surface name."""
    return set(re.findall(r'"([a-z][a-z0-9_]*)"', text))


def derive_builtins(root: Path) -> Tuple[int, str]:
    """Live deduplicated union of five functions in run.rs (BUILTINS.md / AGENTS method).

    Never prefers a scratchpad cache — that would hide surface drift (the disease
    this gate exists to catch).
    """
    method = (
        "LIVE five-function union in compiler/src/backends/run.rs: "
        "emit_builtin_call (match arms + matches!), is_builtin_name, "
        "is_proof_input_builtin, is_poc_kit_builtin, is_non_run_builtin "
        "(surface names only — not format! helper ids)"
    )
    src_path = root / "compiler/src/backends/run.rs"
    if not src_path.is_file():
        return 0, method + " [MISSING run.rs]"
    src = src_path.read_text()

    union: set = set()
    # emit + is_builtin_name: arm/matches style (avoid anubis_* format strings)
    for fn in ("emit_builtin_call", "is_builtin_name"):
        union |= _match_arm_and_matches_names(_fn_body(src, fn))
    # small name sets: all quoted surface identifiers
    for fn in (
        "is_proof_input_builtin",
        "is_poc_kit_builtin",
        "is_non_run_builtin",
    ):
        union |= _all_quoted_surface(_fn_body(src, fn))

    # Drop pure noise that can appear as string literals in match contexts
    union -= {"true", "false", "main"}
    return len(union), method


def derive_lean(root: Path) -> Tuple[int, int, str]:
    """Comment-stripped theorem count and modules-with-theorems count."""
    method = (
        "for each formal/**/*.lean: strip /- ... -/ block comments; count lines matching "
        r"'^\s*theorem '; modules = files with ≥1 such theorem"
    )
    theorems = 0
    mods: set[str] = set()
    formal = root / "formal"
    if not formal.is_dir():
        return 0, 0, method
    for p in formal.rglob("*.lean"):
        text = p.read_text(errors="replace")
        no_block = re.sub(r"/\-.*?\-/", "", text, flags=re.S)
        hit = False
        for line in no_block.splitlines():
            if re.match(r"^\s*theorem\s", line):
                theorems += 1
                hit = True
        if hit:
            mods.add(str(p.relative_to(root)))
    return theorems, len(mods), method


DERIVERS: Dict[str, Callable[[Path], Any]] = {
    "security_fixtures": derive_security_fixtures,
    "language_fixtures": derive_language_fixtures,
    "stdlib_failclosed": derive_stdlib_failclosed,
    "stdlib_doc_ok": derive_stdlib_doc_ok,
    "stdlib_modules": derive_stdlib_modules,
    "native_corpus": derive_native_corpus,
    "builtins": derive_builtins,
}


def derive_all(root: Path) -> Dict[str, Any]:
    out: Dict[str, Any] = {"quantities": {}, "lean": {}}
    for key, fn in DERIVERS.items():
        val, cmd = fn(root)
        out["quantities"][key] = {"value": val, "command": cmd}
    th, mod, method = derive_lean(root)
    out["lean"] = {
        "theorems": th,
        "modules": mod,
        "command": method,
    }
    out["quantities"]["lean_theorems"] = {"value": th, "command": method}
    out["quantities"]["lean_modules"] = {"value": mod, "command": method}
    return out


def main(argv: List[str]) -> int:
    root = Path(argv[1] if len(argv) > 1 else ".").resolve()
    print(json.dumps(derive_all(root), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
