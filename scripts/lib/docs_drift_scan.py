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


# Root-relative "SPEC_1_0_FREEZE.md" and "TUTORIAL.md" were removed 2026-07-28. Neither file has
# ever existed at the repository root, so `scan()` skipped each with a bare `continue` on every run
# — two of fifteen declared owned docs silently absent, which is precisely the vacuous-coverage
# shape the harness-integrity audit named. Their real locations under docs/language/ are listed and
# were always the ones actually scanned; deleting the dead entries removes the illusion of coverage
# without changing a single measured stamp.
LIVE_FILES = [
    "AGENTS.md",
    "README.md",
    "LANGUAGE.md",
    "docs/README.md",
    "docs/CAPABILITIES.md",
    "docs/EXAMPLES.md",
    "docs/CLAIMS.md",
    "docs/language/BUILTINS.md",
    "docs/language/ROADMAP.md",
    "docs/language/STDLIB_CORE.md",
    "MATURITY_CLAIM_MATRIX.md",
    "docs/language/SPEC_1_0_FREEZE.md",
    "A_PLUS_ACCEPTANCE_CRITERIA.md",
    "docs/history/A_PLUS_CLOSEOUT.md",
    "docs/history/A_PLUS_FINAL_REPORT.md",
    "docs/language/TUTORIAL.md",
]

# Numeric live stamps predate the semantic-claim sweep and intentionally remain limited to the
# canonical live status/spec surfaces. Archive-heavy roadmap/matrix/report files are claim-scanned,
# but their dated historical N/N rows are not reinterpreted as live stamps.
#
# docs/CAPABILITIES.md joined 2026-07-28: the capability matrix moved out of README.md and carries
# the live Lean, stdlib-module and builtin counts with it. Registering it here is what keeps those
# stamps measured — a doc that is not in this set is not protected from drift, so an extraction that
# skipped this step would have quietly lowered the ratchet instead of relocating coverage.
LIVE_STAMP_FILES = {
    "AGENTS.md",
    "README.md",
    "LANGUAGE.md",
    "docs/CAPABILITIES.md",
    "docs/CLAIMS.md",
    "docs/language/BUILTINS.md",
    "docs/language/STDLIB_CORE.md",
}

# A line is exempt only if it disclaims being a CURRENT claim.
#
# "as of <date>" was removed 2026-07-28. It does not disclaim anything — it is how this project
# writes a LIVE stamp, and it silently exempted every line that used it. LANGUAGE.md:546 read
# "Gate: bash scripts/run_stdlib_failclosed_gate.sh (**86/86** as of 2026-07-27)" — a line that
# names a gate you can run right now — while the corpus on disk was 104 and the gate measured
# 104/104. The number was 18 short and the drift gate was green, because the scanner read the
# phrase as "historical" and skipped it.
#
# Everything retained below says, unambiguously, "this WAS true, not IS true". Adding a marker
# here is adding a way for a wrong number to stay green: require that the phrase be meaningless
# on a line making a present-tense claim.
# A measurement bound to a NAMED PIN is a record of that artifact, not a claim about the tree.
#
# This is admitted where "as of <date>" was refused, and the difference is the whole point: a date
# does not say WHAT was measured, so a stale number under one stays wrong and unfalsifiable. A
# content-addressed pin says exactly which binary produced the number, and anyone can check it by
# re-running that pin. `anubis-cf98ccebb4c1` measured 311/311 and always will; rewriting it to
# today's 317/317 would not be refreshing a stamp, it would be falsifying a record.
#
# The exemption is deliberately tied to the 12-hex pin form and nothing looser. A line claiming to
# describe "the pinned binary" WITHOUT naming which pin gets no exemption — it is unfalsifiable in
# the same way "as of" was, and the fix is to name the pin.
PIN_BOUND = re.compile(r"anubis-[0-9a-f]{12}")

DATED_LINE = re.compile(
    r"seal date|seal-date|historical|snapshot of|on this seal|"
    r"at that seal|prior Phase|was true|dated seal|seal_r8|historical stamp|"
    r"snapshot only|CLAIMED 20\d{2}|partial CLAIMED 20\d{2}|~~.*~~|"
    r"measured at this close|at the time of this close",
    re.I,
)

# Do-not-stamp / quoted ban exemplars are not themselves violations.
META_BAN_EXEMPT = re.compile(
    r"do not(\s|\*|_|`)*(?:stamp|claim|rewrite|publish)|deliberately not|found no way|"
    r"false as|marked \*\*FALSE|not claimed|stronger than that list|"
    r"not no defects|PARTIAL as total|not a total|"
    r"not a proof of total|not total language soundness|"
    r"does not mean every|does not mean no defects|cannot be read as",
    re.I,
)

