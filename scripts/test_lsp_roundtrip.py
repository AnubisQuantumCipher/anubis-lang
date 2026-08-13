#!/usr/bin/env python3
"""Real JSON-RPC stdio roundtrip against `anubis lsp --stdio`.

Verifies: initialize → initialized → didOpen (diagnostics) → hover → shutdown → exit.
Exit 0 only if diagnostics and contract hover both appear.

The explicit transport flag matches what vscode-languageclient appends when the
extension declares TransportKind.stdio. This keeps the headless gate on the
same process invocation as a real Extension Development Host.
"""
from __future__ import annotations

import hashlib
import json
import os
import select
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def resolve_binary() -> Path:
    configured = os.environ.get("ANUBIS_BIN")
    if configured:
        return Path(configured)
    return ROOT / "target" / "release" / "anubis"


def resolve_out_dir() -> Path:
    configured = os.environ.get("ANUBIS_LSP_OUT")
    if configured:
        return Path(configured)
    return ROOT / "out" / f"dx_rigorous.{os.getpid()}.{time.monotonic_ns()}"


def instrument_identity(binary: Path) -> dict:
    digest = hashlib.sha256()
    size = 0
    with binary.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
            size += len(chunk)
    return {
        "path": str(binary),
        "size_bytes": size,
        "sha256": digest.hexdigest(),
    }


def frame(msg: dict) -> bytes:
    body = json.dumps(msg).encode("utf-8")
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


class LspReader:
    MAX_HEADER_BYTES = 64 * 1024
    MAX_CONTENT_LENGTH = 8 * 1024 * 1024

    def __init__(self, source):
        self.fd = source if isinstance(source, int) else source.fileno()
        self.buffer = bytearray()

    def _read_more(self, deadline: float) -> None:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("LSP response deadline exceeded")
        readable, _, _ = select.select([self.fd], [], [], remaining)
        if not readable:
            raise TimeoutError("LSP response deadline exceeded")
        chunk = os.read(self.fd, 65536)
        if not chunk:
            if self.buffer:
                raise RuntimeError(
                    f"ANUBIS_LSP_TRUNCATED_FRAME: EOF with {len(self.buffer)} buffered bytes"
                )
            raise EOFError("LSP server closed stdout")
        self.buffer.extend(chunk)

    def read_message(self, timeout: float = 8.0) -> dict:
        deadline = time.monotonic() + timeout
        delimiter = b"\r\n\r\n"
        while delimiter not in self.buffer:
            self._read_more(deadline)
            if len(self.buffer) > self.MAX_HEADER_BYTES:
                raise ValueError("LSP header exceeds size limit")
        header_end = self.buffer.index(delimiter)
        header_bytes = bytes(self.buffer[:header_end])
        del self.buffer[: header_end + len(delimiter)]

        headers = {}
        for line in header_bytes.split(b"\r\n"):
            if b":" not in line:
                raise ValueError(f"malformed LSP header: {line!r}")
            key, value = line.decode("ascii").split(":", 1)
            key = key.lower()
            if key in headers:
                raise ValueError(f"duplicate LSP header: {key}")
            headers[key] = value.strip()
        if "content-length" not in headers:
            raise ValueError("missing Content-Length")
        length = int(headers["content-length"])
        if length < 0 or length > self.MAX_CONTENT_LENGTH:
            raise ValueError(f"invalid Content-Length: {length}")
        while len(self.buffer) < length:
            self._read_more(deadline)
        body = bytes(self.buffer[:length])
        del self.buffer[:length]
        return json.loads(body)


def read_messages(proc: subprocess.Popen, timeout: float = 8.0) -> list[dict]:
    """Read framed messages until server EOF, failing on the overall deadline."""
    if proc.stdout is None:
        raise RuntimeError("LSP stdout pipe is unavailable")
    reader = LspReader(proc.stdout)
    messages = []
    deadline = time.monotonic() + timeout
    while True:
        try:
            messages.append(reader.read_message(timeout=max(0.0, deadline - time.monotonic())))
        except EOFError:
            return messages


def finish_lsp_process(proc: subprocess.Popen, timeout: float = 3.0) -> bool:
    """Close stdin and reap the server; return whether termination had to be forced."""
    if proc.stdin is not None and not proc.stdin.closed:
        proc.stdin.close()
    try:
        proc.wait(timeout=timeout)
        return False
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=timeout)
        return True


def lsp_completion_ok(
    messages: list[dict], *, returncode: int | None, forced_termination: bool
) -> bool:
    shutdown_responses = [
        message
        for message in messages
        if isinstance(message, dict)
        and message.get("jsonrpc") == "2.0"
        and type(message.get("id")) is int
        and message.get("id") == 3
        and "result" in message
        and message["result"] is None
        and "error" not in message
    ]
    shutdown_ok = len(shutdown_responses) == 1
    return shutdown_ok and returncode == 0 and not forced_termination


