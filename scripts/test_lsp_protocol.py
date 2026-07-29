#!/usr/bin/env python3
import importlib.util
import json
import os
import subprocess
import sys
import threading
import time
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("test_lsp_roundtrip.py")
SPEC = importlib.util.spec_from_file_location("anubis_lsp_roundtrip", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load LSP roundtrip module from {MODULE_PATH}")
LSP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LSP)


def frame(payload):
    body = json.dumps(payload).encode("utf-8")
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


class LspProtocolReaderTests(unittest.TestCase):
    def test_rejects_duplicate_content_length(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)
        os.write(write_fd, b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}")
        os.close(write_fd)
        try:
            with self.assertRaisesRegex(ValueError, "duplicate"):
                reader.read_message(timeout=1.0)
        finally:
            os.close(read_fd)

    def test_rejects_oversized_unterminated_header(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)

        def writer():
            os.write(write_fd, b"X" * (reader.MAX_HEADER_BYTES + 1))
            os.close(write_fd)

        thread = threading.Thread(target=writer)
        thread.start()
        try:
            with self.assertRaisesRegex(ValueError, "header exceeds"):
                reader.read_message(timeout=1.0)
        finally:
            os.close(read_fd)
            thread.join()

    def test_reads_fragmented_frame(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)
        expected = {"jsonrpc": "2.0", "id": 7, "result": {"ok": True}}

        def writer():
            payload = frame(expected)
            for index in range(0, len(payload), 3):
                os.write(write_fd, payload[index : index + 3])
            os.close(write_fd)

        thread = threading.Thread(target=writer)
        thread.start()
        try:
            self.assertEqual(reader.read_message(timeout=1.0), expected)
        finally:
            os.close(read_fd)
            thread.join()

    def test_preserves_buffered_second_frame(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)
        first = {"jsonrpc": "2.0", "id": 1, "result": {}}
        second = {"jsonrpc": "2.0", "id": 2, "result": None}
        os.write(write_fd, frame(first) + frame(second))
        os.close(write_fd)
        try:
            self.assertEqual(reader.read_message(timeout=1.0), first)
            self.assertEqual(reader.read_message(timeout=1.0), second)
        finally:
            os.close(read_fd)

    def test_rejects_truncated_header_and_body_at_eof(self):
        for payload in [b"Content-Len", b"Content-Length: 5\r\n\r\n{}"]:
            with self.subTest(payload=payload):
                read_fd, write_fd = os.pipe()
                reader = LSP.LspReader(read_fd)
                os.write(write_fd, payload)
                os.close(write_fd)
                try:
                    with self.assertRaisesRegex(RuntimeError, "ANUBIS_LSP_TRUNCATED_FRAME"):
                        reader.read_message(timeout=1.0)
                finally:
                    os.close(read_fd)

    def test_empty_eof_is_clean(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)
        os.close(write_fd)
        try:
            with self.assertRaises(EOFError):
                reader.read_message(timeout=1.0)
        finally:
            os.close(read_fd)

    def test_idle_pipe_honors_deadline(self):
        read_fd, write_fd = os.pipe()
        reader = LSP.LspReader(read_fd)
        started = time.monotonic()
        try:
            with self.assertRaises(TimeoutError):
                reader.read_message(timeout=0.05)
            self.assertLess(time.monotonic() - started, 0.5)
        finally:
            os.close(read_fd)
            os.close(write_fd)

    def test_completion_requires_shutdown_response(self):
        self.assertFalse(LSP.lsp_completion_ok([], returncode=0, forced_termination=False))
        self.assertTrue(
            LSP.lsp_completion_ok(
                [{"jsonrpc": "2.0", "id": 3, "result": None}],
                returncode=0,
                forced_termination=False,
            )
        )

        duplicate = [
            {"jsonrpc": "2.0", "id": 3, "result": None},
            {"jsonrpc": "2.0", "id": 3, "result": None},
        ]
        self.assertFalse(
            LSP.lsp_completion_ok(duplicate, returncode=0, forced_termination=False)
        )

    def test_tracker_rejects_duplicate_response_ids(self):
        messages = []
        responses = {}
        response = {"jsonrpc": "2.0", "id": 1, "result": {}}
        LSP.record_server_message(response, messages, responses)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            LSP.record_server_message(response, messages, responses)

    def test_tracker_rejects_error_responses(self):
        with self.assertRaisesRegex(RuntimeError, "server error"):
            LSP.record_server_message(
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {"code": -32603, "message": "boom"},
                },
                [],
                {},
            )

    def test_tracker_rejects_malformed_jsonrpc_envelopes(self):
        for message in (
            {"jsonrpc": "1.0", "id": 1, "result": {}},
            {"jsonrpc": "2.0", "id": 1},
            {"jsonrpc": "2.0", "method": 7, "params": {}},
        ):
            with self.subTest(message=message):
                with self.assertRaises(ValueError):
                    LSP.record_server_message(message, [], {})

    def test_completion_rejects_crash_or_forced_termination(self):
        shutdown = [{"jsonrpc": "2.0", "id": 3, "result": None}]
        self.assertFalse(LSP.lsp_completion_ok(shutdown, returncode=1, forced_termination=False))
        self.assertFalse(LSP.lsp_completion_ok(shutdown, returncode=0, forced_termination=True))

    def test_forced_termination_reaps_process(self):
        proc = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            forced = LSP.finish_lsp_process(proc, timeout=0.05)
            self.assertTrue(forced)
            self.assertIsNotNone(proc.returncode)
        finally:
            for stream in (proc.stdin, proc.stdout, proc.stderr):
                if stream is not None and not stream.closed:
                    stream.close()


if __name__ == "__main__":
    unittest.main()
