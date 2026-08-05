#!/usr/bin/env python3
"""Verify a staged or downloaded Anubis public release tree.

Ships INSIDE the release so a stranger can re-run it without trusting this repository.

What it proves
  1. Every file listed in ``checksums/SHA256SUMS`` exists and hashes to the recorded digest.
  2. Every file present under ``public/`` (other than the manifest itself) is LISTED. An
     unlisted file is a failure: a manifest you can add files to proves nothing.
  3. The shipped binary matches ``provenance/build.json::binary_sha256``.
  4. When hosted-CI evidence is bundled, its attestation names the same commit as
     ``provenance/source.json`` and its verdict is ``HOSTED_PASS``.

What it does NOT prove
  Nothing about the language's soundness. It is an ASSET-INTEGRITY and SOURCE-BINDING check.
  Re-running the gates is a separate act; see ``docs/CLAIMS.md`` for the claim boundary.

Fails CLOSED: any missing input, unreadable file, or unparsable manifest is a FAIL, never a skip.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, help="the `public/` directory, or its parent")
    args = ap.parse_args()

    root = Path(args.root).resolve()
    if (root / "checksums" / "SHA256SUMS").is_file():
        public = root
    elif (root / "public" / "checksums" / "SHA256SUMS").is_file():
        public = root / "public"
    else:
        print("FAIL: no checksums/SHA256SUMS under --root", file=sys.stderr)
        return 1

    manifest = public / "checksums" / "SHA256SUMS"
    failures: list[str] = []
    listed: set[Path] = set()

    for lineno, line in enumerate(manifest.read_text().splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            digest, rel = line.split(None, 1)
        except ValueError:
            failures.append(f"{manifest.name}:{lineno}: unparsable line")
            continue
        target = (public / rel.lstrip("./")).resolve()
        listed.add(target)
        if not target.is_file():
            failures.append(f"missing: {rel}")
            continue
        actual = sha256(target)
        if actual != digest:
            failures.append(f"digest mismatch: {rel}\n  expected {digest}\n  actual   {actual}")

    # An unlisted file under public/ means the manifest does not cover the tree.
    for path in sorted(public.rglob("*")):
        if path.is_file() and path.resolve() != manifest.resolve():
            if path.resolve() not in listed:
                failures.append(f"present but NOT listed in the manifest: {path.relative_to(public)}")

    build_json = public / "provenance" / "build.json"
    source_json = public / "provenance" / "source.json"
    if not build_json.is_file() or not source_json.is_file():
        failures.append("provenance/build.json or provenance/source.json is missing")
    else:
        build = json.loads(build_json.read_text())
        source = json.loads(source_json.read_text())
        binary = public / "binary" / "macos-arm64" / "anubis"
        if not binary.is_file():
            failures.append("binary/macos-arm64/anubis is missing")
        else:
            actual = sha256(binary)
            if actual != build.get("binary_sha256"):
                failures.append(
                    f"binary digest does not match provenance/build.json\n"
                    f"  recorded {build.get('binary_sha256')}\n  actual   {actual}"
                )

        attestation = public / "evidence" / "hosted-ci" / "attestation_identity.txt"
        gate = public / "evidence" / "hosted-ci" / "gate_report.json"
        if attestation.is_file() or gate.is_file():
            if not (attestation.is_file() and gate.is_file()):
                failures.append("hosted-ci evidence is partial; both attestation and gate report are required")
            else:
                commit = source.get("commit", "")
                if f"github_sha={commit}" not in attestation.read_text():
                    failures.append(f"hosted-ci attestation does not bind commit {commit}")
                if json.loads(gate.read_text()).get("verdict") != "HOSTED_PASS":
                    failures.append("hosted-ci verdict is not HOSTED_PASS")

    if failures:
        print("VERIFY_RELEASE: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"VERIFY_RELEASE: PASS  files={len(listed)}  root={public}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
