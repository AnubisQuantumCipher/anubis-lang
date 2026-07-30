#!/usr/bin/env python3
"""Re-runnable verifier for the Phase-0 record-correction criteria.

This checks the live documentation plus the immutable commit evidence cited by
``docs/evidence/PHASE_0_COMPLETION_2026-07-30.md``. It is intentionally narrow: a
PASS means these ten record invariants hold, not that the repository is sealed.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Sequence, Tuple

UNIVERSAL = re.compile(
    r"`?(?:anubis\s+)?check`?\s+(?:PASS|passing)\s*"
    r"(?:⇒|=>|means(?:\s+that)?)\s+(?:the\s+)?program\s+cannot\s+violate",
    re.IGNORECASE,
)
STALE_PRESENT_TENSE = (
    re.compile(r"\bcurrent working tree\b", re.IGNORECASE),
    re.compile(r"\bstill uncommitted\b", re.IGNORECASE),
    re.compile(r"\bnot yet committed\b", re.IGNORECASE),
    re.compile(r"\bnot committed\b", re.IGNORECASE),
    re.compile(r"\bdoes not claim it as landed\b", re.IGNORECASE),
)
LIVE_BOARD_TOKENS = (
    "252/252",
    "327/327",
    "104/104",
    "766/766",
    "920 files",
    "916 files",
    "48 stamps",
)


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str = ""


def asserted_universal_matches(text: str) -> List[re.Match[str]]:
    return list(UNIVERSAL.finditer(text))


def stale_uncommitted_matches(text: str) -> List[Tuple[int, str]]:
    """Return present-tense landing claims across the whole supplied document.

    Explicitly historical lines are records of a prior state and are not stale live
    claims. Current-working-tree and present-tense uncommitted formulations are.
    """

    matches: List[Tuple[int, str]] = []
    for line_number, line in enumerate(text.splitlines(), 1):
        # Only an explicit record marker receives the historical exemption. Merely appending the
        # word "historical" to a present-tense landing claim must not bypass this invariant.
        normalized = line.strip().lower()
        if normalized.startswith(("historical note:", "[historical]")):
            continue
        if any(pattern.search(line) for pattern in STALE_PRESENT_TENSE):
            matches.append((line_number, line.strip()))
    return matches


def stable_id_counts(text: str, prefix: str, maximum: int) -> List[int]:
    return [
        len(re.findall(rf"^{re.escape(prefix)}{number}\.\s", text, re.MULTILINE))
        for number in range(1, maximum + 1)
    ]


def section(text: str, start: str, end: Optional[str] = None) -> str:
    start_index = text.find(start)
    if start_index < 0:
        return ""
    if end is None:
        return text[start_index:]
    end_index = text.find(end, start_index + len(start))
    return text[start_index:] if end_index < 0 else text[start_index:end_index]


def git_output(root: Path, args: Sequence[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def verify(root: Path) -> List[Check]:
    claims = (root / "docs" / "CLAIMS.md").read_text(encoding="utf-8")
    blueprint = (root / "docs" / "COMPLETION_BLUEPRINT.md").read_text(encoding="utf-8")
    handoff = (root / "docs" / "HANDOFF_LIVE.md").read_text(encoding="utf-8")

    checks: List[Check] = []

    landed_occurrences = claims.count("889d9a7c")
    item_19a = section(claims, "\n19a.", "\n20.")
    item_20 = section(claims, "\n20.", "\n21.")
    landed_named = (
        "landed in `889d9a7c`" in item_19a
        and "landed in" in item_20
        and "`889d9a7c`" in item_20
    )
    # The split line in item 20 is accepted deliberately; the two predicates above
    # require both the landing verb and the exact immutable commit in that item.
    checks.append(
        Check(
            "claims_19a_20_commit_named",
            landed_named,
            f"occurrences={landed_occurrences}",
        )
    )

    b_counts = stable_id_counts(claims, "B", 5)
    checks.append(Check("claims_B_ids_unique", b_counts == [1] * 5, f"counts={b_counts}"))

    r_counts = stable_id_counts(claims, "R", 8)
    checks.append(Check("claims_R_ids_unique", r_counts == [1] * 8, f"counts={r_counts}"))

    boundary = section(
        claims,
        "### Open — boundary honesty / process",
        "### Resolved this arc",
    )
    resolved = section(claims, "### Resolved this arc", "**Status vocabulary:**")
    numeric_markers = re.findall(r"(?m)^\d+\.\s", boundary + resolved)
    old_references = (
        "item 7 (2nd list)",
        "item 7 in the second list",
        "see item 7",
    )
    ambiguous_absent = not numeric_markers and not any(ref in claims for ref in old_references)
    checks.append(Check("claims_old_ambiguous_markers_absent", ambiguous_absent))

    stale = stale_uncommitted_matches(claims)
    stale_detail = "" if not stale else "lines=" + ",".join(str(line) for line, _ in stale)
    checks.append(Check("claims_stale_uncommitted_wording_absent", not stale, stale_detail))

    blueprint_board_hits = [token for token in LIVE_BOARD_TOKENS if token in blueprint]
    blueprint_no_board = (
        "This document carries **no live board counts**" in blueprint and not blueprint_board_hits
    )
    checks.append(
        Check(
            "blueprint_no_live_numeric_board",
            blueprint_no_board,
            "" if not blueprint_board_hits else "tokens=" + ",".join(blueprint_board_hits),
        )
    )

    blueprint_universal = asserted_universal_matches(blueprint)
    checks.append(
        Check(
            "blueprint_no_asserted_universal",
            not blueprint_universal,
            f"occurrences={len(blueprint_universal)}" if blueprint_universal else "",
        )
    )

    live_prefix = handoff.split("## HISTORICAL SNAPSHOT BELOW", 1)[0]
    handoff_hits = [token for token in LIVE_BOARD_TOKENS if token in live_prefix]
    handoff_clean = (
        "CURRENT AUTHORITY — DERIVED, NOT HAND-TYPED" in live_prefix and not handoff_hits
    )
    checks.append(
        Check(
            "live_handoff_pointer_has_no_board_counts",
            handoff_clean,
            "" if not handoff_hits else "tokens=" + ",".join(handoff_hits),
        )
    )

    ancestor = git_output(root, ["merge-base", "--is-ancestor", "889d9a7c", "HEAD"])
    checks.append(Check("commit_889d9a7c_is_ancestor", ancestor.returncode == 0, f"rc={ancestor.returncode}"))

    committed = git_output(root, ["show", "889d9a7c:tools/anubis/src/main.rs"])
    source = committed.stdout if committed.returncode == 0 else ""
    textual_occurrences = source.count("require_vz_offensive(")
    literal_calls = len(
        re.findall(r"(?m)^\s*offensive::require_vz_offensive\(", source)
    )
    guard_count_ok = committed.returncode == 0 and literal_calls == 60 and textual_occurrences == 61
    checks.append(
        Check(
            "commit_889_guard_count",
            guard_count_ok,
            f"literal_calls={literal_calls} textual_occurrences={textual_occurrences}",
        )
    )

    if len(checks) != 10:
        raise RuntimeError(f"record verifier internal error: expected 10 checks, built {len(checks)}")
    return checks


def self_test() -> int:
    poisoned = (
        "Row is fixed in the current working tree.\n"
        "The change is still uncommitted.\n"
        "Another repair is not yet committed.\n"
    )
    lines = [line for line, _ in stale_uncommitted_matches(poisoned)]
    historical = "Historical note: before 03210603 the change was uncommitted.\n"
    universal = "anubis check passing =>\nthe program cannot violate its contracts"
    scoped = "Passing does not yet mean the program cannot violate its contracts"
    ok = (
        lines == [1, 2, 3]
        and stale_uncommitted_matches(historical) == []
        and len(asserted_universal_matches(universal)) == 1
        and asserted_universal_matches(scoped) == []
        and stable_id_counts("B1. one\nB2. two\n", "B", 2) == [1, 1]
        and stable_id_counts("B1. one\nB2. two\nB2. duplicate\n", "B", 2) == [1, 2]
    )
    if not ok:
        print("RECORD_VERIFICATION_SELFTEST: FAIL")
        return 1
    print("RECORD_VERIFICATION_SELFTEST: PASS (stale wording, universal form, and ID poisons detected)")
    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()

    try:
        checks = verify(args.root.resolve())
    except (OSError, UnicodeError, RuntimeError) as error:
        print(f"RECORD_VERIFICATION: ERROR — {error}")
        return 2

    passed = 0
    for check in checks:
        status = "PASS" if check.passed else "FAIL"
        detail = f" — {check.detail}" if check.detail else ""
        print(f"{check.name}: {status}{detail}")
        passed += int(check.passed)
    if passed == len(checks):
        print(f"RECORD_VERIFICATION: PASS ({passed}/{len(checks)})")
        return 0
    print(f"RECORD_VERIFICATION: FAIL ({passed}/{len(checks)})")
    return 1


if __name__ == "__main__":
    sys.exit(main())
