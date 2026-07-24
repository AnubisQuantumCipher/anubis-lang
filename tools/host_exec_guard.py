#!/usr/bin/env python3
"""Anubis PreToolUse host-exec guard (Claude / Grok compatible).

Reads JSON on stdin (hook payload), extracts the shell command, and:

  exit 0  — allow
  exit 2  — deny (message on stderr)

Hard rules:
  - Block malware / weaponization denylist patterns
  - Block broad destructive ops (rm -rf /, force-push, etc.)
"""

from __future__ import annotations

import json
import re
import sys
from typing import Any

# Weaponization / unauthorized offensive tooling (defensive / authorized-lab only).
DENY_RE = re.compile(
    r"reverse[ _-]?shell|meterpreter|/dev/tcp/|nc\s+.*\s+-e\s+|mkfifo\s+.*nc\s+"
    r"|base64\s+-d.*\|\s*(ba)?sh|chattr\s+\+i|crontab\s+-|launchctl\s+load"
    r"|/etc/rc\.local|ld\.so\.preload|keylog|exfiltrat|\bc2\b|beacon\s",
    re.IGNORECASE,
)

DESTRUCTIVE_RE = re.compile(
    r"rm\s+-rf\s+(/|~(?:/|$)|/Users/[^/\s]+/?|\.git(?:/|\s|$))"
    r"|git\s+push\s+[^\n]*(-f|--force)\b"
    r"|:\(\)\s*\{"
    r"|dd\s+if=.*of=/dev/",
    re.IGNORECASE,
)


def _command_from_payload(payload: dict[str, Any]) -> str:
    ti = payload.get("tool_input") or payload.get("input") or {}
    if isinstance(ti, dict):
        for key in ("command", "cmd", "shell_command"):
            v = ti.get(key)
            if isinstance(v, str) and v.strip():
                return v
    for key in ("command", "cmd"):
        v = payload.get(key)
        if isinstance(v, str) and v.strip():
            return v
    return ""


def main() -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        return 0

    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        payload = {"tool_input": {"command": raw}}

    if not isinstance(payload, dict):
        return 0

    cmd = _command_from_payload(payload)

    if cmd and DENY_RE.search(cmd):
        print(
            "BLOCKED: malware/weaponization denylist. "
            "Anubis is defensive/authorized-only. Re-scope to a local toy target or refuse.",
            file=sys.stderr,
        )
        return 2

    if cmd and DESTRUCTIVE_RE.search(cmd):
        print(
            "BLOCKED: destructive command. Refuse or narrow scope "
            "(see tools/grok-safety-check.sh and .grok/hooks/pretool-safety.json).",
            file=sys.stderr,
        )
        return 2

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:
        raise SystemExit(0)
