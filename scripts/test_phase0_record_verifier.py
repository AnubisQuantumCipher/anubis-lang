#!/usr/bin/env python3
"""Focused tests for the durable Phase-0 record verifier."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_phase0_record as record  # noqa: E402


class RecordVerifierTests(unittest.TestCase):
    def test_stale_present_tense_wording_is_rejected_across_whole_document(self) -> None:
        text = """
Row A is fixed in the current working tree.
The change is still uncommitted and this entry does not claim it as landed.
Another repair is not yet committed.
"""
        matches = record.stale_uncommitted_matches(text)
        self.assertEqual([line for line, _ in matches], [2, 3, 4])

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

    def test_asserted_universal_matcher_rejects_direct_and_multiline_forms(self) -> None:
        direct = "`anubis check` PASS means the program cannot violate its contracts"
        multiline = "anubis check passing =>\nthe program cannot violate its contracts"
        scoped = "Passing does not yet mean the program cannot violate its contracts"
        self.assertEqual(len(record.asserted_universal_matches(direct)), 1)
        self.assertEqual(len(record.asserted_universal_matches(multiline)), 1)
        self.assertEqual(len(record.asserted_universal_matches(scoped)), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
