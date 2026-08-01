#!/usr/bin/env python3
"""Emit the source-manifest-bound native-authoritative corpus."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from pathlib import Path
from types import ModuleType
from typing import Callable


def load_stable_manifest() -> Callable[[Path], dict[str, object]]:
    """Load the sibling helper by an exact file path under isolated Python."""
    helper = Path(__file__).resolve(strict=True).with_name("pin_manifest.py")
    module_name = "_anubis_native_corpus_pin_manifest"
    spec = importlib.util.spec_from_file_location(module_name, helper)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot create an import specification for {helper}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
    except BaseException:
        sys.modules.pop(module_name, None)
        raise
    loaded: ModuleType = module
    function = getattr(loaded, "stable_manifest", None)
    if not callable(function):
        raise RuntimeError(f"{helper} does not define callable stable_manifest")
    return function


stable_manifest = load_stable_manifest()

CORPUS_ROOTS = ("examples", "tests/fixtures")


def inventory(root: Path) -> list[str]:
    manifest = stable_manifest(root)
    files = sorted(
        row["path"]
        for row in manifest["rows"]
        if isinstance(row, dict)
        and isinstance(row.get("path"), str)
        and row["path"].endswith(".anb")
        and any(
            row["path"] == corpus_root or row["path"].startswith(f"{corpus_root}/")
            for corpus_root in CORPUS_ROOTS
        )
    )
    if not files:
        print(
            "EMPTY_NATIVE_CORPUS: source manifest contains no authoritative .anb files",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return files


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--count", action="store_true")
    mode.add_argument("--json", action="store_true")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()

    root = Path(os.path.abspath(args.root))
    files = inventory(root)
    if args.count:
        print(len(files))
    elif args.json:
        print(
            json.dumps({"count": len(files), "files": files}, indent=2, sort_keys=True)
        )
    else:
        print("\n".join(files))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
