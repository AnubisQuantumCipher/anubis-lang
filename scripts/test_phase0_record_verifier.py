#!/usr/bin/env python3
"""Focused tests for the durable Phase-0 record verifier."""

from __future__ import annotations

import sys
import tempfile
import unittest
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_phase0_record as record  # noqa: E402


class RecordVerifierTests(unittest.TestCase):
    def test_stale_present_tense_wording_is_rejected_across_whole_document(self) -> None:
        text = """
Row A is fixed in the current working tree.
The change is still uncommitted and this entry does not claim it as landed.
Another repair is not yet committed.
This change is still uncommitted despite a historical-sounding suffix.
"""
        matches = record.stale_uncommitted_matches(text)
        self.assertEqual([line for line, _ in matches], [2, 3, 4, 5])

    def test_landed_and_explicitly_historical_wording_is_accepted(self) -> None:
        text = """
The repair landed in commit 03210603.
Historical note: before 03210603 the change was uncommitted.
"""
        self.assertEqual(record.stale_uncommitted_matches(text), [])

    def test_stable_ids_require_exactly_one_of_each(self) -> None:
        good = "B1. one\nB2. two\nR1. one\nR2. two\n"
        self.assertEqual(record.stable_id_counts(good, "B", 2), [1, 1])
        self.assertEqual(record.stable_id_counts(good, "R", 2), [1, 1])
        duplicate = good + "B2. duplicate\n"
        self.assertEqual(record.stable_id_counts(duplicate, "B", 2), [1, 2])

    def test_landing_clause_rejects_negated_commit_claims(self) -> None:
        matcher = getattr(record, "positive_landing_commit_named", None)
        self.assertIsNotNone(matcher, "positive landing-clause matcher is missing")
        if matcher is None:
            return
        self.assertTrue(matcher("Landing-status: LANDED commit=`889d9a7c`", "889d9a7c"))
        self.assertFalse(matcher("The repair landed in `889d9a7c`.", "889d9a7c"))
        self.assertFalse(matcher("The repair was not landed in `889d9a7c`.", "889d9a7c"))
        self.assertFalse(matcher("The repair never landed in `889d9a7c`.", "889d9a7c"))
        self.assertFalse(matcher("The repair was not actually landed in `889d9a7c`.", "889d9a7c"))
        self.assertFalse(matcher("It is false that the repair landed in `889d9a7c`.", "889d9a7c"))
        self.assertFalse(
            matcher(
                "~~~text\nLanding-status: LANDED commit=`889d9a7c`\n~~~\n",
                "889d9a7c",
            )
        )
        self.assertFalse(
            matcher(
                "Landing-status: LANDED commit=`889d9a7c`\n"
                "landing-status: NOT LANDED commit=`889d9a7c`\n",
                "889d9a7c",
            )
        )
        self.assertFalse(
            matcher(
                "```bad`info\nLanding-status: LANDED commit=`889d9a7c`\n```\n",
                "889d9a7c",
            )
        )
        self.assertFalse(
            matcher("    Landing-status: LANDED commit=`889d9a7c`\n", "889d9a7c")
        )
        self.assertFalse(
            matcher(
                "- ```\n  Landing-status: LANDED commit=`889d9a7c`\n",
                "889d9a7c",
            )
        )
        self.assertFalse(
            matcher(
                "```\nx\n```\u00a0\nLanding-status: LANDED commit=`889d9a7c`\n",
                "889d9a7c",
            )
        )
        for separator in ("\x0b", "\x0c", "\x85", "\u2028", "\u2029"):
            self.assertFalse(
                matcher(
                    "```\nx\n```"
                    + separator
                    + "Landing-status: LANDED commit=`889d9a7c`\n",
                    "889d9a7c",
                ),
                repr(separator),
            )
        for invalid_indent in ("\t", "\u00a0", "\u2003"):
            self.assertFalse(
                matcher(
                    "Landing-status: LANDED commit=`889d9a7c`\n"
                    f"{invalid_indent}```text\n"
                    "landing-status: NOT LANDED commit=`889d9a7c`\n"
                    "```\n",
                    "889d9a7c",
                ),
                repr(invalid_indent),
            )
        for tag in ("pre", "script", "style", "textarea"):
            self.assertFalse(
                matcher(
                    f"<{tag}>Landing-status: LANDED commit=`889d9a7c`</{tag}>\n",
                    "889d9a7c",
                ),
                tag,
            )
        for opener, closer in (
            ("<?anubis", "?>"),
            ("<!ANUBIS", ">"),
            ("<![CDATA[", "]]>"),
            ("<div hidden>", "</div>"),
            ("<anubis-report>", "</anubis-report>"),
            ("<template>", "</template>"),
        ):
            self.assertFalse(
                matcher(
                    opener
                    + "\nLanding-status: LANDED commit=`889d9a7c`\n"
                    + closer
                    + "\n",
                    "889d9a7c",
                ),
                opener,
            )
        canonical = "Landing-status: LANDED commit=`889d9a7c`\n"
        for visible_poison in (
            "\\<!--\nlanding-status: NOT LANDED commit=`889d9a7c`\n-->",
            "\\<script>\nlanding-status: NOT LANDED commit=`889d9a7c`\n\\</script>",
            "< script>\nlanding-status: NOT LANDED commit=`889d9a7c`\n</script>",
        ):
            self.assertFalse(matcher(canonical + visible_poison + "\n", "889d9a7c"))
        for inline_container in (
            "prefix <span hidden>\n" + canonical,
            "prefix <div hidden>\n" + canonical,
            "\\`<div hidden>`\n\n" + canonical,
            "`x\\`<script>`\n\n" + canonical + "\n`y\\`</script>`\n",
            "prefix ``\n" + canonical + "``\n",
            "![prefix\n" + canonical + "](https://example.com/x.png)\n",
            "[prefix\n" + canonical + "](https://example.com)\n",
        ):
            self.assertFalse(matcher(inline_container, "889d9a7c"), inline_container)

    def test_asserted_universal_matcher_rejects_direct_and_multiline_forms(self) -> None:
        direct = "`anubis check` PASS means the program cannot violate its contracts"
        multiline = "anubis check passing =>\nthe program cannot violate its contracts"
        scoped = "Passing does not yet mean the program cannot violate its contracts"
        self.assertEqual(len(record.asserted_universal_matches(direct)), 1)
        self.assertEqual(len(record.asserted_universal_matches(multiline)), 1)
        self.assertEqual(len(record.asserted_universal_matches(scoped)), 0)

    def test_markdown_emphasis_does_not_bypass_asserted_universal_matcher(self) -> None:
        plain = "`anubis check` PASS => the program cannot violate its contracts"
        emphasized = "`anubis check` *PASS* => the program cannot violate its contracts"
        self.assertEqual(len(record.asserted_universal_matches(plain)), 1)
        self.assertEqual(len(record.asserted_universal_matches(emphasized)), 1)

    def test_pre_fix_pin_overclaims_are_rejected_but_historical_scope_is_accepted(self) -> None:
        matcher = getattr(record, "pre_fix_pin_overclaim_matches", None)
        self.assertIsNotNone(matcher, "pre-fix pin provenance matcher is missing")
        if matcher is None:
            return
        bad = """
A source-current pin, `vm/pins/anubis-242902cfefc0`, matches the tree.
The pin supplies the missing half of the discriminator.
The sealed slice closes CLAIMS 19a and 20.
"""
        good = record.CANONICAL_PRE_FIX_RECEIPT + "\n"
        self.assertEqual(len(matcher(bad)), 1)
        self.assertEqual(matcher(good), [])
        indented = "\n".join("    " + line for line in good.splitlines()) + "\n"
        self.assertTrue(matcher(indented))
        self.assertTrue(matcher("<div>\n" + good + "</div>\n"))

    def test_pre_fix_pin_policy_requires_four_head_bound_references_and_denies_authority(self) -> None:
        policy = getattr(record, "pin_242_provenance_violations", None)
        self.assertIsNotNone(policy, "pre-fix pin provenance policy is missing")
        if policy is None:
            return
        claims = (
            "20. **A later source-matched receipt may close this item.**\n\n"
            + record.CANONICAL_PRE_FIX_RECEIPT
            + "\n\n"
            + record.CANONICAL_PRE_FIX_RECEIPT
            + "\n\n"
            + record.CANONICAL_PRE_FIX_RECEIPT
            + "\n\n21. Next item.\n"
        )
        handoff = record.CANONICAL_PRE_FIX_RECEIPT + "\n"
        self.assertEqual(policy(claims, handoff), [])
        self.assertTrue(
            any("reference-count" in detail for _, detail in policy(claims, ""))
        )
        self.assertTrue(
            any(
                "missing-head" in detail
                for _, detail in policy(claims.replace("0f407853", "unknown", 2), handoff)
            )
        )
        poisoned = claims.replace(
            "cannot verify later repairs",
            "proves the post-fix repair",
            1,
        )
        self.assertTrue(
            any(
                "noncanonical-pre-fix-reference" in detail
                for _, detail in policy(poisoned, handoff)
            )
        )
        establishes = claims.replace(
            "cannot verify later repairs",
            "establishes that the later repair works",
            1,
        )
        authoritative = claims.replace(
            "cannot verify later repairs",
            "is authoritative evidence for the later repair",
            1,
        )
        shows = claims.replace(
            "cannot verify later repairs", "shows that the later repair works", 1
        )
        bare_authoritative = claims.replace(
            "cannot verify later repairs", "is authoritative for later repairs", 1
        )
        self.assertTrue(any("noncanonical-pre-fix-reference" in detail for _, detail in policy(establishes, handoff)))
        self.assertTrue(any("noncanonical-pre-fix-reference" in detail for _, detail in policy(authoritative, handoff)))
        self.assertTrue(any("noncanonical-pre-fix-reference" in detail for _, detail in policy(shows, handoff)))
        self.assertTrue(any("noncanonical-pre-fix-reference" in detail for _, detail in policy(bare_authoritative, handoff)))
        proof = claims.replace(
            "cannot verify later repairs",
            "is proof of the later repair",
            1,
        )
        self.assertTrue(any("noncanonical-pre-fix-reference" in detail for _, detail in policy(proof, handoff)))
        live_poison = (
            "Historical receipt `vm/pins/anubis-242902cfefc0` records head `0f407853`; "
            "it shows the repair works.\n"
            + record.RECEIPT_SCOPE_MARKER
            + "\n"
        )
        self.assertTrue(
            any(
                "noncanonical-pre-fix-reference" in detail
                for _, detail in policy(claims, handoff, live_poison)
            )
        )
        hidden_fifth = handoff + (
            "```text\nPin `vm/pins/anubis-242902cfefc0` records head `0f407853`.\n"
        )
        self.assertTrue(
            any(
                "unbalanced-code-fence" in detail
                for _, detail in policy(claims, hidden_fifth)
            )
        )

    def test_item20_handoff_must_not_contradict_authoritative_claims(self) -> None:
        policy = getattr(record, "item20_receipt_consistency_violations", None)
        self.assertIsNotNone(policy, "item-20 cross-document consistency policy is missing")
        if policy is None:
            return
        claims = """
20. **Source fix landed; POST-FIX GUEST VERIFICATION OPEN.**

CLAIMS-20-receipt-status: OPEN
CLAIMS-20-receipt-identity: MISMATCH

The exported report and checked-out report are different
objects. Closure requires a fresh
strict-validator PASS on the current source pin.

21. Next item.
"""
        honest = record.HANDOFF_CLAIMS20_MARKER + "\n"
        contradictory = "CLAIMS 20 is now closed with matching report identity.\n"
        violations = policy(claims, contradictory, honest)
        self.assertTrue(any("HANDOFF:claims20-authority" in detail for _, detail in violations))
        self.assertEqual(policy(claims, honest, honest), [])
        self.assertTrue(policy(claims, "    " + honest, honest))
        self.assertTrue(policy(claims, "<div>\n" + honest + "</div>\n", honest))
        self.assertTrue(policy(claims, "```text\n" + honest + "```\n", honest))
        self.assertTrue(policy(claims, "prefix ``\n" + honest + "``\n", honest))
        self.assertTrue(policy(claims, "prefix <span hidden>\n" + honest, honest))
        indented_fields = claims.replace(
            "CLAIMS-20-receipt-status: OPEN", "    CLAIMS-20-receipt-status: OPEN"
        )
        self.assertTrue(policy(indented_fields, honest, honest))
        fenced_fields = claims.replace(
            "CLAIMS-20-receipt-status: OPEN\nCLAIMS-20-receipt-identity: MISMATCH",
            "```text\nCLAIMS-20-receipt-status: OPEN\n"
            "CLAIMS-20-receipt-identity: MISMATCH\n```",
        )
        self.assertTrue(policy(fenced_fields, honest, honest))
        synonym = "CLAIMS 20 is fully resolved; the binary and report hashes match.\n"
        split = "CLAIMS 20 is now\nclosed by the receipt; the binary and report identities match.\n"
        synonym_chase = (
            "CLAIMS 20 is done and its identities are identical.\n",
            "CLAIMS 20 is settled and its reports are equal.\n",
            "CLAIMS 20 is ready to sign off; both artifacts are the same.\n",
        )
        self.assertTrue(policy(claims, synonym, honest))
        self.assertTrue(policy(claims, split, honest))
        for poison in synonym_chase:
            self.assertTrue(policy(claims, poison, honest), poison)
        live_poison = "CLAIMS 20 is fully resolved; the receipt hashes match.\n"
        self.assertTrue(
            any(
                "HANDOFF_LIVE:claims20-authority" in detail
                for _, detail in policy(claims, honest, live_poison)
            )
        )
        pending = claims.replace("CLAIMS-20-receipt-status: OPEN", "CLAIMS-20-receipt-status: PENDING")
        self.assertTrue(
            any("claims-item-20-status" in detail for _, detail in policy(pending, honest, honest))
        )
        hashes_differ = claims.replace(
            "CLAIMS-20-receipt-identity: MISMATCH",
            "CLAIMS-20-receipt-identity: MISMATCH\n\nThe receipt hashes differ.",
        )
        self.assertEqual(policy(hashes_differ, honest, honest), [])

    def test_completion_report_requires_all_sections_and_complete_boundary(self) -> None:
        policy = getattr(record, "phase0_completion_report_violations", None)
        self.assertIsNotNone(policy, "Phase-0 completion-report policy is missing")
        if policy is None:
            return
        names = (
            "Header", "Exit criteria", "RED before GREEN", "Over-rejection guard",
            "Falsification", "Independent audit", "Convergence metrics", "Seal and CI",
            "What I did NOT verify", "What I got wrong", "Landing state", "Sign-off",
        )
        headings = "\n\n".join(
            f"## {number}. {name}" for number, name in enumerate(names, 1)
        )
        hold = headings + "\n\nPhase 0: HOLD\nBlocking: item 20\n"
        complete = headings + "\n\nPhase 0: COMPLETE\nBlocking: nothing within Phase 0\n"
        hold_problems = policy(hold)
        self.assertTrue(any("signoff-not-complete" in problem for problem in hold_problems))
        self.assertTrue(any("blocking-not-clear" in problem for problem in hold_problems))
        self.assertEqual(policy(complete), [])
        contradictory = complete + "Phase 0: HOLD\nBlocking: unresolved\n"
        self.assertTrue(policy(contradictory))
        indented_contradiction = complete + "  Phase 0: HOLD\n  Blocking: unresolved\n"
        self.assertTrue(policy(indented_contradiction))
        indented_report = "\n".join("    " + line for line in complete.splitlines()) + "\n"
        self.assertTrue(policy(indented_report))
        nested_report = "- ```\n" + "\n".join(
            "  " + line for line in complete.splitlines()
        ) + "\n"
        self.assertTrue(policy(nested_report))
        reversed_report = "\n\n".join(
            f"## {number}. {name}" for number, name in reversed(list(enumerate(names, 1)))
        ) + "\n\nPhase 0: COMPLETE\nBlocking: nothing within Phase 0\n"
        self.assertTrue(policy(reversed_report))
        commented = "<!--\n" + complete + "-->\n"
        self.assertTrue(policy(commented))
        fenced = "```text\n" + complete + "```\n"
        self.assertTrue(policy(fenced))
        tilde_fenced = "~~~markdown\n" + complete + "~~~\n"
        self.assertTrue(policy(tilde_fenced))
        invalid_backtick_info = complete + (
            "```bad`info\n"
            "Phase 0: HOLD\n"
            "Blocking: unresolved\n"
            "```\n"
        )
        self.assertTrue(policy(invalid_backtick_info))
        for invalid_indent in ("\t", "\u00a0", "\u2003"):
            visible_contradiction = complete + (
                f"{invalid_indent}```text\n"
                "Phase 0: HOLD\n"
                "Blocking: unresolved\n"
                "```\n"
            )
            self.assertTrue(policy(visible_contradiction), repr(invalid_indent))
        invalid_closer = "```\nx\n```\u00a0\n" + complete
        self.assertTrue(policy(invalid_closer))
        for separator in ("\x0b", "\x0c", "\x85", "\u2028", "\u2029"):
            self.assertTrue(policy("```\nx\n```" + separator + complete), repr(separator))
        for tag in ("pre", "script", "style", "textarea"):
            self.assertTrue(policy(f"<{tag}>\n{complete}</{tag}>\n"), tag)
        for opener, closer in (
            ("<?anubis", "?>"),
            ("<!ANUBIS", ">"),
            ("<![CDATA[", "]]>"),
            ("<div hidden>", "</div>"),
            ("<table>", "</table>"),
            ("<section>", "</section>"),
            ("<details>", "</details>"),
            ("<anubis-report>", "</anubis-report>"),
            ("<template>", "</template>"),
        ):
            self.assertTrue(policy(opener + "\n" + complete + closer + "\n"), opener)
        self.assertTrue(policy("prefix <div hidden>\n" + complete))
        self.assertTrue(policy("prefix ``\n" + complete + "``\n"))
        self.assertTrue(
            policy("![prefix\n" + complete + "](https://example.com/x.png)\n")
        )
        reversed_comments = "-->\n" + complete + "<!--\n"
        self.assertTrue(policy(reversed_comments))
        decoy = complete.replace(
            "## 12. Sign-off",
            "## 12. Decoy\nPhase 0: COMPLETE\nBlocking: nothing within Phase 0\n"
            "## 12. Sign-off\nPhase 0: HOLD\nBlocking: unresolved",
        )
        self.assertTrue(policy(decoy))

    def test_phase0_report_selector_is_iso_date_bound(self) -> None:
        selector = getattr(record, "select_phase0_report", None)
        self.assertIsNotNone(selector, "ISO-date Phase-0 report selector is missing")
        if selector is None:
            return
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            older = root / "PHASE_0_COMPLETION_2026-07-30.md"
            latest = root / "PHASE_0_COMPLETION_2026-07-31.md"
            older.write_text("old")
            latest.write_text("new")
            selected, problems = selector(root, date(2026, 7, 31))
            self.assertEqual(selected, latest)
            self.assertEqual(problems, [])
            (root / "PHASE_0_COMPLETION_Z_OLD.md").write_text("decoy")
            _, problems = selector(root, date(2026, 7, 31))
            self.assertTrue(any("invalid-report-name" in problem for problem in problems))
            (root / "PHASE_0_COMPLETION_Z_OLD.md").unlink()
            (root / "PHASE_0_COMPLETION_2026-08-01.md").write_text("future")
            _, problems = selector(root, date(2026, 7, 31))
            self.assertTrue(any("future-report" in problem for problem in problems))


if __name__ == "__main__":
    unittest.main(verbosity=2)
