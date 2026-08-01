#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import os
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PRODUCER = ROOT / "scripts/phase1_verdict_diff.py"
PRODUCER_NAMESPACE = runpy.run_path(str(PRODUCER))
CHECKER_CHILD_ENVIRONMENT = PRODUCER_NAMESPACE["checker_child_environment"]
CANONICAL_JSON_BYTES = PRODUCER_NAMESPACE["canonical_json_bytes"]
PIN_VERIFICATION_ENVIRONMENT = PRODUCER_NAMESPACE["pin_verification_environment"]
PIN_IDENTITY_RECEIPTS = PRODUCER_NAMESPACE["pin_identity_receipts"]
PIN_IDENTITY_SNAPSHOTS = PRODUCER_NAMESPACE["pin_identity_snapshots"]
PUBLISH_REPORT_NO_CLOBBER = PRODUCER_NAMESPACE["publish_report_no_clobber"]
READ_INVENTORY = PRODUCER_NAMESPACE["read_inventory"]
READ_CURRENT_RECEIPT = PRODUCER_NAMESPACE["read_current_receipt"]
RUN_PIN_VERIFICATION = PRODUCER_NAMESPACE["run_pin_verification"]
SNAPSHOT_AUTHORITATIVE_FIXTURES = PRODUCER_NAMESPACE["snapshot_authoritative_fixtures"]
SOURCE_MANIFEST_ROWS = PRODUCER_NAMESPACE["source_manifest_rows"]
STABLE_SNAPSHOT_REGULAR = PRODUCER_NAMESPACE["stable_snapshot_regular"]
VALIDATE_OUTPUT_ROOT_EXCLUSION = PRODUCER_NAMESPACE["validate_output_root_exclusion"]
VALIDATED_SYSTEM_BASH = PRODUCER_NAMESPACE["validated_system_bash"]
VERIFY_AUTHORITATIVE_FIXTURE_SNAPSHOTS = PRODUCER_NAMESPACE[
    "verify_authoritative_fixture_snapshots"
]
INVOKE = PRODUCER_NAMESPACE["invoke"]
OPEN_STABLE_REGULAR = PRODUCER_NAMESPACE["open_stable_regular"]
SRC_TREE = "c" * 64
SRC_LIST = "d" * 64
POLICY_SHA = "e" * 64


