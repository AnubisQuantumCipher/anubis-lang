"""Run only the registered finite bool-result cases; retain every observation."""
import collections
import csv
import hashlib
import json
import pathlib
import subprocess

root = pathlib.Path.cwd()
out = root / "out/proof-bool/cli-checks"
out.mkdir(parents=True, exist_ok=True)
pin = json.loads((root / "out/proof-bool/cli-pin.json").read_text())
binary = pathlib.Path(pin["path"])
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = digest(binary)
assert before == pin["sha256"]
registered = list(csv.DictReader((root / "tests/soundness/matrix/registry.tsv").open(), delimiter="\t"))
cases = [r for r in registered if r["id"].startswith("ifc2r2_proof_bool_")]
assert cases
rows = []
history = []
for case in cases:
    for form in case["forms"].split(","):
        suffix = ".direct.anb" if form == "direct" else ".anb"
        source = pathlib.Path("tests/soundness/matrix/cases") / (case["id"] + suffix)
        for mode in ("check", "ifc2-report"):
            command = [str(binary), mode, str(source)]
            if mode == "check":
                command += ["--out", str(out / (case["id"] + "-" + form))]
            completed = subprocess.run(command, capture_output=True, text=True, timeout=120)
            output = completed.stdout + completed.stderr
            observed = ("ACCEPT" if completed.returncode == 0 else
                        "SEC_REJECT" if completed.returncode == 1 and "ANUBIS_SECRET_EXFILTRATION" in output else "INVALID")
            expected = "SEC_REJECT" if case["intent"] == "REJECT" else "ACCEPT"
            passed = observed == expected
            row = dict(id=case["id"], form=form, mode=mode, intent=case["intent"],
                       source=str(source), source_sha256=digest(source), command=command,
                       rc=completed.returncode, stdout=completed.stdout, stderr=completed.stderr,
                       observed=observed, passed=passed)
            rows.append(row)
            print(json.dumps({k: row[k] for k in ("id", "form", "mode", "rc", "observed", "passed")}), flush=True)
            if mode == "check":
                result = "compare" if form == "direct" else "PASS" if passed else "INVALID" if observed == "INVALID" else "FAIL-silent-accept" if observed == "ACCEPT" else "FAIL-wrong-class"
                history.append([case["id"], form, "proof-bool-focused-b835b94b", pin["code_commit"], before, observed, result])
after = digest(binary)
assert before == after
receipt = dict(pin=pin, binary_sha256_before=before, binary_sha256_after=after,
               scope="Only registered ifc2r2_proof_bool_ cases and listed forms; full Safe check plus IFC2 report. No full-matrix claim.",
               rows=rows, summary=dict(collections.Counter("PASS" if r["passed"] else "FAIL" for r in rows)))
(out / "results.json").write_text(json.dumps(receipt, indent=2) + "\n")
with (out / "history.tsv").open("w") as f:
    csv.writer(f, delimiter="\t", lineterminator="\n").writerows(history)
print(json.dumps(receipt["summary"]))
raise SystemExit(0 if all(r["passed"] for r in rows) else 1)
