#!/usr/bin/env python3
"""Re-runnable verifier for the Phase-0 record-correction criteria.

This checks the live documentation plus the immutable commit evidence cited by
the latest ``docs/evidence/PHASE_0_COMPLETION_*.md``. It is intentionally narrow: a
PASS means these thirteen record invariants hold, not that the repository is sealed.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import date
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
PIN_242 = "vm/pins/anubis-242902cfefc0"
RECEIPT_SCOPE_MARKER = "[receipt-scope: pre-fix-only; head: 0f407853; authority: none]"
CANONICAL_PRE_FIX_RECEIPT = (
    "Historical receipt `vm/pins/anubis-242902cfefc0` records head `0f407853`; "
    "it predates `889d9a7c` and cannot verify later repairs. "
    + RECEIPT_SCOPE_MARKER
)
HANDOFF_CLAIMS20_MARKER = "CLAIMS-20-authority: DEFER-TO-CLAIMS"
REPORT_NAME = re.compile(r"^PHASE_0_COMPLETION_(\d{4}-\d{2}-\d{2})\.md$")
PHASE0_REPORT_SECTION_PATTERNS = (
    r"Header\b",
    r"Exit criteria\b",
    r"RED before GREEN\b",
    r"Over-rejection guard\b",
    r"Falsification\b",
    r"(?:Independent\s+)?audit\b",
    r"Convergence metrics\b",
    r"Seal and CI\b",
    r"What I did NOT verify\b",
    r"What I got wrong\b",
    r"Landing state\b",
    r"Sign-off\b",
)


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str = ""


def prose_outside_inline_code(line: str) -> str:
    """Remove complete same-line CommonMark code spans for conservative HTML screening."""

    def backslash_escaped(index: int) -> bool:
        backslashes = 0
        cursor = index - 1
        while cursor >= 0 and line[cursor] == "\\":
            backslashes += 1
            cursor -= 1
        return backslashes % 2 == 1

    prose: List[str] = []
    cursor = 0
    while cursor < len(line):
        opening = None
        for candidate in re.finditer(r"`+", line[cursor:]):
            if not backslash_escaped(cursor + candidate.start()):
                opening = candidate
                break
        if opening is None:
            prose.append(line[cursor:])
            break
        opening_start = cursor + opening.start()
        opening_end = cursor + opening.end()
        prose.append(line[cursor:opening_start])
        delimiter = opening.group(0)
        closing = None
        for candidate in re.finditer(r"`+", line[opening_end:]):
            # Backslash escapes are not interpreted inside a CommonMark code span. Any later run
            # of exactly the opener's length closes it, even when a backslash immediately precedes
            # that run.
            if candidate.group(0) == delimiter:
                closing = candidate
                break
        if closing is None:
            # An unmatched run is ordinary source for this narrow screen. Keeping it visible is
            # conservative: any following tag-looking construct will be refused.
            prose.append(line[opening_start:])
            break
        cursor = opening_end + closing.end()
    return "".join(prose)


def isolated_source_line(lines: Sequence[str], index: int) -> bool:
    """Return whether a structured field is its own top-level source paragraph."""

    return (index == 0 or lines[index - 1] == "") and (
        index == len(lines) - 1 or lines[index + 1] == ""
    )


def visible_markdown(text: str) -> Tuple[str, List[str]]:
    """Return top-level prose outside simple fences, failing closed on ambiguous markup."""

    problems: List[str] = []
    forbidden_line_separators = {
        "\x0b": "VT",
        "\x0c": "FF",
        "\x85": "NEL",
        "\u2028": "LS",
        "\u2029": "PS",
    }
    for character, name in forbidden_line_separators.items():
        if character in text:
            problems.append(f"non-commonmark-line-separator:{name}")

    # CommonMark recognizes CRLF, LF, and CR as line endings. Python splitlines() recognizes more
    # Unicode separators and can therefore invent a fence close that CommonMark never sees.
    visible = text.replace("\r\n", "\n").replace("\r", "\n")
    if "<!--" in visible or "-->" in visible:
        problems.append("html-comments-forbidden-in-authority-record")

    lines: List[str] = []
    fence_char: Optional[str] = None
    fence_length = 0
    for line in visible.split("\n"):
        if fence_char is not None:
            if re.match(
                rf"^ {{0,3}}{re.escape(fence_char)}{{{fence_length},}}[ \t]*$",
                line,
            ):
                fence_char = None
                fence_length = 0
            continue

        # CommonMark fence indentation is zero to three literal ASCII spaces. ``\s`` would also
        # accept tabs and Unicode spaces, incorrectly hiding text that CommonMark renders.
        fence = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if fence:
            # CommonMark forbids a backtick in the info string of a backtick fence. Treating that
            # invalid opener as a fence would hide ordinary visible prose until a later delimiter,
            # including contradictory policy markers.
            if fence.group(1)[0] == "`" and "`" in fence.group(2):
                problems.append("invalid-backtick-fence-info")
                lines.append(line)
                continue
            fence_char = fence.group(1)[0]
            fence_length = len(fence.group(1))
            continue

        # The authoritative record does not need HTML. After masking complete same-line code spans,
        # reject every tag/declaration opener anywhere in prose; an unclosed inline ``<span hidden>``
        # can otherwise make later top-level source fields browser-invisible. Four-space/tab-indented
        # lines are CommonMark code and cannot contain a top-level authority field.
        if not line.startswith(("    ", "\t")):
            prose = prose_outside_inline_code(line)
            if re.search(r"(?<!\\)<(?:/?[A-Za-z]|[!?])", prose):
                problems.append("html-construct-forbidden")
            if re.search(r"\\<(?:/?[A-Za-z]|[!?])", prose):
                problems.append("escaped-html-control")
            if re.search(r"<(?:[ \t]+)/?[A-Za-z]", prose):
                problems.append("invalid-html-block-opener")
        lines.append(line)
    if fence_char is not None:
        problems.append("unbalanced-code-fence")
    return "\n".join(lines) + "\n", problems


def asserted_universal_matches(text: str) -> List[re.Match[str]]:
    # Markdown emphasis is presentation, not policy. Removing `*`/`_` preserves newlines, so the
    # matcher cannot be bypassed by changing `PASS` to `*PASS*`. Backticks remain because the
    # grammar already handles the optional code span explicitly.
    normalized = re.sub(r"[*_]", "", text)
    return list(UNIVERSAL.finditer(normalized))


def positive_landing_commit_named(text: str, commit: str) -> bool:
    """Require one canonical landing fact instead of inferring affirmation from prose."""

    visible, problems = visible_markdown(text)
    if problems:
        return False
    expected = f"Landing-status: LANDED commit=`{commit}`"
    source = text.replace("\r\n", "\n").replace("\r", "\n")
    source_lines = source.split("\n")
    source_marker_indices = [
        index
        for index, line in enumerate(source_lines)
        if re.match(r"^[ \t]*landing-status\s*:", line, re.IGNORECASE)
    ]
    visible_markers = [
        line
        for line in visible.split("\n")
        if re.match(r"^[ \t]*landing-status\s*:", line, re.IGNORECASE)
    ]
    return (
        len(source_marker_indices) == 1
        and source_lines[source_marker_indices[0]] == expected
        and isolated_source_line(source_lines, source_marker_indices[0])
        and visible_markers == [expected]
    )


def pre_fix_pin_overclaim_matches(text: str) -> List[Tuple[int, str]]:
    """Require every pre-fix 242902 reference to use one exact bounded receipt."""

    text, markup_problems = visible_markdown(text)
    matches: List[Tuple[int, str]] = [(0, problem) for problem in markup_problems]
    offset = 0
    for paragraph in re.split(r"\n\s*\n", text):
        start = text.find(paragraph, offset)
        offset = max(offset, start + len(paragraph))
        if PIN_242 not in paragraph:
            continue
        line_number = text.count("\n", 0, max(0, start)) + 1
        normalized = re.sub(r"\s+", " ", re.sub(r"[*_]", "", paragraph)).strip()
        indented = any(
            line.startswith((" ", "\t")) for line in paragraph.split("\n") if line
        )
        if normalized != CANONICAL_PRE_FIX_RECEIPT or indented:
            matches.append((line_number, "noncanonical-pre-fix-reference"))
    return matches


def pin_242_provenance_violations(
    claims: str, handoff_history: str, handoff_live: str = ""
) -> List[Tuple[int, str]]:
    """Require exactly four canonical, head-bound references to the pre-fix pin."""

    claims_visible, claims_markup = visible_markdown(claims)
    history_visible, history_markup = visible_markdown(handoff_history)
    live_visible, live_markup = visible_markdown(handoff_live)
    violations: List[Tuple[int, str]] = [
        (0, f"{source}:{problem}")
        for source, problems in (
            ("CLAIMS", claims_markup),
            ("HANDOFF", history_markup),
            ("HANDOFF_LIVE", live_markup),
        )
        for problem in problems
    ]
    violations += (
        pre_fix_pin_overclaim_matches(claims_visible)
        + pre_fix_pin_overclaim_matches(history_visible)
        + pre_fix_pin_overclaim_matches(live_visible)
    )
    reference_count = (
        claims_visible.count(PIN_242)
        + history_visible.count(PIN_242)
        + live_visible.count(PIN_242)
    )
    if reference_count != 4:
        violations.append((0, f"reference-count={reference_count},expected=4"))

    for text_name, text in (
        ("CLAIMS", claims_visible),
        ("HANDOFF", history_visible),
        ("HANDOFF_LIVE", live_visible),
    ):
        offset = 0
        for paragraph in re.split(r"\n\s*\n", text):
            start = text.find(paragraph, offset)
            offset = max(offset, start + len(paragraph))
            if PIN_242 not in paragraph:
                continue
            line_number = text.count("\n", 0, max(0, start)) + 1
            if "0f407853" not in paragraph:
                violations.append((line_number, f"{text_name}:missing-head-0f407853"))

    return violations


def item20_receipt_consistency_violations(
    claims: str, handoff_history: str, handoff_live: str = ""
) -> List[Tuple[int, str]]:
    """Reject present-tense handoff closure claims that contradict CLAIMS item 20.

    The pin-specific invariant catches promotion of the pre-fix receipt. It did not compare the
    later post-fix receipt across documents, so CLAIMS could keep the receipt OPEN for mismatched
    identities while HANDOFF simultaneously called it closed with matching identities.
    """

    claims_source = claims.replace("\r\n", "\n").replace("\r", "\n")
    history_source = handoff_history.replace("\r\n", "\n").replace("\r", "\n")
    live_source = handoff_live.replace("\r\n", "\n").replace("\r", "\n")
    claims, claims_markup = visible_markdown(claims_source)
    handoff_history, history_markup = visible_markdown(history_source)
    handoff_live, live_markup = visible_markdown(live_source)
    markup_problems = claims_markup + history_markup + live_markup
    if markup_problems:
        return [(0, problem) for problem in markup_problems]
    item_20 = section(claims_source, "\n20.", "\n21.")
    visible_item_20 = section(claims, "\n20.", "\n21.")
    if not item_20:
        return [(0, "claims-item-20-missing")]
    status_fields = re.findall(
        r"(?mi)^CLAIMS-20-receipt-status\s*:\s*(\S+)[ \t]*$", item_20
    )
    identity_fields = re.findall(
        r"(?mi)^CLAIMS-20-receipt-identity\s*:\s*(\S+)[ \t]*$", item_20
    )
    status_mentions = re.findall(
        r"(?mi)^[ \t]*CLAIMS-20-receipt-status\s*:", item_20
    )
    identity_mentions = re.findall(
        r"(?mi)^[ \t]*CLAIMS-20-receipt-identity\s*:", item_20
    )
    visible_status_fields = re.findall(
        r"(?mi)^CLAIMS-20-receipt-status\s*:\s*(\S+)[ \t]*$",
        visible_item_20,
    )
    visible_identity_fields = re.findall(
        r"(?mi)^CLAIMS-20-receipt-identity\s*:\s*(\S+)[ \t]*$",
        visible_item_20,
    )
    item_20_lines = item_20.split("\n")
    status_indices = [
        index
        for index, line in enumerate(item_20_lines)
        if re.match(r"(?i)^CLAIMS-20-receipt-status\s*:", line)
    ]
    identity_indices = [
        index
        for index, line in enumerate(item_20_lines)
        if re.match(r"(?i)^CLAIMS-20-receipt-identity\s*:", line)
    ]
    structured_block = (
        len(status_indices) == 1
        and len(identity_indices) == 1
        and identity_indices[0] == status_indices[0] + 1
        and (status_indices[0] == 0 or item_20_lines[status_indices[0] - 1] == "")
        and (
            identity_indices[0] == len(item_20_lines) - 1
            or item_20_lines[identity_indices[0] + 1] == ""
        )
    )
    field_problems: List[Tuple[int, str]] = []
    if (
        len(status_fields) != 1
        or len(status_mentions) != 1
        or visible_status_fields != status_fields
        or not structured_block
        or status_fields[0] not in {"OPEN", "CLOSED"}
    ):
        field_problems.append(
            (0, f"claims-item-20-status={status_fields!r},expected-one-OPEN-or-CLOSED")
        )
    if (
        len(identity_fields) != 1
        or len(identity_mentions) != 1
        or visible_identity_fields != identity_fields
        or identity_fields[0] not in {"MATCH", "MISMATCH"}
    ):
        field_problems.append(
            (0, f"claims-item-20-identity={identity_fields!r},expected-one-MATCH-or-MISMATCH")
        )
    if field_problems:
        return field_problems
    violations: List[Tuple[int, str]] = []
    for source_name, raw_text, visible_text in (
        ("HANDOFF", history_source, handoff_history),
        ("HANDOFF_LIVE", live_source, handoff_live),
    ):
        raw_lines = raw_text.split("\n")
        authority_indices = [
            index
            for index, line in enumerate(raw_lines)
            if re.search(r"\bCLAIMS[ -]?20\b", line, re.IGNORECASE)
        ]
        authority_lines = [raw_lines[index] for index in authority_indices]
        visible_authority_lines = [
            line
            for line in visible_text.split("\n")
            if re.search(r"\bCLAIMS[ -]?20\b", line, re.IGNORECASE)
        ]
        if (
            authority_lines != [HANDOFF_CLAIMS20_MARKER]
            or visible_authority_lines != [HANDOFF_CLAIMS20_MARKER]
            or len(authority_indices) != 1
            or not isolated_source_line(raw_lines, authority_indices[0])
        ):
            violations.append(
                (
                    0,
                    f"{source_name}:claims20-authority={authority_lines!r},"
                    f"expected={[HANDOFF_CLAIMS20_MARKER]!r}",
                )
            )
    return violations


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


def phase0_completion_report_violations(text: str) -> List[str]:
    """Require the twelve-section Phase-0 artifact to carry a clear completion boundary."""

    visible, violations = visible_markdown(text)
    source = text.replace("\r\n", "\n").replace("\r", "\n")
    source_lines = source.split("\n")
    source_heading_indices = [
        index
        for index, line in enumerate(source_lines)
        if re.match(r"^[ \t]*##\s+\d+\.", line)
    ]
    source_heading_lines = [source_lines[index] for index in source_heading_indices]
    if len(source_heading_lines) != 12 or any(
        line.startswith((" ", "\t")) for line in source_heading_lines
    ) or any(
        not isolated_source_line(source_lines, index)
        for index in source_heading_indices
    ):
        violations.append(
            f"top-level-source-heading-count={len(source_heading_lines)},expected=12"
        )
    source_status_indices = [
        index
        for index, line in enumerate(source_lines)
        if re.match(r"^[ \t]*Phase 0\s*:", line)
    ]
    source_blocking_indices = [
        index
        for index, line in enumerate(source_lines)
        if re.match(r"^[ \t]*Blocking\s*:", line)
    ]
    source_status_lines = [source_lines[index] for index in source_status_indices]
    source_blocking_lines = [source_lines[index] for index in source_blocking_indices]
    source_signoff_block = (
        len(source_status_indices) == 1
        and len(source_blocking_indices) == 1
        and source_blocking_indices[0] == source_status_indices[0] + 1
        and (
            source_status_indices[0] == 0
            or source_lines[source_status_indices[0] - 1] == ""
        )
        and (
            source_blocking_indices[0] == len(source_lines) - 1
            or source_lines[source_blocking_indices[0] + 1] == ""
        )
    )
    if source_status_lines != ["Phase 0: COMPLETE"]:
        violations.append("source-signoff-not-exactly-one-top-level-complete")
    if source_blocking_lines != ["Blocking: nothing within Phase 0"]:
        violations.append("source-blocking-not-exactly-one-top-level-clear")
    if not source_signoff_block:
        violations.append("source-signoff-not-an-isolated-structured-block")

    headings = list(re.finditer(r"(?m)^## (\d+)\.\s+([^\n]+)$", visible))
    if len(headings) != 12:
        violations.append(f"visible-heading-count={len(headings)},expected=12")
    for index, name_pattern in enumerate(PHASE0_REPORT_SECTION_PATTERNS, 1):
        if index > len(headings):
            violations.append(f"section-{index}-missing")
            continue
        heading = headings[index - 1]
        observed_number = int(heading.group(1))
        observed_title = heading.group(2).strip()
        if observed_number != index:
            violations.append(
                f"section-order-index={index},observed-number={observed_number}"
            )
        if not re.match(name_pattern, observed_title, re.IGNORECASE):
            violations.append(
                f"section-{index}-wrong-title={observed_title!r}"
            )

    signoff = visible[headings[11].end() :] if len(headings) >= 12 else ""
    status_values = re.findall(r"(?m)^Phase 0:\s*(.*?)\s*$", signoff)
    blocking_values = re.findall(r"(?m)^Blocking:\s*(.*?)\s*$", signoff)
    if status_values != ["COMPLETE"]:
        violations.append("signoff-not-complete")
    if blocking_values != ["nothing within Phase 0"]:
        violations.append("blocking-not-clear")
    return violations


def select_phase0_report(evidence_dir: Path, today: date) -> Tuple[Optional[Path], List[str]]:
    """Select the newest strict ISO-dated report and reject ambiguous/unsafe candidates."""

    problems: List[str] = []
    dated: dict[date, List[Path]] = {}
    try:
        candidates = sorted(
            path
            for path in evidence_dir.iterdir()
            if path.name.startswith("PHASE_0_COMPLETION_")
        )
    except OSError as exc:
        return None, [f"report-directory-error={exc}"]
    for path in candidates:
        match = REPORT_NAME.fullmatch(path.name)
        if not match:
            problems.append(f"invalid-report-name={path.name}")
            continue
        try:
            report_date = date.fromisoformat(match.group(1))
        except ValueError:
            problems.append(f"invalid-report-date={path.name}")
            continue
        if path.is_symlink() or not path.is_file():
            problems.append(f"unsafe-report-path={path.name}")
            continue
        if report_date > today:
            problems.append(f"future-report={path.name}")
            continue
        dated.setdefault(report_date, []).append(path)
    if not dated:
        problems.append("completion-report-missing")
        return None, problems
    newest_date = max(dated)
    if len(dated[newest_date]) != 1:
        problems.append(f"ambiguous-latest-report-date={newest_date.isoformat()}")
        return None, problems
    return dated[newest_date][0], problems


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
    handoff_history = (root / "docs" / "HANDOFF.md").read_text(encoding="utf-8")
    live_prefix = handoff.split("## HISTORICAL SNAPSHOT BELOW", 1)[0]
    completion_report_path, report_selection_problems = select_phase0_report(
        root / "docs" / "evidence", date.today()
    )
    completion_report = (
        completion_report_path.read_text(encoding="utf-8")
        if completion_report_path is not None
        else ""
    )

    checks: List[Check] = []

    landed_occurrences = claims.count("889d9a7c")
    item_19a = section(claims, "\n19a.", "\n20.")
    item_20 = section(claims, "\n20.", "\n21.")
    landed_named = positive_landing_commit_named(
        item_19a, "889d9a7c"
    ) and positive_landing_commit_named(item_20, "889d9a7c")
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

    pin_overclaims = pin_242_provenance_violations(claims, handoff_history, live_prefix)
    pin_detail = (
        ""
        if not pin_overclaims
        else "matches=" + ",".join(f"{line}:{label}" for line, label in pin_overclaims)
    )
    checks.append(
        Check("pin_242_pre_fix_provenance_honest", not pin_overclaims, pin_detail)
    )

    item20_conflicts = item20_receipt_consistency_violations(
        claims, handoff_history, live_prefix
    )
    item20_detail = (
        ""
        if not item20_conflicts
        else "matches=" + ",".join(
            f"{line}:{label}" for line, label in item20_conflicts
        )
    )
    checks.append(
        Check(
            "claims_handoff_item20_receipt_consistent",
            not item20_conflicts,
            item20_detail,
        )
    )

    report_problems = list(report_selection_problems)
    if completion_report_path is not None:
        report_problems.extend(phase0_completion_report_violations(completion_report))
    report_detail = (
        f"path={completion_report_path.relative_to(root)}"
        if completion_report_path is not None and not report_problems
        else "problems=" + ",".join(report_problems)
    )
    checks.append(
        Check("phase0_completion_report_current", not report_problems, report_detail)
    )

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

    if len(checks) != 13:
        raise RuntimeError(f"record verifier internal error: expected 13 checks, built {len(checks)}")
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
    emphasized_universal = "anubis check *passing* =>\nthe program cannot violate its contracts"
    scoped = "Passing does not yet mean the program cannot violate its contracts"
    bad_pin = (
        "A source-current pin `vm/pins/anubis-242902cfefc0` matches the tree and proves the repair.\n\n"
        "The sealed slice closes CLAIMS 19a and 20.\n"
    )
    good_pin = CANONICAL_PRE_FIX_RECEIPT + "\n"
    open_item_20 = (
        "\n20. **Source fix landed; POST-FIX GUEST VERIFICATION OPEN.**\n\n"
        "CLAIMS-20-receipt-status: OPEN\n"
        "CLAIMS-20-receipt-identity: MISMATCH\n\n"
        "The reports are different objects.\n\n21. Next item.\n"
    )
    honest_handoff = HANDOFF_CLAIMS20_MARKER + "\n"
    false_handoff = "CLAIMS 20 is done and its report identities are identical.\n"
    report_names = (
        "Header", "Exit criteria", "RED before GREEN", "Over-rejection guard",
        "Falsification", "Independent audit", "Convergence metrics", "Seal and CI",
        "What I did NOT verify", "What I got wrong", "Landing state", "Sign-off",
    )
    report_headings = "\n\n".join(
        f"## {number}. {name}" for number, name in enumerate(report_names, 1)
    )
    hold_report = report_headings + "\n\nPhase 0: HOLD\nBlocking: receipt mismatch\n"
    complete_report = (
        report_headings
        + "\n\nPhase 0: COMPLETE\nBlocking: nothing within Phase 0\n"
    )
    hold_violations = phase0_completion_report_violations(hold_report)
    ok = (
        lines == [1, 2, 3]
        and stale_uncommitted_matches(historical) == []
        and len(asserted_universal_matches(universal)) == 1
        and len(asserted_universal_matches(emphasized_universal)) == 1
        and asserted_universal_matches(scoped) == []
        and len(pre_fix_pin_overclaim_matches(bad_pin)) == 1
        and pre_fix_pin_overclaim_matches(good_pin) == []
        and item20_receipt_consistency_violations(
            open_item_20, honest_handoff, honest_handoff
        ) == []
        and len(
            item20_receipt_consistency_violations(
                open_item_20, false_handoff, honest_handoff
            )
        ) == 1
        and phase0_completion_report_violations(complete_report) == []
        and "signoff-not-complete" in hold_violations
        and "blocking-not-clear" in hold_violations
        and stable_id_counts("B1. one\nB2. two\n", "B", 2) == [1, 1]
        and stable_id_counts("B1. one\nB2. two\nB2. duplicate\n", "B", 2) == [1, 2]
    )
    if not ok:
        print("RECORD_VERIFICATION_SELFTEST: FAIL")
        return 1
    print(
        "RECORD_VERIFICATION_SELFTEST: PASS "
        "(stale wording, emphasized universal, pre-fix pin, report, and ID poisons detected)"
    )
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