def record_server_message(
    message: dict, messages: list[dict], responses: dict[int, dict]
) -> None:
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
        raise ValueError(f"invalid JSON-RPC envelope: {message!r}")
    if "id" in message:
        response_id = message.get("id")
        if type(response_id) is not int or "method" in message:
            raise ValueError(f"invalid server response envelope: {message!r}")
        has_result = "result" in message
        has_error = "error" in message
        if has_result == has_error:
            raise ValueError(f"response must contain exactly one of result/error: {message!r}")
        if has_error:
            raise RuntimeError(f"LSP server error response id={response_id}: {message['error']!r}")
        if response_id in responses:
            raise ValueError(f"duplicate LSP response id={response_id}")
        responses[response_id] = message
    else:
        if not isinstance(message.get("method"), str):
            raise ValueError(f"invalid server notification envelope: {message!r}")
        if "result" in message or "error" in message:
            raise ValueError(f"notification contains response fields: {message!r}")
    messages.append(message)


def read_until_response(
    reader: LspReader,
    response_id: int,
    messages: list[dict],
    responses: dict[int, dict],
    timeout: float,
) -> dict:
    deadline = time.monotonic() + timeout
    while response_id not in responses:
        message = reader.read_message(timeout=max(0.0, deadline - time.monotonic()))
        record_server_message(message, messages, responses)
    return responses[response_id]


def drain_until_eof(
    reader: LspReader,
    messages: list[dict],
    responses: dict[int, dict],
    timeout: float,
) -> None:
    deadline = time.monotonic() + timeout
    while True:
        try:
            message = reader.read_message(timeout=max(0.0, deadline - time.monotonic()))
        except EOFError:
            return
        record_server_message(message, messages, responses)


