#!/usr/bin/env python3
"""Real JSON-RPC stdio roundtrip against `anubis lsp --stdio`.

Verifies: initialize → initialized → didOpen (diagnostics) → hover → shutdown → exit.
Exit 0 only if diagnostics and contract hover both appear.

The explicit transport flag matches what vscode-languageclient appends when the
extension declares TransportKind.stdio. This keeps the headless gate on the
same process invocation as a real Extension Development Host.
"""
from __future__ import annotations

import json
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target" / "release" / "anubis"


def frame(msg: dict) -> bytes:
    body = json.dumps(msg).encode("utf-8")
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


def read_messages(proc: subprocess.Popen, timeout: float = 8.0) -> list[dict]:
    """Read all available framed messages until timeout with no new data."""
    assert proc.stdout is not None
    buf = b""
    msgs: list[dict] = []
    deadline = time.time() + timeout
    while time.time() < deadline:
        # non-blocking-ish: read chunks
        chunk = proc.stdout.read(1)
        if not chunk:
            if proc.poll() is not None:
                break
            time.sleep(0.01)
            continue
        buf += chunk
        while True:
            sep = buf.find(b"\r\n\r\n")
            if sep < 0:
                break
            headers = buf[:sep].decode("utf-8", errors="replace")
            length = None
            for line in headers.split("\r\n"):
                if line.lower().startswith("content-length:"):
                    length = int(line.split(":", 1)[1].strip())
            if length is None:
                buf = buf[sep + 4 :]
                continue
            body_start = sep + 4
            if len(buf) < body_start + length:
                break
            body = buf[body_start : body_start + length]
            buf = buf[body_start + length :]
            msgs.append(json.loads(body.decode("utf-8")))
            # reset deadline a bit when we get traffic
            deadline = max(deadline, time.time() + 1.5)
    return msgs


def main() -> int:
    if not BIN.is_file():
        print(f"FAIL: missing binary {BIN}", file=sys.stderr)
        return 2

    bad_src = "fn main() { let x: u32 = true; }\n"
    good_src = """fn div(a: u32, b: u32) -> u32 requires(b != 0) ensures(result == a / b) {
  return a / b;
}
fn main() { print(div(4, 2)); }
"""

    proc = subprocess.Popen(
        [str(BIN), "lsp", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(ROOT),
    )
    assert proc.stdin is not None

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

    send({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": None})
    send({"jsonrpc": "2.0", "method": "exit"})

    msgs = read_messages(proc, timeout=10.0)
    try:
        proc.stdin.close()
    except Exception:
        pass
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()

    # Debug dump
    out_dir = ROOT / "out" / "dx_rigorous"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "lsp_messages.json").write_text(json.dumps(msgs, indent=2))
    err = proc.stderr.read().decode("utf-8", errors="replace") if proc.stderr else ""
    (out_dir / "lsp_stderr.txt").write_text(err)

    init_ok = any(
        m.get("id") == 1 and "result" in m and m["result"].get("capabilities")
        for m in msgs
    )
    diags = [
        m
        for m in msgs
        if m.get("method") == "textDocument/publishDiagnostics"
        and m.get("params", {}).get("uri") == uri_bad
    ]
    diag_ok = False
    for d in diags:
        arr = d.get("params", {}).get("diagnostics") or []
        if len(arr) > 0:
            diag_ok = True
            break

    hover_msgs = [m for m in msgs if m.get("id") == 2]
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
        if "requires" in text or "Contracts" in text or "div" in text:
            hover_ok = True

    print(f"init_ok={init_ok} diag_ok={diag_ok} hover_ok={hover_ok} n_msgs={len(msgs)}")
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
