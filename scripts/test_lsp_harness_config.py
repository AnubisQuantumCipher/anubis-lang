#!/usr/bin/env python3
import hashlib
import importlib.util
import os
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("test_lsp_roundtrip.py")
SPEC = importlib.util.spec_from_file_location("anubis_lsp_roundtrip", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load LSP roundtrip module from {MODULE_PATH}")
LSP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LSP)


class LspHarnessConfigTests(unittest.TestCase):
    def test_dx_gate_wires_zero_test_assertions_and_protocol_poisons(self):
        source = (Path(__file__).parent / "run_dx_gate.sh").read_text()
        helper = re.search(r"run_unit_filter\(\) \{(?P<body>.*?)^\}", source, re.S | re.M)
        if helper is None:
            self.fail("run_unit_filter helper missing")
        body = helper.group("body")
        self.assertLess(body.index("cargo test"), body.index("assert_rust_tests_exercised"))
        for phase in ("phase5_", "phase6_"):
            pattern = re.compile(
                rf'cargo test[^\n]+{phase}[^\n]*>"\$OUT/(?P<log>p[56]\.log)".*?'
                rf'assert_rust_tests_exercised "\$OUT/(?P=log)" "{phase}"',
                re.S,
            )
            self.assertRegex(source, pattern)
        self.assertIn("python3 scripts/test_lsp_protocol.py", source)
        self.assertIn("python3 scripts/test_lsp_harness_config.py", source)

    def test_binary_honors_anubis_bin(self):
        with mock.patch.dict(os.environ, {"ANUBIS_BIN": "/tmp/anubis-pin"}, clear=False):
            self.assertEqual(LSP.resolve_binary(), Path("/tmp/anubis-pin"))

    def test_output_honors_private_directory(self):
        with mock.patch.dict(
            os.environ, {"ANUBIS_LSP_OUT": "/tmp/anubis-private-lsp"}, clear=False
        ):
            self.assertEqual(LSP.resolve_out_dir(), Path("/tmp/anubis-private-lsp"))

    def test_default_output_is_unique_per_run(self):
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("ANUBIS_LSP_OUT", None)
            first = LSP.resolve_out_dir()
            second = LSP.resolve_out_dir()
        self.assertEqual(first.parent, LSP.ROOT / "out")
        self.assertNotEqual(first, second)

    def test_instrument_identity_records_path_size_and_sha256(self):
        with tempfile.TemporaryDirectory() as temp:
            binary = Path(temp) / "anubis-pin"
            binary.write_bytes(b"frozen-anubis")
            identity = LSP.instrument_identity(binary)
            self.assertEqual(identity["path"], str(binary))
            self.assertEqual(identity["size_bytes"], len(b"frozen-anubis"))
            self.assertEqual(
                identity["sha256"], hashlib.sha256(b"frozen-anubis").hexdigest()
            )


if __name__ == "__main__":
    unittest.main()