class Phase1VerdictDiffIdentityTests(unittest.TestCase):
    def metadata_bytes(self, pin: Path, binary_sha: str, *, modern: bool) -> bytes:
        lines = [
            f"pin:    vm/pins/{pin.name}",
            f"sha256: {binary_sha}",
            "source: target/release/anubis",
            f"head:   {'0' * 40 if modern else '01234567'}",
            "utc:    2026-07-31T00:00:00Z",
        ]
        if modern:
            lines.extend(
                (
                    "pin_schema: anubis.binary-pin.v2",
                    "build_mode: technical-existing-target",
                    f"head_tree: {'1' * 40}",
                    "commit_bound: false",
                    "manifest_schema: anubis.pin-source-manifest.v2",
                    f"policy_sha256: {POLICY_SHA}",
                    "src_count: 1",
                    f"src_list_sha256: {SRC_LIST}",
                )
            )
        lines.append(f"src_tree: {SRC_TREE}")
        return ("\n".join(lines) + "\n").encode()

    def make_fixture(self, base: Path) -> tuple[Path, Path, Path, str, str]:
        base = base.resolve(strict=True)
        root = base / "repo"
        pins = root / "vm/pins"
        helper = root / "scripts/lib/native_corpus_inventory.py"
        pins.mkdir(parents=True)
        helper.parent.mkdir(parents=True)
        helper.write_text("#!/usr/bin/env python3\n")

        old_bytes = b"historical-old-pin\n"
        new_bytes = b"current-new-pin\n"
        old_sha = hashlib.sha256(old_bytes).hexdigest()
        new_sha = hashlib.sha256(new_bytes).hexdigest()
        old = pins / f"anubis-{old_sha[:12]}"
        new = pins / f"anubis-{new_sha[:12]}-src-{SRC_TREE[:12]}"
        old.write_bytes(old_bytes)
        new.write_bytes(new_bytes)
        old.chmod(0o555)
        new.chmod(0o555)

        old_meta_raw = self.metadata_bytes(old, old_sha, modern=False)
        new_meta_raw = self.metadata_bytes(new, new_sha, modern=True)
        old_meta = Path(str(old) + ".meta")
        new_meta = Path(str(new) + ".meta")
        old_meta.write_bytes(old_meta_raw)
        new_meta.write_bytes(new_meta_raw)
        old_meta.chmod(0o444)
        new_meta.chmod(0o444)
        (pins / "CURRENT").write_text(f"vm/pins/{new.name}\n")
        return root, old, new, old_sha, hashlib.sha256(old_meta_raw).hexdigest()

    def identity(
        self,
        root: Path,
        pin: Path,
        *,
        binary_sha: str | None,
        meta_sha: str | None,
        modern: bool,
    ):
        return PIN_IDENTITY_RECEIPTS(
            root.resolve(strict=True),
            pin.resolve(strict=True),
            expected_binary_sha256=binary_sha,
            expected_meta_sha256=meta_sha,
            require_modern_meta=modern,
        )

    def rewrite_meta(self, path: Path, raw: bytes) -> str:
        path.chmod(0o644)
        path.write_bytes(raw)
        path.chmod(0o444)
        return hashlib.sha256(raw).hexdigest()

    def run_producer(
        self,
        root: Path,
        old: Path,
        new: Path,
        expected_old_sha: str,
        expected_old_meta_sha: str,
        output: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        output = output or root / "out/verdict.json"
        return subprocess.run(
            [
                "/usr/bin/python3",
                "-I",
                "-B",
                str(PRODUCER),
                "--old",
                str(old),
                "--new",
                str(new),
                "--expected-old-sha256",
                expected_old_sha,
                "--expected-old-meta-sha256",
                expected_old_meta_sha,
                "--root",
                str(root),
                "--out",
                str(output),
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_legacy_old_and_modern_new_metadata_pass_identity_validation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            old_receipt, old_meta = self.identity(
                root, old, binary_sha=old_sha, meta_sha=old_meta_sha, modern=False
            )
            new_receipt, new_meta = self.identity(
                root, new, binary_sha=None, meta_sha=None, modern=True
            )
            self.assertEqual(old_receipt["sha256"], old_sha)
            self.assertNotIn("src_count", old_meta["fields"])
            self.assertEqual(new_meta["fields"]["src_count"], "1")
            self.assertEqual(new_receipt["path"], str(new.resolve()))

    def test_legacy_old_and_modern_new_metadata_pass_snapshot_validation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, old, new, old_sha, old_meta_sha = self.make_fixture(base)
            old_receipt, old_meta, old_snapshot, old_snapshot_receipt = (
                PIN_IDENTITY_SNAPSHOTS(
                    root.resolve(strict=True),
                    old.resolve(strict=True),
                    base / "snapshots/old",
                    expected_binary_sha256=old_sha,
                    expected_meta_sha256=old_meta_sha,
                    require_modern_meta=False,
                )
            )
            new_receipt, new_meta, new_snapshot, new_snapshot_receipt = (
                PIN_IDENTITY_SNAPSHOTS(
                    root.resolve(strict=True),
                    new.resolve(strict=True),
                    base / "snapshots/new",
                    expected_binary_sha256=None,
                    expected_meta_sha256=None,
                    require_modern_meta=True,
                )
            )
            self.assertEqual(old_receipt["sha256"], old_sha)
            self.assertNotIn("src_count", old_meta["fields"])
            self.assertEqual(new_meta["fields"]["src_count"], "1")
            self.assertEqual(old_snapshot.read_bytes(), old.read_bytes())
            self.assertEqual(new_snapshot.read_bytes(), new.read_bytes())
            self.assertEqual(old_snapshot_receipt["binary"]["mode_octal"], "0500")
            self.assertEqual(new_snapshot_receipt["metadata"]["mode_octal"], "0400")

    def test_source_qualified_release_pin_passes_identity_validation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, _, new, _, _ = self.make_fixture(Path(tmp))
            release = new.with_name(new.name + "-release")
            old_meta = Path(str(new) + ".meta")
            release_meta = Path(str(release) + ".meta")
            new.rename(release)
            raw = (
                old_meta.read_bytes()
                .replace(
                    f"vm/pins/{new.name}".encode(), f"vm/pins/{release.name}".encode()
                )
                .replace(
                    b"source: target/release/anubis",
                    b"source: fresh-exact-head-archive",
                )
                .replace(
                    b"build_mode: technical-existing-target",
                    b"build_mode: cargo-build-locked-release-exact-head-archive-clean-target",
                )
                .replace(b"commit_bound: false", b"commit_bound: true")
            )
            old_meta.rename(release_meta)
            self.rewrite_meta(release_meta, raw)
            receipt, metadata = self.identity(
                root, release, binary_sha=None, meta_sha=None, modern=True
            )
            self.assertEqual(metadata["fields"]["commit_bound"], "true")
            self.assertEqual(receipt["path"], str(release.resolve()))

    def test_malformed_expected_hashes_are_rejected_before_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            bad_binary = self.run_producer(root, old, new, "g" * 64, old_meta_sha)
            self.assertNotEqual(bad_binary.returncode, 0)
            self.assertIn(
                "--expected-old-sha256 must be 64 lowercase hex", bad_binary.stderr
            )
            self.assertFalse((root / "out/verdict.json").exists())
            bad_meta = self.run_producer(root, old, new, old_sha, "short")
            self.assertNotEqual(bad_meta.returncode, 0)
            self.assertIn(
                "--expected-old-meta-sha256 must be 64 lowercase hex", bad_meta.stderr
            )
            self.assertFalse((root / "out/verdict.json").exists())

    def test_producer_requires_isolated_python_startup(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            poison_dir = base / "python-poison"
            marker = base / "sitecustomize-ran"
            poison_dir.mkdir()
            (poison_dir / "sitecustomize.py").write_text(
                "from pathlib import Path\n"
                f"Path({str(marker)!r}).write_text('ran\\n')\n"
            )
            environment = dict(os.environ)
            environment["PYTHONPATH"] = str(poison_dir)

            unsafe = subprocess.run(
                [sys.executable, str(PRODUCER), "--help"],
                cwd=ROOT,
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertNotEqual(unsafe.returncode, 0)
            self.assertIn("requires isolated Python startup", unsafe.stderr)
            self.assertTrue(marker.exists())

            marker.unlink()
            safe = subprocess.run(
                ["/usr/bin/python3", "-I", "-B", str(PRODUCER), "--help"],
                cwd=ROOT,
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(safe.returncode, 0, safe.stdout + safe.stderr)
            self.assertFalse(marker.exists())
            self.assertIn("/usr/bin/python3 -I -B", safe.stdout)

    def test_inventory_helper_exact_isolated_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "sitecustomize-ran"
            poison_dir = Path(tmp) / "python-poison"
            poison_dir.mkdir()
            (poison_dir / "sitecustomize.py").write_text(
                "from pathlib import Path\n"
                f"Path({str(marker)!r}).write_text('ran\\n')\n"
            )
            environment = dict(os.environ)
            environment["PYTHONPATH"] = str(poison_dir)
            result = subprocess.run(
                [
                    "/usr/bin/python3",
                    "-I",
                    "-B",
                    "scripts/lib/native_corpus_inventory.py",
                    "--count",
                    "--root",
                    ".",
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "921")
            self.assertFalse(marker.exists())

    def test_substituted_old_pin_is_rejected_before_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, _, old_meta_sha = self.make_fixture(Path(tmp))
            result = self.run_producer(root, old, new, "0" * 64, old_meta_sha)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("old pin sha256 does not match", result.stderr)
            self.assertFalse((root / "out/verdict.json").exists())

    def test_existing_output_is_never_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            output = root / "out/verdict.json"
            output.parent.mkdir()
            output.write_text("historical\n")
            result = self.run_producer(
                root,
                old,
                new,
                old_sha,
                old_meta_sha,
                output,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("refusing to overwrite existing output", result.stderr)
            self.assertEqual(output.read_text(), "historical\n")

    def test_manifest_bound_output_destination_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            output = root / "examples/verdict.json"
            result = self.run_producer(
                root,
                old,
                new,
                old_sha,
                old_meta_sha,
                output,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "must be below the repository's excluded output root", result.stderr
            )
            self.assertFalse(output.exists())

    def test_output_symlink_components_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, old, new, old_sha, old_meta_sha = self.make_fixture(base)
            outside = base / "outside"
            outside.mkdir()
            (root / "out").symlink_to(outside, target_is_directory=True)
            result = self.run_producer(
                root,
                old,
                new,
                old_sha,
                old_meta_sha,
                root / "out/verdict.json",
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("must not contain symlink components", result.stderr)
            self.assertFalse((outside / "verdict.json").exists())

        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, old, new, old_sha, old_meta_sha = self.make_fixture(base)
            outside = base / "outside"
            outside.mkdir()
            (root / "out").mkdir()
            (root / "out/alias").symlink_to(outside, target_is_directory=True)
            result = self.run_producer(
                root,
                old,
                new,
                old_sha,
                old_meta_sha,
                root / "out/alias/verdict.json",
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("must not contain symlink components", result.stderr)
            self.assertFalse((outside / "verdict.json").exists())

    def test_atomic_publish_succeeds_without_leaving_temporary_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp).resolve(strict=True) / "nested/out.json"
            PUBLISH_REPORT_NO_CLOBBER(output, {"verdict": "PASS"})
            self.assertEqual(output.read_text(), '{\n  "verdict": "PASS"\n}\n')
            self.assertEqual(list(output.parent.glob(output.name + ".tmp.*")), [])

    def test_atomic_publish_does_not_follow_parent_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp).resolve(strict=True)
            outside = base / "outside"
            outside.mkdir()
            alias = base / "alias"
            alias.symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(SystemExit, "invalid or symlinked component"):
                PUBLISH_REPORT_NO_CLOBBER(alias / "out.json", {"verdict": "PASS"})
            self.assertFalse((outside / "out.json").exists())

    def test_concurrently_created_output_is_never_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp).resolve(strict=True) / "out.json"
            producer_os = PUBLISH_REPORT_NO_CLOBBER.__globals__["os"]
            real_link = os.link

            def competing_link(
                source,
                destination,
                *,
                src_dir_fd=None,
                dst_dir_fd=None,
                follow_symlinks=True,
            ):
                descriptor = os.open(
                    destination,
                    os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                    0o600,
                    dir_fd=dst_dir_fd,
                )
                os.write(descriptor, b"concurrent\n")
                os.close(descriptor)
                return real_link(
                    source,
                    destination,
                    src_dir_fd=src_dir_fd,
                    dst_dir_fd=dst_dir_fd,
                    follow_symlinks=follow_symlinks,
                )

            with mock.patch.object(producer_os, "link", side_effect=competing_link):
                with self.assertRaisesRegex(
                    SystemExit, "refusing to overwrite concurrently created output"
                ):
                    PUBLISH_REPORT_NO_CLOBBER(output, {"verdict": "PASS"})
            self.assertEqual(output.read_text(), "concurrent\n")
            self.assertEqual(list(output.parent.glob(output.name + ".tmp.*")), [])

    def test_expected_old_metadata_digest_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, _ = self.make_fixture(Path(tmp))
            with self.assertRaisesRegex(SystemExit, "expected-old-meta-sha256"):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha="0" * 64, modern=False
                )

    def test_wrong_basename_and_outside_pin_directory_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            wrong_name = old.with_name("anubis-deadbeefdead")
            wrong_name.write_bytes(old.read_bytes())
            wrong_name.chmod(0o555)
            wrong_meta_raw = self.metadata_bytes(wrong_name, old_sha, modern=False)
            wrong_meta = Path(str(wrong_name) + ".meta")
            wrong_meta.write_bytes(wrong_meta_raw)
            wrong_meta.chmod(0o444)
            with self.assertRaisesRegex(SystemExit, "basename"):
                self.identity(
                    root,
                    wrong_name,
                    binary_sha=old_sha,
                    meta_sha=hashlib.sha256(wrong_meta_raw).hexdigest(),
                    modern=False,
                )

            outside = Path(tmp) / old.name
            outside.write_bytes(old.read_bytes())
            outside.chmod(0o555)
            with self.assertRaisesRegex(
                SystemExit, "outside the repository pin directory"
            ):
                self.identity(
                    root, outside, binary_sha=old_sha, meta_sha=None, modern=False
                )

    def test_old_final_component_symlink_aliases_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, old, new, old_sha, old_meta_sha = self.make_fixture(base)
            aliases = (root / "vm/pins/old-alias", base / "outside-old-alias")
            for alias in aliases:
                alias.symlink_to(old)
                with self.subTest(alias=alias):
                    result = self.run_producer(root, alias, new, old_sha, old_meta_sha)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(
                        "--old must not be a final-component symlink alias",
                        result.stderr,
                    )
                    self.assertFalse((root / "out/verdict.json").exists())

    def test_new_final_component_symlink_aliases_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, old, new, old_sha, old_meta_sha = self.make_fixture(base)
            aliases = (root / "vm/pins/new-alias", base / "outside-new-alias")
            for alias in aliases:
                alias.symlink_to(new)
                with self.subTest(alias=alias):
                    result = self.run_producer(root, old, alias, old_sha, old_meta_sha)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(
                        "--new must not be a final-component symlink alias",
                        result.stderr,
                    )
                    self.assertFalse((root / "out/verdict.json").exists())

    def test_current_exact_canonical_value_has_a_stable_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, _, new, _, _ = self.make_fixture(Path(tmp))
            target, receipt = READ_CURRENT_RECEIPT(root, root / "vm/pins/CURRENT")
            self.assertEqual(target, new.resolve(strict=True))
            self.assertEqual(receipt["value"], new.relative_to(root).as_posix())
            self.assertEqual(receipt["target"], str(new.resolve(strict=True)))
            self.assertEqual(receipt["size_bytes"], len(receipt["value"].encode()) + 1)

    def test_current_symlink_and_malformed_values_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, _, new, _, _ = self.make_fixture(Path(tmp))
            current = root / "vm/pins/CURRENT"
            real = current.with_name("CURRENT.real")
            current.rename(real)
            current.symlink_to(real)
            with self.assertRaisesRegex(
                SystemExit, "CURRENT must not contain symlink components"
            ):
                READ_CURRENT_RECEIPT(root, current)

        malformed_values = (
            b"vm/pins/anubis-deadbeef",
            b" vm/pins/anubis-deadbeef\n",
            b"vm/pins/anubis-deadbeef \n",
            b"vm/pins/anubis-deadbeef\n\n",
            b"vm/pins/anubis-deadbeef\r\n",
            b"/vm/pins/anubis-deadbeef\n",
            b"vm/pins/../anubis-deadbeef\n",
            b"vm/pins/anubis/deadbeef\n",
            b"vm/pins/anubis\\deadbeef\n",
            b"vm/pins/anubis-\x00deadbeef\n",
        )
        for malformed in malformed_values:
            with (
                self.subTest(malformed=malformed),
                tempfile.TemporaryDirectory() as tmp,
            ):
                root, _, _, _, _ = self.make_fixture(Path(tmp))
                current = root / "vm/pins/CURRENT"
                current.write_bytes(malformed)
                with self.assertRaisesRegex(SystemExit, "CURRENT must"):
                    READ_CURRENT_RECEIPT(root, current)

    def test_transient_current_substitution_changes_closing_path_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, _, _ = self.make_fixture(Path(tmp))
            current = root / "vm/pins/CURRENT"
            opening_target, opening_receipt = READ_CURRENT_RECEIPT(root, current)
            saved = current.with_name("CURRENT.saved")
            current.rename(saved)
            current.write_text(f"{old.relative_to(root).as_posix()}\n")
            transient_target, _ = READ_CURRENT_RECEIPT(root, current)
            current.unlink()
            saved.rename(current)
            closing_target, closing_receipt = READ_CURRENT_RECEIPT(root, current)
            self.assertEqual(opening_target, closing_target)
            self.assertEqual(opening_target, new.resolve(strict=True))
            self.assertEqual(transient_target, old.resolve(strict=True))
            self.assertNotEqual(opening_receipt, closing_receipt)

    def test_writable_and_symlink_metadata_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            old_meta.chmod(0o644)
            with self.assertRaisesRegex(SystemExit, "must be non-writable"):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=old_meta_sha, modern=False
                )

        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            target = old_meta.with_name(old_meta.name + ".real")
            old_meta.rename(target)
            old_meta.symlink_to(target)
            with self.assertRaisesRegex(SystemExit, "regular non-symlink"):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=old_meta_sha, modern=False
                )

    def test_duplicate_metadata_key_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, _ = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            raw = old_meta.read_bytes() + f"sha256: {old_sha}\n".encode()
            meta_sha = self.rewrite_meta(old_meta, raw)
            with self.assertRaisesRegex(
                SystemExit, "duplicate pin metadata field 'sha256'"
            ):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=meta_sha, modern=False
                )

    def test_wrong_metadata_pin_and_sha_fields_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, _ = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            raw = old_meta.read_bytes().replace(
                f"vm/pins/{old.name}".encode(), b"vm/pins/anubis-wrong"
            )
            meta_sha = self.rewrite_meta(old_meta, raw)
            with self.assertRaisesRegex(SystemExit, "pin field does not match"):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=meta_sha, modern=False
                )

        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, _ = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            raw = old_meta.read_bytes().replace(old_sha.encode(), ("f" * 64).encode())
            meta_sha = self.rewrite_meta(old_meta, raw)
            with self.assertRaisesRegex(SystemExit, "sha256 does not match binary"):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=meta_sha, modern=False
                )

    def test_src_tree_and_modern_count_list_contracts_are_enforced(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, new, old_sha, _ = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            raw = old_meta.read_bytes().replace(SRC_TREE.encode(), b"invalid")
            meta_sha = self.rewrite_meta(old_meta, raw)
            with self.assertRaisesRegex(
                SystemExit, "src_tree must be 64 lowercase hex"
            ):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=meta_sha, modern=False
                )

            new_meta = Path(str(new) + ".meta")
            raw = (
                b"\n".join(
                    line
                    for line in new_meta.read_bytes().splitlines()
                    if not line.startswith((b"src_count:", b"src_list_sha256:"))
                )
                + b"\n"
            )
            self.rewrite_meta(new_meta, raw)
            with self.assertRaisesRegex(SystemExit, "current pin metadata requires"):
                self.identity(root, new, binary_sha=None, meta_sha=None, modern=True)

        with tempfile.TemporaryDirectory() as tmp:
            root, _, new, _, _ = self.make_fixture(Path(tmp))
            new_meta = Path(str(new) + ".meta")
            raw = (
                b"\n".join(
                    line
                    for line in new_meta.read_bytes().splitlines()
                    if not line.startswith((b"manifest_schema:", b"policy_sha256:"))
                )
                + b"\n"
            )
            self.rewrite_meta(new_meta, raw)
            with self.assertRaisesRegex(SystemExit, "current pin metadata requires"):
                self.identity(root, new, binary_sha=None, meta_sha=None, modern=True)

        with tempfile.TemporaryDirectory() as tmp:
            root, _, new, _, _ = self.make_fixture(Path(tmp))
            new_meta = Path(str(new) + ".meta")
            raw = new_meta.read_bytes().replace(b"src_count: 1", b"src_count: 0")
            self.rewrite_meta(new_meta, raw)
            with self.assertRaisesRegex(
                SystemExit, "src_count must be a positive integer"
            ):
                self.identity(root, new, binary_sha=None, meta_sha=None, modern=True)

        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, _ = self.make_fixture(Path(tmp))
            old_meta = Path(str(old) + ".meta")
            raw = old_meta.read_bytes() + b"src_count: 1\n"
            meta_sha = self.rewrite_meta(old_meta, raw)
            with self.assertRaisesRegex(
                SystemExit, "must be both present or both absent"
            ):
                self.identity(
                    root, old, binary_sha=old_sha, meta_sha=meta_sha, modern=False
                )

    def test_full_receipt_detects_nonwritable_mode_mutation_at_close(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root, old, _, old_sha, old_meta_sha = self.make_fixture(Path(tmp))
            _, opening_meta = self.identity(
                root, old, binary_sha=old_sha, meta_sha=old_meta_sha, modern=False
            )
            old_meta = Path(str(old) + ".meta")
            old_meta.chmod(0o400)
            _, closing_meta = self.identity(
                root, old, binary_sha=old_sha, meta_sha=old_meta_sha, modern=False
            )
            self.assertNotEqual(opening_meta, closing_meta)
            self.assertNotEqual(opening_meta["mode_octal"], closing_meta["mode_octal"])


class Phase1VerdictDiffSnapshotTests(unittest.TestCase):
    def write_checker(self, path: Path, *, reads_fixture: bool) -> None:
        source = (
            '#!/bin/sh\n[ "$1" = check ] && [ "$(cat "$2")" = ALLOW ]\n'
            if reads_fixture
            else "#!/bin/sh\nexit 0\n"
        )
        path.write_text(source)
        path.chmod(0o555)

    def replace_temporarily(self, path: Path, replacement: bytes, mode: int):
        saved = path.with_name(path.name + ".saved")
        path.rename(saved)
        path.write_bytes(replacement)
        path.chmod(mode)
        return saved

    def restore(self, path: Path, saved: Path) -> None:
        path.unlink()
        saved.rename(path)

    def make_checker_z3_binding(
        self,
        base: Path,
    ) -> tuple[
        Path,
        Path,
        dict[str, object],
        dict[str, object],
        dict[str, str],
        dict[str, object],
    ]:
        source = base / "z3-source"
        snapshot = base / "private-toolchain/bin/z3"
        source.write_text(
            "#!/bin/sh\n"
            'if [ "${1:-}" = --version ]; then\n'
            "  echo 'Z3 version 4.15.4 - private snapshot'\n"
            "  exit 0\n"
            "fi\n"
            "echo private-snapshot\n"
        )
        source.chmod(0o555)
        _, source_receipt, snapshot_receipt = STABLE_SNAPSHOT_REGULAR(
            source,
            snapshot,
            "test Z3",
            require_executable=True,
            require_nonwritable=True,
            snapshot_executable=True,
        )
        environment, contract = CHECKER_CHILD_ENVIRONMENT(snapshot, snapshot_receipt)
        return (
            source,
            snapshot,
            source_receipt,
            snapshot_receipt,
            environment,
            contract,
        )

    def test_pin_verification_uses_validated_system_bash_not_path_shim(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp).resolve(strict=True)
            root = base / "repo"
            script = root / "scripts/publish_pin.sh"
            actual_marker = base / "system-bash-ran"
            shim_marker = base / "path-bash-ran"
            bash_env_marker = base / "bash-env-ran"
            script.parent.mkdir(parents=True)
            script.write_text(f': > "{actual_marker}"\nexit 23\n')
            shim_dir = base / "shim"
            shim_dir.mkdir()
            shim = shim_dir / "bash"
            shim.write_text(f'#!/bin/sh\n: > "{shim_marker}"\nexit 0\n')
            shim.chmod(0o755)
            bash_env = base / "bash-env"
            bash_env.write_text(f': > "{bash_env_marker}"\n')
            bash, bash_receipt = VALIDATED_SYSTEM_BASH()
            environment, contract = PIN_VERIFICATION_ENVIRONMENT()
            with mock.patch.dict(
                os.environ,
                {
                    "BASH_ENV": str(bash_env),
                    "ENV": str(bash_env),
                    "PATH": str(shim_dir),
                },
                clear=False,
            ):
                receipt = RUN_PIN_VERIFICATION(root, bash, environment=environment)
            self.assertEqual(bash, Path("/bin/bash"))
            self.assertEqual(bash_receipt["owner_uid"], 0)
            self.assertTrue(bash_receipt["executable"])
            self.assertFalse(bash_receipt["writable"])
            self.assertEqual(receipt["rc"], 23)
            self.assertEqual(
                receipt["argv"][:3],
                ["/bin/bash", "--noprofile", "--norc"],
            )
            contract_payload = dict(contract)
            contract_payload.pop("contract_sha256")
            self.assertEqual(
                contract["contract_sha256"],
                hashlib.sha256(CANONICAL_JSON_BYTES(contract_payload)).hexdigest(),
            )
            self.assertNotIn("BASH_ENV", environment)
            self.assertTrue(actual_marker.exists())
            self.assertFalse(shim_marker.exists())
            self.assertFalse(bash_env_marker.exists())

    def test_inventory_ignores_path_git_shim_and_alternate_index(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp).resolve(strict=True)
            root = base / "repo"
            fixture = root / "examples/case.anb"
            helper = root / "scripts/lib/native_corpus_inventory.py"
            fixture.parent.mkdir(parents=True)
            helper.parent.mkdir(parents=True)
            fixture.write_text("fn main() {}\n")
            helper.write_text(
                "import json, subprocess, sys\n"
                "from pathlib import Path\n"
                "root = Path(sys.argv[sys.argv.index('--root') + 1])\n"
                "proc = subprocess.run("
                "['git', 'ls-files', '-z', '--', 'examples', 'tests/fixtures'], "
                "cwd=root, check=True, capture_output=True)\n"
                "files = sorted(x.decode() for x in proc.stdout.split(b'\\0') if x)\n"
                "print(json.dumps({'count': len(files), 'files': files}))\n"
            )
            subprocess.run(
                ["/usr/bin/git", "-C", str(root), "init", "-q"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            subprocess.run(
                ["/usr/bin/git", "-C", str(root), "add", "examples/case.anb"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            poison = root / "examples/poison.anb"
            poison.write_text("fn poison() {}\n")
            alternate_index = base / "alternate.index"
            alternate_environment = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            alternate_environment["GIT_INDEX_FILE"] = str(alternate_index)
            subprocess.run(
                ["/usr/bin/git", "-C", str(root), "add", "examples/poison.anb"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=alternate_environment,
            )
            poison.unlink()

            shim_dir = base / "shim"
            marker = base / "shim-ran"
            sitecustomize_marker = base / "sitecustomize-ran"
            python_poison = base / "python-poison"
            python_poison.mkdir()
            (python_poison / "sitecustomize.py").write_text(
                "from pathlib import Path\n"
                f"Path({str(sitecustomize_marker)!r}).write_text('ran\\n')\n"
            )
            shim_dir.mkdir()
            shim = shim_dir / "git"
            shim.write_text(
                f"#!/bin/sh\ntouch '{marker}'\nprintf 'examples/poison.anb\\0'\n"
            )
            shim.chmod(0o755)
            with mock.patch.dict(
                os.environ,
                {
                    "PATH": str(shim_dir),
                    "GIT_INDEX_FILE": str(alternate_index),
                    "PYTHONPATH": str(python_poison),
                },
                clear=False,
            ):
                inventory, receipt = READ_INVENTORY(root, helper)
            self.assertEqual(inventory, {"count": 1, "files": ["examples/case.anb"]})
            self.assertEqual(receipt["rc"], 0)
            self.assertEqual(receipt["argv"][:3], ["/usr/bin/python3", "-I", "-B"])
            self.assertFalse(marker.exists())
            self.assertFalse(sitecustomize_marker.exists())

    def test_checker_invocation_discards_caller_environment_poison(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            binary = base / "anubis"
            fixture = base / "case.anb"
            observed = base / "observed.env"
            z3_observed = base / "observed.z3"
            (
                z3_source,
                z3_snapshot,
                z3_source_receipt,
                z3_snapshot_receipt,
                environment,
                contract,
            ) = self.make_checker_z3_binding(base)
            binary.write_text(
                "#!/bin/sh\n"
                f"/usr/bin/env > '{observed}'\n"
                f"z3 --version > '{z3_observed}'\n"
                '[ "$ANUBIS_NATIVE_AUTHORITATIVE" = 1 ] || exit 41\n'
                '[ "$HOME" = /var/empty ] || exit 42\n'
                '[ -z "${ANUBIS_NATIVE_CONFLICT_BUDGET+x}" ] || exit 44\n'
                '[ -z "${DYLD_INSERT_LIBRARIES+x}" ] || exit 45\n'
                '[ -z "${PYTHONPATH+x}" ] || exit 46\n'
                '[ -z "${GIT_INDEX_FILE+x}" ] || exit 47\n'
                '[ -z "${RUSTFLAGS+x}" ] || exit 48\n'
                "exit 0\n"
            )
            binary.chmod(0o555)
            fixture.write_text("ALLOW")
            poison = {
                "ANUBIS_NATIVE_AUTHORITATIVE": "0",
                "ANUBIS_NATIVE_CONFLICT_BUDGET": "1",
                "ANUBIS_NATIVE_GATE_CEILING": "1",
                "ANUBIS_NATIVE_CLAUSE_CEILING": "1",
                "ANUBIS_NATIVE_CERT_WORK": "1",
                "ANUBIS_NATIVE_TIME_BUDGET_MS": "1",
                "ANUBIS_NATIVE_STATS_LOG": str(base / "native-stats"),
                "ANUBIS_NATIVE_SHADOW": "1",
                "ANUBIS_NATIVE_SHADOW_LOG": str(base / "native-shadow"),
                "ANUBIS_SHADOW_TYPES": "1",
                "ANUBIS_WRAP_SAFETY": "off",
                "ANUBIS_DUMP_SMT": "1",
                "HOME": str(base / "poison-home"),
                "PATH": str(base / "poison-bin"),
                "DYLD_INSERT_LIBRARIES": str(base / "poison.dylib"),
                "LD_PRELOAD": str(base / "poison.so"),
                "PYTHONHOME": str(base / "python-home"),
                "PYTHONPATH": str(base / "python-path"),
                "GIT_DIR": str(base / "git-dir"),
                "GIT_INDEX_FILE": str(base / "git-index"),
                "CARGO_HOME": str(base / "cargo-home"),
                "CARGO_TARGET_DIR": str(base / "cargo-target"),
                "RUSTFLAGS": "-C linker=/tmp/poison",
                "RUSTC_WRAPPER": str(base / "rustc-wrapper"),
                "CC": str(base / "cc"),
                "CXX": str(base / "cxx"),
                "CFLAGS": "-include /tmp/poison.h",
                "LDFLAGS": "-L/tmp/poison",
                "SDKROOT": str(base / "sdk"),
                "MACOSX_DEPLOYMENT_TARGET": "0.0",
            }
            poison_bin = Path(poison["PATH"])
            poison_bin.mkdir()
            poison_z3_marker = base / "poison-z3-ran"
            poison_z3 = poison_bin / "z3"
            poison_z3.write_text(
                f"#!/bin/sh\n: > '{poison_z3_marker}'\necho 'Z3 version 4.15.4 - poison'\n"
            )
            poison_z3.chmod(0o755)
            saved_source = self.replace_temporarily(
                z3_source,
                b"#!/bin/sh\necho 'Z3 version 4.15.4 - replaced source'\n",
                0o555,
            )
            with mock.patch.dict(os.environ, poison, clear=False):
                first_digest = contract["contract_sha256"]
                try:
                    result = INVOKE(
                        binary,
                        fixture,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )
                finally:
                    self.restore(z3_source, saved_source)
                _, repeated_contract = CHECKER_CHILD_ENVIRONMENT(
                    z3_snapshot,
                    z3_snapshot_receipt,
                )

            self.assertEqual(result["class"], "ACCEPT")
            digest_payload = dict(contract)
            digest_payload.pop("contract_sha256")
            self.assertEqual(
                first_digest,
                hashlib.sha256(CANONICAL_JSON_BYTES(digest_payload)).hexdigest(),
            )
            self.assertEqual(first_digest, repeated_contract["contract_sha256"])
            self.assertEqual(environment, contract["variables"])
            self.assertEqual(environment["ANUBIS_NATIVE_AUTHORITATIVE"], "1")
            self.assertEqual(
                {key for key in environment if key.startswith("ANUBIS_NATIVE_")},
                {"ANUBIS_NATIVE_AUTHORITATIVE"},
            )
            observed_environment = dict(
                line.split("=", 1)
                for line in observed.read_text().splitlines()
                if "=" in line
            )
            replaced_names = {"ANUBIS_NATIVE_AUTHORITATIVE", "HOME", "PATH"}
            for poisoned_name, poisoned_value in poison.items():
                if poisoned_name in replaced_names:
                    self.assertEqual(
                        observed_environment[poisoned_name],
                        environment[poisoned_name],
                    )
                    self.assertNotEqual(
                        observed_environment[poisoned_name], poisoned_value
                    )
                else:
                    self.assertNotIn(poisoned_name, observed_environment)
            self.assertIn(
                "Z3 version 4.15.4 - private snapshot", z3_observed.read_text()
            )
            self.assertFalse(poison_z3_marker.exists())
            _, z3_source_close = OPEN_STABLE_REGULAR(
                z3_source,
                "test Z3 source at closure",
                require_executable=True,
                require_nonwritable=True,
            )
            # The checker used the immutable private copy, while the closing
            # source receipt still exposes the replace-and-restore attempt.
            self.assertNotEqual(z3_source_receipt, z3_source_close)

    def test_checker_invocation_rejects_private_z3_path_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            binary = base / "anubis"
            fixture = base / "case.anb"
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o555)
            fixture.write_text("ALLOW")
            _, z3_snapshot, _, z3_snapshot_receipt, environment, _ = (
                self.make_checker_z3_binding(base)
            )
            saved_snapshot = self.replace_temporarily(
                z3_snapshot,
                b"#!/bin/sh\necho 'Z3 version 4.15.4 - replacement'\n",
                0o555,
            )
            try:
                with self.assertRaisesRegex(SystemExit, "path identity changed"):
                    INVOKE(
                        binary,
                        fixture,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )
            finally:
                self.restore(z3_snapshot, saved_snapshot)

    def test_transient_binary_path_substitution_cannot_change_snapshot_invocation(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            binary = base / "anubis"
            fixture = base / "case.anb"
            snapshot = base / "private/anubis"
            self.write_checker(binary, reads_fixture=False)
            fixture.write_text("ALLOW")
            _, source_receipt, snapshot_receipt = STABLE_SNAPSHOT_REGULAR(
                binary,
                snapshot,
                "test binary",
                require_executable=True,
                require_nonwritable=True,
                snapshot_executable=True,
            )
            saved = self.replace_temporarily(binary, b"#!/bin/sh\nexit 1\n", 0o555)
            _, z3_snapshot, _, z3_snapshot_receipt, environment, _ = (
                self.make_checker_z3_binding(base)
            )
            try:
                self.assertEqual(
                    INVOKE(
                        binary,
                        fixture,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )["class"],
                    "REJECT",
                )
                self.assertEqual(
                    INVOKE(
                        snapshot,
                        fixture,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )["class"],
                    "ACCEPT",
                )
            finally:
                self.restore(binary, saved)
            self.assertEqual(snapshot_receipt["sha256"], source_receipt["sha256"])
            self.assertEqual(snapshot_receipt["mode_octal"], "0500")

    def test_transient_fixture_substitution_cannot_change_snapshot_invocation(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / "repo"
            fixture = root / "tests/fixtures/case.anb"
            binary = base / "anubis"
            binary_snapshot = base / "private/bin/anubis"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("ALLOW")
            self.write_checker(binary, reads_fixture=True)
            STABLE_SNAPSHOT_REGULAR(
                binary,
                binary_snapshot,
                "test binary",
                require_executable=True,
                require_nonwritable=True,
                snapshot_executable=True,
            )
            digest = hashlib.sha256(fixture.read_bytes()).hexdigest()
            snapshots, receipt = SNAPSHOT_AUTHORITATIVE_FIXTURES(
                root,
                ["tests/fixtures/case.anb"],
                {
                    "tests/fixtures/case.anb": {
                        "path": "tests/fixtures/case.anb",
                        "sha256": digest,
                        "executable": False,
                    }
                },
                base / "private/corpus",
            )
            relative, fixture_snapshot = snapshots[0]
            saved = self.replace_temporarily(fixture, b"DENY", 0o644)
            _, z3_snapshot, _, z3_snapshot_receipt, environment, _ = (
                self.make_checker_z3_binding(base)
            )
            try:
                self.assertEqual(
                    INVOKE(
                        binary_snapshot,
                        fixture,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )["class"],
                    "REJECT",
                )
                self.assertEqual(
                    INVOKE(
                        binary_snapshot,
                        fixture_snapshot,
                        5,
                        environment=environment,
                        z3_snapshot=z3_snapshot,
                        z3_snapshot_receipt=z3_snapshot_receipt,
                    )["class"],
                    "ACCEPT",
                )
            finally:
                self.restore(fixture, saved)
            self.assertEqual(relative, "tests/fixtures/case.anb")
            self.assertEqual(
                fixture_snapshot.relative_to(base / "private/corpus").as_posix(),
                relative,
            )
            self.assertEqual(receipt["count"], 1)

    def test_fixture_snapshot_digest_and_executable_bit_are_manifest_bound(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / "repo"
            fixture = root / "examples/case.anb"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("ALLOW")
            fixture.chmod(0o755)
            digest = hashlib.sha256(fixture.read_bytes()).hexdigest()
            with self.assertRaisesRegex(SystemExit, "source fixture does not match"):
                SNAPSHOT_AUTHORITATIVE_FIXTURES(
                    root,
                    ["examples/case.anb"],
                    {
                        "examples/case.anb": {
                            "path": "examples/case.anb",
                            "sha256": digest,
                            "executable": False,
                        }
                    },
                    base / "private/corpus",
                )

    def test_fixture_snapshot_mutation_is_rejected_after_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / "repo"
            fixture = root / "examples/case.anb"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("ALLOW")
            digest = hashlib.sha256(fixture.read_bytes()).hexdigest()
            manifest_rows = {
                "examples/case.anb": {
                    "path": "examples/case.anb",
                    "sha256": digest,
                    "executable": False,
                }
            }
            snapshots, _ = SNAPSHOT_AUTHORITATIVE_FIXTURES(
                root,
                ["examples/case.anb"],
                manifest_rows,
                base / "private/corpus",
            )
            snapshot = snapshots[0][1]
            snapshot.chmod(0o600)
            snapshot.write_text("DENY")
            snapshot.chmod(0o400)
            with self.assertRaisesRegex(SystemExit, "changed after measurement"):
                VERIFY_AUTHORITATIVE_FIXTURE_SNAPSHOTS(snapshots, manifest_rows)

    def test_full_source_manifest_rows_are_bound_to_new_metadata(self) -> None:
        row = {
            "path": "tests/fixtures/case.anb",
            "sha256": hashlib.sha256(b"ALLOW").hexdigest(),
            "executable": False,
        }
        paths = [row["path"]]
        list_digest = hashlib.sha256(
            json.dumps(paths, ensure_ascii=True, separators=(",", ":")).encode("ascii")
        ).hexdigest()
        row_bytes = json.dumps(
            row,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("ascii")
        tree_digest = hashlib.sha256(row_bytes + b"\n").hexdigest()
        manifest = {
            "schema": "anubis.pin-source-manifest.v2",
            "policy_path": "scripts/lib/pin_manifest_policy.json",
            "policy_schema": "anubis.pin-manifest-policy.v2",
            "policy_sha256": POLICY_SHA,
            "count": 1,
            "list_sha256": list_digest,
            "tree_sha256": tree_digest,
            "rows": [row],
        }
        metadata = {
            "fields": {
                "manifest_schema": "anubis.pin-source-manifest.v2",
                "policy_sha256": POLICY_SHA,
                "src_count": "1",
                "src_list_sha256": list_digest,
                "src_tree": tree_digest,
            }
        }
        self.assertEqual(SOURCE_MANIFEST_ROWS(manifest, metadata), {row["path"]: row})
        metadata["fields"]["src_tree"] = "0" * 64
        with self.assertRaisesRegex(SystemExit, "does not match new pin metadata"):
            SOURCE_MANIFEST_ROWS(manifest, metadata)

    def test_output_root_must_be_explicitly_excluded_by_pin_bound_policy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            policy_relative = "scripts/lib/pin_manifest_policy.json"
            policy_path = root / policy_relative
            policy_path.parent.mkdir(parents=True)
            policy = {
                "roots": ["scripts"],
                "files": [],
                "excluded_top_level_entries": {
                    "out": {"kind": "directory", "reason": "generated evidence"}
                },
            }
            raw = (json.dumps(policy, sort_keys=True) + "\n").encode()
            policy_path.write_bytes(raw)
            digest = hashlib.sha256(raw).hexdigest()
            manifest = {
                "policy_path": policy_relative,
                "policy_sha256": digest,
            }
            rows = {
                policy_relative: {
                    "path": policy_relative,
                    "sha256": digest,
                    "executable": False,
                }
            }
            receipt = VALIDATE_OUTPUT_ROOT_EXCLUSION(root, manifest, rows)
            self.assertEqual(receipt["sha256"], digest)

            policy["roots"].append("out")
            poisoned = (json.dumps(policy, sort_keys=True) + "\n").encode()
            policy_path.write_bytes(poisoned)
            poisoned_digest = hashlib.sha256(poisoned).hexdigest()
            manifest["policy_sha256"] = poisoned_digest
            rows[policy_relative]["sha256"] = poisoned_digest
            with self.assertRaisesRegex(SystemExit, "also binds the output root"):
                VALIDATE_OUTPUT_ROOT_EXCLUSION(root, manifest, rows)


if __name__ == "__main__":
    unittest.main(verbosity=2)
