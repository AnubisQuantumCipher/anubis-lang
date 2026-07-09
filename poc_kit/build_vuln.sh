#!/usr/bin/env bash
# Build the local intentionally-vulnerable gold target for PoC kit demos.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/poc_kit"
mkdir -p bin
cc -O0 -g -fno-stack-protector -o bin/vuln_local vuln_local.c
chmod +x bin/vuln_local
echo "built: $ROOT/poc_kit/bin/vuln_local"
# smoke: short input should exit 0
printf 'X' | ./bin/vuln_local >/dev/null
# smoke: long input should crash (nonzero / signal)
set +e
python3 -c 'import sys; sys.stdout.buffer.write(b"A"*80)' | ./bin/vuln_local >/dev/null 2>&1
rc=$?
set -e
if [[ $rc -eq 0 ]]; then
  echo "WARN: expected crash on 80-byte input, got exit 0" >&2
  exit 1
fi
echo "smoke ok (short=0, long crash rc=$rc)"
