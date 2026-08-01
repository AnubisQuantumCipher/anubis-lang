#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/seal_verdict_validate.py"
CORE_GATES = [
    "capset_registry_parity",
    "capset_selfhost",
    "check_run_parity",
    "corpus_inventory_binding",
    "docs_drift",
    "formal",
    "formal_kernel",
    "gate_common_adoption",
    "gate_run_freshness",
    "host_resource_contract",
    "instrument_hygiene",
    "language",
    "native_authoritative",
    "proof_correspondence",
    "run_failclosed",
    "runtime",
    "security",
    "selfhost",
    "stdlib_failclosed",
    "taint_selfhost",
]
LEDGER_COMMIT = "b" * 40


def valid_ledger() -> bytes:
    names = [name for name in sorted(CORE_GATES) if name != "gate_run_freshness"]
    return "".join(
        f"{name} {LEDGER_COMMIT} {index + 1}\n" for index, name in enumerate(names)
    ).encode("ascii")


def valid_payload(ledger: bytes) -> dict[str, object]:
    ledger_commit = ledger.splitlines()[0].split(b" ")[1].decode("ascii")
    gates = [
        {"name": name, "status": "PASS", "declared_verdict_line": f"{name}: PASS", "score_reason": "declared_PASS_line"}
        for name in sorted(CORE_GATES)
    ]
    return {
        "gate": "seal_checklist",
        "status": "SEAL_PASS",
        "detail": "pass=20 skip=0 known_fail=0 pinned=/tmp/anubis.snap sha256=" + ("a" * 64),
        "profile": "core",
        "seal_out": "/tmp/seal",
        "instrument": {"raw": "seal_instrument_v1\nsha256=" + ("a" * 64) + "\n"},
        "gates": gates,
        "scoring_rule": "declared_verdict_line_only_never_body_grep_FAIL",
        "known_failing_manifest": "_None as of now_",
        "gate_run_ledger": {
            "schema": "anubis.gate-run-ledger-binding.v1",
            "sha256": hashlib.sha256(ledger).hexdigest(),
            "rows": 19,
            "commit": ledger_commit,
            "promoted_name": "gate_run_ledger.validated",
        },
    }


