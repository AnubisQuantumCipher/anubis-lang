#!/usr/bin/env python3
"""Finite ordinary acceptance/diagnostic checks, not crash or stress tests."""
import csv
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time

root = Path('/home/sicarii/.cache/anubis-item21/codex-continuation')
controls = root / 'closure-capture-controls-input'
out = root / 'closure-capture-controls'
out.mkdir(exist_ok=False)
pins = {
    'baseline': Path('/home/sicarii/.cache/anubis-item21/pins/anubis-ord3b'),
    'alias': Path('/home/sicarii/.cache/anubis-item21/pins/anubis-ord3x1'),
    'prototype': Path('/home/sicarii/.cache/anubis-item21/pins/anubis-codex-closure-cycle1'),
}
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
pin_hashes = {k: digest(p) for k, p in pins.items()}
rows = []
with (controls / 'manifest.tsv').open() as f:
    cases = list(csv.DictReader(f, delimiter='\t'))
for case in cases:
    path = Path(case['file'])
    if not path.is_absolute():
        path = controls / path
    snapshot = out / path.name
    snapshot.write_bytes(path.read_bytes())
    for label, pin in pins.items():
        command = [str(pin), 'check', str(snapshot)]
        start = time.monotonic()
        log = out / (path.stem + '-' + label + '.log')
        with log.open('wb') as stream:
            try:
                result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, timeout=45)
                rc = result.returncode
            except subprocess.TimeoutExpired:
                rc = 'timeout'
        text = log.read_text(errors='replace')
        row = {'case': path.name, 'intent': case['intent'], 'pin': label,
               'command': command, 'rc': rc, 'elapsed_seconds': time.monotonic() - start,
               'diagnostics': sorted(set(re.findall(r'ANUBIS_[A-Z0-9_]+', text))),
               'source_sha256': digest(snapshot), 'log_sha256': digest(log)}
        rows.append(row)
        (out / 'results.json').write_text(json.dumps({'pins': pin_hashes, 'checks': rows}, indent=2)+'\n')
        print(json.dumps(row), flush=True)
        if rc == 'timeout' or (isinstance(rc, int) and rc < 0):
            raise SystemExit('abnormal termination; stopped, no retry')
assert pin_hashes == {k: digest(p) for k, p in pins.items()}
print('Finite checks completed; inspect intent and diagnostic class before interpreting.', flush=True)
