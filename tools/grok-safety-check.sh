#!/usr/bin/env bash
# grok-safety-check.sh — refuse obviously destructive or out-of-repo actions
# Source or exec before risky commands. Exit 1 on violation.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CWD="$(pwd)"

# Must be inside the repo tree
if [[ "$CWD" != "$REPO_ROOT"* ]]; then
  echo "SAFETY: cwd $CWD is outside repo $REPO_ROOT" >&2
  exit 1
fi

# Block rm -rf on anything that looks like root, home, or non-generated
for arg in "$@"; do
  case "$arg" in
    /|~|~/*|$HOME|$HOME/*|/Users/*)
      if [[ "$arg" != *"/target"* && "$arg" != *"/out"* && "$arg" != *"/tmp"* && "$arg" != *"worktrees"* ]]; then
        echo "SAFETY: refusing broad rm on $arg (only generated dirs allowed)" >&2
        exit 1
      fi
      ;;
  esac
done

# Block obvious git history destruction outside controlled branches
if [[ "$*" =~ git\ (reset\ --hard|clean\ -fdx|push\ -f) ]]; then
  if ! git branch --show-current | grep -q 'a-plus-maturity'; then
    echo "SAFETY: destructive git op only allowed on a-plus-maturity branches" >&2
    exit 1
  fi
fi

echo "safety-check: OK"
exit 0