class SealVerdictValidateTests(unittest.TestCase):
    def validate(
        self,
        mutate=None,
        ledger_mutate=None,
        include_ledger: bool = True,
        bind_mutated_ledger: bool = False,
        writable_ledger: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            verdict = root / "seal_verdict.json"
            ledger = valid_ledger()
            if ledger_mutate:
                ledger = ledger_mutate(ledger)
            payload = valid_payload(ledger if bind_mutated_ledger else valid_ledger())
            if mutate:
                mutate(payload)
            verdict.write_text(json.dumps(payload, indent=2) + "\n")
            ledger_path = root / "gate_run_ledger.validated"
            ledger_path.write_bytes(ledger)
            ledger_path.chmod(0o444)
            if writable_ledger:
                ledger_path.chmod(0o644)
            command = ["python3", str(TOOL), "--verdict", str(verdict)]
            if include_ledger:
                command.extend(["--ledger", str(ledger_path)])
            command.extend(["--profile", "core"])
            result = subprocess.run(
                command,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            return result

    def test_complete_core_roster_passes(self) -> None:
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("SEAL_VERDICT_VALIDATE_PASS", result.stdout)

    def assert_rejected(self, mutate, needle: str) -> None:
        result = self.validate(mutate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(needle, result.stdout + result.stderr)

    def test_missing_gate_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["gates"] = [g for g in payload["gates"] if g["name"] != "formal_kernel"]  # type: ignore[index]
        self.assert_rejected(mutate, "missing")

    def test_extra_gate_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["gates"].append({"name": "surprise", "status": "PASS"})  # type: ignore[union-attr]
        self.assert_rejected(mutate, "extra")

    def test_duplicate_gate_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["gates"].append(dict(payload["gates"][0]))  # type: ignore[index,union-attr]
        self.assert_rejected(mutate, "duplicate")

    def test_nonpass_gate_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["gates"][0]["status"] = "FAIL"  # type: ignore[index]
        self.assert_rejected(mutate, "non-PASS")

    def test_detail_count_mismatch_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["detail"] = "pass=19 skip=0 known_fail=0 pinned=/tmp/anubis.snap sha256=" + ("a" * 64)
        self.assert_rejected(mutate, "detail")

    def test_missing_ledger_argument_rejected(self) -> None:
        result = self.validate(include_ledger=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--ledger is required", result.stderr)

    def test_missing_ledger_binding_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload.pop("gate_run_ledger")
        self.assert_rejected(mutate, "binding is missing")

    def test_writable_ledger_rejected(self) -> None:
        result = self.validate(writable_ledger=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be non-writable", result.stderr)

    def test_symlink_ledger_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ledger = valid_ledger()
            target = root / "outside-ledger"
            target.write_bytes(ledger)
            target.chmod(0o444)
            ledger_path = root / "gate_run_ledger.validated"
            ledger_path.symlink_to(target)
            verdict = root / "seal_verdict.json"
            verdict.write_text(json.dumps(valid_payload(ledger), indent=2) + "\n")
            result = subprocess.run(
                [
                    "python3",
                    str(TOOL),
                    "--verdict",
                    str(verdict),
                    "--ledger",
                    str(ledger_path),
                    "--profile",
                    "core",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("regular non-symlink", result.stdout + result.stderr)

    def test_ledger_digest_tamper_rejected(self) -> None:
        result = self.validate(ledger_mutate=lambda ledger: ledger.replace(b" 1\n", b" 9\n", 1))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sha256 mismatch", result.stdout + result.stderr)

    def test_rebound_incomplete_ledger_roster_rejected(self) -> None:
        result = self.validate(
            ledger_mutate=lambda ledger: b"\n".join(ledger.splitlines()[1:]) + b"\n",
            bind_mutated_ledger=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ledger roster mismatch", result.stdout + result.stderr)

    def test_ledger_commit_binding_mismatch_rejected(self) -> None:
        def mutate(payload: dict[str, object]) -> None:
            payload["gate_run_ledger"]["commit"] = "c" * 40  # type: ignore[index]
        self.assert_rejected(mutate, "bound commit")

    def test_repo_head_binding_passes_and_rejects_any_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "config", "user.email", "seal-validator@example.invalid"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "config", "user.name", "seal-validator-test"],
                cwd=root,
                check=True,
            )
            (root / "epoch").write_text("one\n")
            subprocess.run(["git", "add", "epoch"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "one"], cwd=root, check=True)
            head = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                text=True,
                stdout=subprocess.PIPE,
                check=True,
            ).stdout.strip()
            ledger = valid_ledger().replace(LEDGER_COMMIT.encode("ascii"), head.encode("ascii"))
            ledger_path = root / "gate_run_ledger.validated"
            ledger_path.write_bytes(ledger)
            ledger_path.chmod(0o444)
            verdict = root / "seal_verdict.json"
            verdict.write_text(json.dumps(valid_payload(ledger), indent=2) + "\n")
            command = [
                "python3",
                str(TOOL),
                "--verdict",
                str(verdict),
                "--ledger",
                str(ledger_path),
                "--repo-root",
                str(root),
                "--profile",
                "core",
            ]
            passing = subprocess.run(
                command,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(passing.returncode, 0, passing.stdout + passing.stderr)

            (root / "epoch").write_text("two\n")
            subprocess.run(["git", "add", "epoch"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "two"], cwd=root, check=True)
            drifted = subprocess.run(
                command,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertNotEqual(drifted.returncode, 0)
            self.assertIn("does not equal repository HEAD", drifted.stdout + drifted.stderr)

    def test_repo_head_binding_ignores_path_git_shim(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "config", "user.email", "seal-validator@example.invalid"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "config", "user.name", "seal-validator-test"],
                cwd=root,
                check=True,
            )
            (root / "epoch").write_text("one\n")
            subprocess.run(["git", "add", "epoch"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "one"], cwd=root, check=True)
            head = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                text=True,
                stdout=subprocess.PIPE,
                check=True,
            ).stdout.strip()
            ledger = valid_ledger().replace(LEDGER_COMMIT.encode("ascii"), head.encode("ascii"))
            ledger_path = root / "gate_run_ledger.validated"
            ledger_path.write_bytes(ledger)
            ledger_path.chmod(0o444)
            verdict = root / "seal_verdict.json"
            verdict.write_text(json.dumps(valid_payload(ledger), indent=2) + "\n")
            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            marker = root / "fake-git-invoked"
            fake_git = fake_bin / "git"
            fake_git.write_text(
                "#!/usr/bin/env bash\n"
                f"printf invoked > {marker!s}\n"
                "exit 97\n"
            )
            fake_git.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}:{environment.get('PATH', '')}"
            environment["GIT_DIR"] = str(root / "attacker-git-dir")
            result = subprocess.run(
                [
                    "python3",
                    str(TOOL),
                    "--verdict",
                    str(verdict),
                    "--ledger",
                    str(ledger_path),
                    "--repo-root",
                    str(root),
                    "--profile",
                    "core",
                ],
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertFalse(marker.exists(), "PATH-controlled Git shim was executed")

    def test_run_seal_checklist_refuses_preexisting_output_root(self) -> None:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        self.assertIn("SEAL_REFUSED: output root already exists", source)
        self.assertIn("if [[ -e \"$SEAL_OUT\" ]]; then", source)
        self.assertIn("mkdir -p \"$SEAL_OUT\"", source)

    def test_run_seal_checklist_adopts_final_validator(self) -> None:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        self.assertIn("scripts/lib/seal_verdict_validate.py", source)
        self.assertIn("SEAL_VERDICT_VALIDATOR_RC", source)

    def test_validator_prints_exact_rosters(self) -> None:
        for profile, expected in (("core", 20), ("full", 24)):
            result = subprocess.run(
                [
                    "python3",
                    str(TOOL),
                    "--profile",
                    profile,
                    "--print-roster",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(result.stdout.splitlines()), expected)

    def test_seal_uses_output_ledger_and_promotes_only_after_validation(self) -> None:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        self.assertIn('GATE_RUN_LEDGER_WORKING="$SEAL_OUT/gate_run_ledger.working"', source)
        self.assertNotIn("docs/.gate_run_ledger", source)
        validator = source.index('if [[ $SEAL_VERDICT_VALIDATOR_RC -ne 0 ]]')
        revalidation = source.index('capture_gate_run_ledger_binding "pre_promotion"')
        promotion = source.index('python3 "$ROOT/scripts/lib/gate_run_ledger_promote.py"')
        post_promotion_validator = source.index('seal_verdict_validator_post_promotion.log')
        post_promotion_pin = source.index('verify_selected_source_pin "post_promotion"', promotion)
        closing_validator = source.index('seal_verdict_validator_closing.log')
        self.assertLess(validator, revalidation)
        self.assertLess(revalidation, promotion)
        self.assertLess(validator, promotion)
        self.assertLess(promotion, post_promotion_validator)
        self.assertLess(post_promotion_validator, post_promotion_pin)
        self.assertLess(post_promotion_pin, closing_validator)
        self.assertIn('--ledger "$GATE_RUN_LEDGER_WORKING"', source)
        self.assertIn('--ledger "$GATE_RUN_LEDGER_VALIDATED"', source)
        self.assertGreaterEqual(source.count('--repo-root "$ROOT"'), 3)
        self.assertIn('scripts/lib/gate_run_ledger_promote.py\n)', source)
        self.assertGreaterEqual(source.count('PATH="$PIN_VERIFY_PATH" scripts/publish_pin.sh --verify'), 2)
        self.assertIn('verify_selected_source_pin "final"', source)
        self.assertIn('verify_selected_source_pin "post_promotion"', source)
        self.assertIn('"$opening_pin" != "$CURRENT_PIN"', source)
        self.assertIn('"$closing_pin" != "$CURRENT_PIN"', source)
        self.assertIn('"$verify_output" != "pin matches tree: $CURRENT_PIN"', source)
        self.assertIn(
            '"$INITIAL_PIN_VERIFY_OUTPUT" == "pin matches tree: $CURRENT_PIN"', source
        )
        self.assertIn("unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE", source)
        self.assertIn('[[ "$GATE_RUN_LEDGER_BINDING_SHA" == "$PREVALIDATOR_BOUND_SHA" ]]', source)
        self.assertIn('[[ "$GATE_RUN_LEDGER_BINDING_ROWS" == "$PREVALIDATOR_BOUND_ROWS" ]]', source)
        self.assertIn('[[ "$GATE_RUN_LEDGER_BINDING_COMMIT" == "$PREVALIDATOR_BOUND_COMMIT" ]]', source)
        self.assertNotIn('mv "$GATE_RUN_LEDGER_WORKING" "$GATE_RUN_LEDGER_VALIDATED"', source)
        self.assertIn('"gate_run_ledger"', source)
        self.assertIn('publish_pin_verify_${phase}.log', source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