# High-signal semantic claims only. A generic ban on words such as "always" and "never" produced
# policy/procedure false positives ("never use the bare alias") and was not maintainable. These
# patterns instead target the exact proof/soundness overclaims that can silently outrun evidence.
ABSOLUTE_CLAIM_PATTERNS = [
    (
        "check-run-invariant",
        re.compile(
            r"green `?anubis check`? never certifies a contract that `?anubis run`? violates",
            re.I,
        ),
    ),
    (
        "absolute-check-promise",
        re.compile(
            r"cannot violate (?:its|the program(?:'s)?) stated contracts, effects, capabilities, "
            r"or information-flow policy at runtime",
            re.I,
        ),
    ),
    (
        "privacy-absolute",
        re.compile(
            r"secret bits never leave|guarantees? (?:that )?nothing private leaves|"
            r"private (?:data|bits?) (?:can(?:not|'t)|never) (?:leave|escape)",
            re.I,
        ),
    ),
    (
        "fails-closed-everywhere",
        re.compile(r"fails? closed,? everywhere", re.I),
    ),
    (
        "totality-finality",
        re.compile(
            r"Safe(?:-mode)? (?:is )?total(?: IFC| information-flow)?|"
            r"(?:false-accept class|research elevation) closed forever|"
            r"roadmap soundness complete|100% secure|(?:^|\W)no defects(?:\W|$)",
            re.I,
        ),
    ),
    (
        "aggregate-proof",
        re.compile(
            r"every guarantee is (?:either )?proven(?:[- ]or[- ]|[- ])scoped|"
            r"no guarantee is overstated",
            re.I,
        ),
    ),
    (
        "approximate-walker-count",
        re.compile(r"~\s*\d+[^\n]{0,80}\bwalkers?\b", re.I),
    ),
]

FIXPOINT_SEAL = re.compile(
    r"(?:fixpoint[^\n]{0,80}(?:\bsealed\b|VM-sealed)|"
    r"(?:\bsealed\b|VM-sealed)[^\n]{0,80}fixpoint)",
    re.I,
)
FIXPOINT_EVIDENCE = re.compile(
    r"vm/pins/anubis-[0-9a-f]+|scripts/vm/EXPECTED_FIXPOINT_VM|"
    r"out/[^ )`]+|sha-?256",
    re.I,
)


def is_dated(line: str) -> bool:
    return bool(DATED_LINE.search(line)) or bool(PIN_BOUND.search(line))


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


def paragraph_context(lines: List[str], index: int) -> str:
    """Return a small prose paragraph around index; table rows remain isolated."""
    if lines[index].lstrip().startswith("|"):
        return lines[index]
    lo = index
    hi = index
    while lo > 0 and lines[lo - 1].strip() and not lines[lo - 1].lstrip().startswith("|"):
        lo -= 1
        if index - lo >= 4:
            break
    while hi + 1 < len(lines) and lines[hi + 1].strip() and not lines[hi + 1].lstrip().startswith("|"):
        hi += 1
        if hi - index >= 4:
            break
    return "\n".join(lines[lo : hi + 1])


def scan(root: Path, measured: Dict[str, int]) -> Tuple[List[str], int, int]:
    failures: List[str] = []
    stamps = 0
    claim_guards = 0

    for rel in LIVE_FILES:
        path = root / rel
        if not path.is_file():
            continue
        lines = path.read_text(errors="replace").splitlines()
        in_fence = False
        historical_section = False
        for i, line in enumerate(lines, 1):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            if rel == "docs/language/ROADMAP.md" and re.search(
                r"^## Earlier status —", line
            ):
                historical_section = True
            context = paragraph_context(lines, i - 1)
            stamp_dated = is_dated(line)
            claim_dated = historical_section or is_dated(context)

            if not stamp_dated and rel in LIVE_STAMP_FILES:
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

            # Absolute semantic claims. Exemptions must negate/scope the claim itself; merely linking
            # CLAIMS.md is intentionally insufficient because that used to suppress unsafe prose.
            if not claim_dated and not META_BAN_EXEMPT.search(context):
                for rule, pattern in ABSOLUTE_CLAIM_PATTERNS:
                    claim_guards += 1
                    if pattern.search(line):
                        failures.append(f"UNFALSIFIABLE_CLAIM {rel}:{i} {rule}")
                claim_guards += 1
                if FIXPOINT_SEAL.search(line) and not FIXPOINT_EVIDENCE.search(context):
                    failures.append(
                        f"UNFALSIFIABLE_CLAIM {rel}:{i} sealed-without-evidence-path"
                    )

    return failures, stamps, claim_guards


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
    failures, stamps, claim_guards = scan(root, measured)
    out = {
        "stamps_checked": stamps,
        "claim_guards_checked": claim_guards,
        "scan_failures": len(failures),
        "failures": failures,
        "measured": measured,
    }
    print(json.dumps(out, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