def main() -> int:
    binary = resolve_binary()
    out_dir = resolve_out_dir()
    if not binary.is_file():
        print(f"FAIL: missing binary {binary}", file=sys.stderr)
        return 2
    instrument_before = instrument_identity(binary)

    bad_src = "fn main() { let x: u32 = true; }\n"
    good_src = """fn div(a: u32, b: u32) -> u32 requires(b != 0) ensures(result == a / b) {
  return a / b;
}
fn main() { print(div(4, 2)); }
"""

    proc = subprocess.Popen(
        [str(binary), "lsp", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(ROOT),
    )
    if proc.stdin is None:
        raise RuntimeError("LSP stdin pipe is unavailable")

    def send(msg: dict) -> None:
        proc.stdin.write(frame(msg))
        proc.stdin.flush()

    uri_bad = "file:///tmp/anubis_lsp_bad.anb"
    uri_good = "file:///tmp/anubis_lsp_good.anb"

    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": os.getpid(),
                "rootUri": None,
                "capabilities": {},
                "clientInfo": {"name": "dx-rigorous", "version": "0"},
            },
        }
    )
    if proc.stdout is None:
        raise RuntimeError("LSP stdout pipe is unavailable")
    reader = LspReader(proc.stdout)
    msgs: list[dict] = []
    responses: dict[int, dict] = {}
    try:
        phase_timeout = float(os.environ.get("ANUBIS_LSP_TIMEOUT_SECS", "20"))
    except ValueError:
        print("FAIL: ANUBIS_LSP_TIMEOUT_SECS must be numeric", file=sys.stderr)
        finish_lsp_process(proc, timeout=3)
        return 2
    if not 1.0 <= phase_timeout <= 120.0:
        print("FAIL: ANUBIS_LSP_TIMEOUT_SECS must be within 1..120", file=sys.stderr)
        finish_lsp_process(proc, timeout=3)
        return 2
    try:
        read_until_response(reader, 1, msgs, responses, phase_timeout)
    except (TimeoutError, EOFError, ValueError, RuntimeError, json.JSONDecodeError) as exc:
        finish_lsp_process(proc, timeout=3)
        print(f"FAIL: initialize response: {exc}", file=sys.stderr)
        return 1
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})

    # diagnostics path
    send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri_bad,
                    "languageId": "anubis",
                    "version": 1,
                    "text": bad_src,
                }
            },
        }
    )

    # hover path
    send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri_good,
                    "languageId": "anubis",
                    "version": 1,
                    "text": good_src,
                }
            },
        }
    )
    # position of "div" in "fn div" — line 0, character 3
    send(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/hover",
            "params": {
                "textDocument": {"uri": uri_good},
                "position": {"line": 0, "character": 3},
            },
        }
    )

    try:
        read_until_response(reader, 2, msgs, responses, phase_timeout)
        diagnostics_deadline = time.monotonic() + phase_timeout
        while not any(
            message.get("method") == "textDocument/publishDiagnostics"
            and message.get("params", {}).get("uri") == uri_bad
            for message in msgs
        ):
            message = reader.read_message(
                timeout=max(0.0, diagnostics_deadline - time.monotonic())
            )
            record_server_message(message, msgs, responses)

        send({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": None})
        read_until_response(reader, 3, msgs, responses, phase_timeout)
        send({"jsonrpc": "2.0", "method": "exit"})
        proc.stdin.close()
        drain_until_eof(reader, msgs, responses, phase_timeout)
    except (TimeoutError, EOFError, ValueError, RuntimeError, json.JSONDecodeError) as exc:
        finish_lsp_process(proc, timeout=3)
        print(f"FAIL: LSP protocol read: {exc}", file=sys.stderr)
        return 1
    forced_termination = finish_lsp_process(proc, timeout=3)
    completion_ok = lsp_completion_ok(
        msgs,
        returncode=proc.returncode,
        forced_termination=forced_termination,
    ) and set(responses) == {1, 2, 3}

    # Debug dump
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "lsp_messages.json").write_text(json.dumps(msgs, indent=2))
    instrument_after = instrument_identity(binary)
    instrument_stable = instrument_before == instrument_after
    (out_dir / "instrument.json").write_text(
        json.dumps(
            {
                "before": instrument_before,
                "after": instrument_after,
                "stable": instrument_stable,
            },
            indent=2,
        )
        + "\n"
    )
    err = proc.stderr.read().decode("utf-8", errors="replace") if proc.stderr else ""
    (out_dir / "lsp_stderr.txt").write_text(err)

    expected_capabilities = {"textDocumentSync": 1, "hoverProvider": True}
    init_ok = False
    for message in msgs:
        result = message.get("result")
        capabilities = result.get("capabilities") if isinstance(result, dict) else None
        if (
            message.get("id") != 1
            or not isinstance(capabilities, dict)
            or capabilities != expected_capabilities
        ):
            continue
        init_ok = (
            type(capabilities.get("textDocumentSync")) is int
            and capabilities.get("textDocumentSync") == 1
            and capabilities.get("hoverProvider") is True
        )
    diags = [
        m
        for m in msgs
        if m.get("method") == "textDocument/publishDiagnostics"
        and m.get("params", {}).get("uri") == uri_bad
    ]
    diag_ok = False
    for d in diags if len(diags) == 1 else []:
        arr = d.get("params", {}).get("diagnostics") or []
        for diagnostic in arr:
            diagnostic_range = diagnostic.get("range") or {}
            start = diagnostic_range.get("start") or {}
            end = diagnostic_range.get("end") or {}
            positions = [
                start.get("line"),
                start.get("character"),
                end.get("line"),
                end.get("character"),
            ]
            if (
                diagnostic.get("code") == "ANUBIS_TYPECHECK"
                and diagnostic.get("source") == "anubis"
                and type(diagnostic.get("severity")) is int
                and diagnostic.get("severity") == 1
                and isinstance(diagnostic.get("message"), str)
                and "ANUBIS_TYPE_MISMATCH" in diagnostic["message"]
                and all(type(value) is int and value >= 0 for value in positions)
                and tuple(positions[2:]) > tuple(positions[:2])
            ):
                diag_ok = True
                break

    hover_msgs = [responses[2]] if 2 in responses else []
    hover_ok = False
    for h in hover_msgs:
        result = h.get("result")
        if not result:
            continue
        contents = result.get("contents")
        text = ""
        if isinstance(contents, dict):
            text = contents.get("value") or ""
        elif isinstance(contents, str):
            text = contents
        if all(marker in text for marker in ("fn div", "Contracts", "requires", "ensures")):
            hover_ok = True

    print(
        f"init_ok={init_ok} diag_ok={diag_ok} hover_ok={hover_ok} "
        f"completion_ok={completion_ok} returncode={proc.returncode} "
        f"forced_termination={forced_termination} instrument_stable={instrument_stable} "
        f"n_msgs={len(msgs)}"
    )
    if not completion_ok:
        print("FAIL: shutdown response or clean server exit missing", file=sys.stderr)
        return 1
    if not instrument_stable:
        print("FAIL: LSP instrument changed during session", file=sys.stderr)
        return 1
    if not init_ok:
        print("FAIL: initialize", file=sys.stderr)
        return 1
    if not diag_ok:
        print("FAIL: no diagnostics for type error", file=sys.stderr)
        print(json.dumps(msgs, indent=2)[:2000], file=sys.stderr)
        return 1
    if not hover_ok:
        print("FAIL: hover missing contracts", file=sys.stderr)
        print(json.dumps(hover_msgs, indent=2)[:2000], file=sys.stderr)
        return 1
    print("LSP_ROUNDTRIP: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
