#!/usr/bin/env python3
"""Scan owned live docs for stamp drift and absolute unfalsifiable phrases.

LIVE undated present-tense stamps must match re-derived quantities.
DATED / historical lines are exempt.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Tuple


LIVE_FILES = [
    "AGENTS.md",
    "Agents.md",
    "README.md",
    "LANGUAGE.md",
    "docs/CLAIMS.md",
    "docs/language/BUILTINS.md",
    "docs/language/STDLIB_CORE.md",
]

DATED_LINE = re.compile(
    r"as of |as-of|seal date|seal-date|historical|snapshot of|on this seal|"
    r"at that seal|prior Phase|was true|dated seal|seal_r8|historical stamp|"
    r"snapshot only|CLAIMED 20\d{2}|partial CLAIMED 20\d{2}|~~.*~~",
    re.I,
)

# Do-not-stamp / quoted ban exemplars are not themselves violations.
META_BAN_EXEMPT = re.compile(
    r"do not(\s|\*|_|`)*stamp|do not(\s|\*|_|`)*rewrite|deliberately not|found no way|"
    r"false as|marked \*\*FALSE|stronger than that list|"
    r"CLAIMS\.md|Open — load-bearing|no KNOWN defects|not no defects|"
    r"named residual|bounded residual|PARTIAL as total|not a total|"
    r"not a proof of total|empty published residual",
    re.I,
)


def is_dated(line: str) -> bool:
    return bool(DATED_LINE.search(line))


def extract_pair_after(line: str, keyword: re.Pattern[str]) -> str | None:
    """Find first N/N after keyword match."""
    m = keyword.search(line)
    if not m:
        return None
    rest = line[m.end() :]
    p = re.search(r"(\d+)/(\d+)", rest)
    if not p:
        return None
    return p.group(0)


def scan(root: Path, measured: Dict[str, int]) -> Tuple[List[str], int]:
    failures: List[str] = []
    stamps = 0

    for rel in LIVE_FILES:
        path = root / rel
        if not path.is_file():
            continue
        lines = path.read_text(errors="replace").splitlines()
        for i, line in enumerate(lines, 1):
            dated = is_dated(line)

            if not dated:
                # security N/N (keyword-local)
                pair = extract_pair_after(
                    line, re.compile(r"security(\s+fixtures)?", re.I)
                )
                if pair:
                    stamps += 1
                    left, right = pair.split("/")
                    exp = measured["security"]
                    if int(left) != exp or int(right) != exp:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} security claimed {pair} "
                            f"measured {exp}/{exp}"
                        )

                # Live disk inventory **N**
                m = re.search(r"(?:Live )?disk inventory\s+\*\*(\d+)\*\*", line, re.I)
                if m:
                    stamps += 1
                    if int(m.group(1)) != measured["security"]:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} security_disk claimed {m.group(1)} "
                            f"measured {measured['security']}"
                        )

                # language N/N
                pair = extract_pair_after(
                    line, re.compile(r"language(\s+core)?", re.I)
                )
                if pair and "programming language" not in line.lower():
                    stamps += 1
                    left, right = pair.split("/")
                    exp = measured["language"]
                    if int(left) != exp or int(right) != exp:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} language claimed {pair} "
                            f"measured {exp}/{exp}"
                        )

                # stdlib fail-closed N/N
                pair = None
                if re.search(r"stdlib.*fail[- ]?closed|fail[- ]?closed.*stdlib|failclosed_gate", line, re.I):
                    pair = extract_pair_after(line, re.compile(r"stdlib|fail[- ]?closed|failclosed", re.I))
                elif re.search(r"Stdlib fail-closed|stdlib fail-closed", line, re.I):
                    pair = extract_pair_after(line, re.compile(r"fail-closed|Stdlib", re.I))
                # LANGUAGE/BUILTINS form: failclosed_gate.sh → **45/45**
                if re.search(r"run_stdlib_failclosed_gate", line):
                    p = re.search(r"(\d+)/(\d+)", line)
                    if p:
                        pair = p.group(0)
                if pair:
                    stamps += 1
                    left, right = pair.split("/")
                    exp = measured["stdlib"]
                    if int(left) != exp or int(right) != exp:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} stdlib_failclosed claimed {pair} "
                            f"measured {exp}/{exp}"
                        )

                # native-authoritative **N files** / PASS over N files
                m = re.search(
                    r"native[- ]authoritative[^\d]{0,40}\*\*(\d+)\s*files|"
                    r"native[- ]authoritative[^\d]{0,40}(\d+)\s*files|"
                    r"PASS over (\d+) files",
                    line,
                    re.I,
                )
                if m:
                    n = next(g for g in m.groups() if g)
                    stamps += 1
                    if int(n) != measured["native"]:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} native_corpus claimed {n} "
                            f"measured {measured['native']}"
                        )

                # builtins inventory (AGENTS: "Builtins are 213"; BUILTINS: Count: 213; etc.)
                m = re.search(
                    r"(?:\*\*)?Count:\s*(\d+)|"
                    r"Complete inventory\s*\((\d+)\)|"
                    r"Complete name count is\s*(\d+)|"
                    r"\*\*(\d+)\s*builtins\*\*|"
                    r"inventory\s*\((\d+)\)\s*—|"
                    r"Builtin inventory\s*\((\d+)\)|"
                    r"Builtins are\s+(\d+)|"
                    r"builtins are\s+(\d+)|"
                    r"~?(\d+)\s*builtins|"
                    r"builtin surface \(~?(\d+)\)|"
                    r"~(\d+)-builtin",
                    line,
                    re.I,
                )
                if m:
                    n = int(next(g for g in m.groups() if g))
                    if 100 <= n <= 400:
                        stamps += 1
                        if n != measured["builtins"]:
                            failures.append(
                                f"STAMP_DRIFT {rel}:{i} builtins claimed {n} "
                                f"measured {measured['builtins']}"
                            )

                # stdlib doc_ok count (e.g. "doc_ok/ 18 fixtures", "DOC_OK locks (18)")
                m = re.search(
                    r"doc_ok[^\d]{0,40}(\d+)\s*fixtures|"
                    r"DOC_OK[^\d]{0,40}(\d+)|"
                    r"(\d+)\s+doc_ok|"
                    r"stdlib/doc_ok[^\d]{0,30}(\d+)",
                    line,
                    re.I,
                )
                if m:
                    n = int(next(g for g in m.groups() if g))
                    if 1 <= n <= 200:
                        stamps += 1
                        if n != measured["doc_ok"]:
                            failures.append(
                                f"STAMP_DRIFT {rel}:{i} stdlib_doc_ok claimed {n} "
                                f"measured {measured['doc_ok']}"
                            )

                # stdlib module count (e.g. "13 content-locked ... modules", "13 modules")
                m = re.search(
                    r"(\d+)\s+content-locked[^\n]{0,40}modules|"
                    r"stdlib[^\n]{0,40}(\d+)\s+modules|"
                    r"(\d+)\s+Anubis-source modules|"
                    r"std/\):\s*\*\*(\d+)\s+modules\*\*|"
                    r"compiler/stdlib/std/[^\d]{0,20}(\d+)",
                    line,
                    re.I,
                )
                if m:
                    n = int(next(g for g in m.groups() if g))
                    if 5 <= n <= 50:
                        stamps += 1
                        if n != measured["modules"]:
                            failures.append(
                                f"STAMP_DRIFT {rel}:{i} stdlib_modules claimed {n} "
                                f"measured {measured['modules']}"
                            )

                # Lean: "162 Lean 4 theorems across 15 modules" or "162 theorems / 15 modules"
                m = re.search(
                    r"(\d+)\s+Lean\s+4\s+theorems\s+across\s+(\d+)\s+modules|"
                    r"Lean\s+\*\*(\d+)\s+theorems\s*/\s*(\d+)\s+modules\*\*|"
                    r"Lean\s*=\s*(\d+)\s*/\s*(\d+)|"
                    r"Lean is\s+(\d+)\s+theorems across\s+(\d+)\s+modules|"
                    r"(\d+)\s+theorems across\s+(\d+)\s+modules",
                    line,
                    re.I,
                )
                if m:
                    nums = [int(g) for g in m.groups() if g]
                    stamps += 1
                    if nums[0] != measured["lean_th"]:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} lean_theorems claimed {nums[0]} "
                            f"measured {measured['lean_th']}"
                        )
                    if len(nums) > 1 and nums[1] != measured["lean_mod"]:
                        failures.append(
                            f"STAMP_DRIFT {rel}:{i} lean_modules claimed {nums[1]} "
                            f"measured {measured['lean_mod']}"
                        )

            # Absolute phrases
            if not dated and not META_BAN_EXEMPT.search(line):
                if re.search(
                    r"cannot violate its stated contracts, effects, capabilities, "
                    r"or information-flow policy at runtime",
                    line,
                    re.I,
                ) and not re.search(r"found no way", line, re.I):
                    failures.append(
                        f"ABSOLUTE_PHRASE {rel}:{i} absolute-check-promise"
                    )
                if re.search(r"fails closed, everywhere, on purpose", line, re.I):
                    failures.append(
                        f"ABSOLUTE_PHRASE {rel}:{i} fails-closed-everywhere"
                    )
                if re.search(
                    r"Safe is total IFC|Safe-mode is total|"
                    r"research elevation closed forever|"
                    r"100% secure|roadmap soundness complete",
                    line,
                    re.I,
                ):
                    failures.append(f"ABSOLUTE_PHRASE {rel}:{i} totality-ban")
                if re.search(
                    r"self-host fixpoint sealed|\bfixpoint sealed\b", line, re.I
                ) and not re.search(
                    r"vm/pins/|sha256|publish_pin|EXPECTED_FIXPOINT", line, re.I
                ):
                    failures.append(
                        f"ABSOLUTE_PHRASE {rel}:{i} sealed-without-pin"
                    )

    return failures, stamps


def main(argv: List[str]) -> int:
    if len(argv) < 3:
        print("usage: docs_drift_scan.py ROOT DERIVED_JSON", file=sys.stderr)
        return 2
    root = Path(argv[1]).resolve()
    derived = json.loads(Path(argv[2]).read_text())
    q = derived["quantities"]
    measured = {
        "security": q["security_fixtures"]["value"],
        "language": q["language_fixtures"]["value"],
        "stdlib": q["stdlib_failclosed"]["value"],
        "doc_ok": q["stdlib_doc_ok"]["value"],
        "modules": q["stdlib_modules"]["value"],
        "native": q["native_corpus"]["value"],
        "builtins": q["builtins"]["value"],
        "lean_th": derived["lean"]["theorems"],
        "lean_mod": derived["lean"]["modules"],
    }
    failures, stamps = scan(root, measured)
    out = {
        "stamps_checked": stamps,
        "scan_failures": len(failures),
        "failures": failures,
        "measured": measured,
    }
    print(json.dumps(out, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
