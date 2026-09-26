#!/usr/bin/env python3
"""Compare the same harmless Safe sources on two pinned Anubis binaries.

Use a fresh --out directory. This runs `check --evidence` only; it never executes
the programs. Raw bundles stay under --out, while probe-results.json contains no
machine-local paths or model values.
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


SOURCES = {
    "mixed": "fn main() {\n    let x = 1;\n    assert(x == 1);\n    assert(x == 2);\n}\n",
    "empty": "fn main() {\n    let x = 1;\n}\n",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)

    results = []
    for case, body in SOURCES.items():
        source = args.out / f"{case}.anb"
        source.write_text(body)
        for label, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
            output = args.out / f"{label}-{case}"
            output.mkdir()
            run = subprocess.run(
                [str(binary), "check", str(source), "--evidence", "--out", str(output)],
                text=True,
                capture_output=True,
                timeout=180,
                check=False,
            )
            for channel, data in (("stdout", run.stdout), ("stderr", run.stderr)):
                redacted = data.replace(str(args.out), "<OUT>").replace(str(binary), "<PIN>")
                (args.out / f"{label}-{case}.{channel}.redacted.txt").write_text(redacted)
            bundles = sorted(output.glob("evidence-*/"))
            if len(bundles) != 1:
                raise RuntimeError(f"{label}-{case}: expected one bundle, got {len(bundles)}")
            bundle = bundles[0]
            checks = json.loads((bundle / "solver.json").read_text())
            replay = json.loads((bundle / "analysis/solver_replay.json").read_text())
            results.append(
                {
                    "case": case,
                    "binary": label,
                    "binary_sha256": digest(binary),
                    "source_sha256": hashlib.sha256(body.encode()).hexdigest(),
                    "exit_code": run.returncode,
                    "solver_checks": [
                        {"name": c["name"], "status": c["status"], "detail": c["detail"]}
                        for c in checks
                    ],
                    "solver_replay": replay,
                    "manifest_sha256": digest(bundle / "MANIFEST.sha256"),
                }
            )
    (args.out / "probe-results.json").write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
