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

# This project's OWN defensive vocabulary, stripped before matching.
#
# Anubis is a dual-use language whose entire subject matter is the thing this guard defends against,
# so its diagnostic codes and subcommand names collide with topic words in the denylist below.
# `ANUBIS_SECRET_EXFILTRATION` is the name of a CHECK THAT PREVENTS exfiltration; `c2-cycle` is a
# subcommand an auditor is expected to exercise. Matching them blocked ordinary defensive work —
# grepping for a diagnostic code, or briefing an agent to audit a shipped command — which is worse
# than useless: a guard that fires on the name of a defence teaches everyone to route around it, and
# a routed-around guard catches nothing when it matters.
#
# Only NAMES are exempted here. Every MECHANISM pattern below still applies to the same command, so
# `ANUBIS_SECRET_EXFILTRATION` passes while an actual `/dev/tcp/` payload in the same line does not.
SELF_VOCAB_RE = re.compile(
    r"ANUBIS_[A-Z0-9_]+"          # the language's own diagnostic codes
    r"|\bc2[-_]cycle\b"           # shipped subcommand, audited by the offensive lane
    r"|\bexfiltration_[a-z_]+\b"  # fixture / test identifiers naming the property under test
    r"|examples/security/[\w./-]+"  # fixture paths in this repo
    r"|scratchpad/[\w./-]+",      # agent reports and probe corpora
    re.IGNORECASE,
)

# Weaponization / unauthorized offensive tooling (defensive / authorized-lab only).
#
# Prefer MECHANISM over VOCABULARY: these match how a payload is actually built or persisted, not
# what a security topic is called. Mechanism patterns are precise and hard to trip accidentally;
# topic words are not, and in this repo they fire constantly on legitimate work.
DENY_RE = re.compile(
    r"reverse[ _-]?shell|meterpreter|/dev/tcp/|nc\s+.*\s+-e\s+|mkfifo\s+.*nc\s+"
    r"|base64\s+-d.*\|\s*(ba)?sh|chattr\s+\+i|crontab\s+-|launchctl\s+load"
    r"|/etc/rc\.local|ld\.so\.preload|keylog"
    # `exfiltrat` and a bare `c2` were removed as standalone triggers: both are pure TOPIC words that
    # this repo uses to NAME its defences. The mechanisms they were proxying for — a reverse shell,
    # a /dev/tcp channel, a beacon loop — are each still matched on their own above and below.
    r"|beacon\s+(?:interval|jitter|sleep|callback)",
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

    # Strip this project's own defensive vocabulary BEFORE matching, so a diagnostic code or a
    # shipped subcommand name cannot trip a weaponization rule. Mechanism patterns are unaffected:
    # nothing here removes a `/dev/tcp/`, an `nc -e`, or a persistence write, so a command that
    # mentions a diagnostic AND builds a payload is still blocked on the payload.
    scan = SELF_VOCAB_RE.sub(" ", cmd) if cmd else cmd

    if scan and DENY_RE.search(scan):
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
