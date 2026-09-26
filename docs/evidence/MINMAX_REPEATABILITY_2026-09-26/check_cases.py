#!/usr/bin/env python3
"""Finite min/max CLI checks, retaining source, commands, output and pin identity."""
import argparse
import hashlib
import json
import pathlib
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("pin", type=pathlib.Path)
parser.add_argument("output", type=pathlib.Path)
args = parser.parse_args()
root = pathlib.Path.cwd()
args.pin = args.pin.resolve()
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=False)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

before = digest(args.pin)
rows = []
registry = root / "tests/soundness/matrix/registry.tsv"
for line in registry.read_text().splitlines():
    fields = line.split("\t")
    if not fields[0].startswith("ifc2r2_minmax_"):
        continue
    case, expected = fields[0], fields[3]
    source = root / "tests/soundness/matrix/cases" / (case + ".anb")
    dest = args.output / case
    dest.mkdir()
    (dest / source.name).write_bytes(source.read_bytes())
    command = [str(args.pin), "check", str(source), "--out", str(dest / "out")]
    try:
        proc = subprocess.run(command, text=True, capture_output=True, timeout=120)
        stdout, stderr, rc = proc.stdout, proc.stderr, proc.returncode
        verdict = "ACCEPT" if rc == 0 else (
            "SEC_REJECT" if "ANUBIS_SECRET_EXFILTRATION" in stdout + stderr
            else "WRONG_CLASS")
    except subprocess.TimeoutExpired as exc:
        stdout, stderr, rc, verdict = str(exc.stdout), str(exc.stderr), None, "TIMEOUT"
    (dest / "stdout.txt").write_text(stdout)
    (dest / "stderr.txt").write_text(stderr)
    row = dict(case=case, expected=expected, source_sha256=digest(source),
               command=command, returncode=rc, verdict=verdict,
               passed=verdict == ("ACCEPT" if expected == "ACCEPT" else "SEC_REJECT"))
    rows.append(row)
    print(json.dumps(row), flush=True)

after = digest(args.pin)
result = dict(pin=str(args.pin), sha256_before=before, sha256_after=after,
              rows=rows, pin_unchanged=before == after,
              all_passed=bool(rows) and all(row["passed"] for row in rows) and before == after)
(args.output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
raise SystemExit(0 if result["all_passed"] else 1)
