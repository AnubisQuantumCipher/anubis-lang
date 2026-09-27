#!/usr/bin/env python3
"""Lead-run unit verification. Run inside capped.sh; retain every subprocess log."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

wt, target, pin, out = map(Path, sys.argv[1:])
assert all(p.is_absolute() for p in (wt, target, pin, out))
assert wt.resolve() != Path('/home/sicarii/Projects/anubis-lang').resolve()
assert subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=wt, text=True).strip() == str(wt)
out.mkdir(exist_ok=False)
os.chdir(wt)
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(target), TMPDIR='/home/sicarii/.cache/anubis-item21/tmp',
           ANUBIS_BIN=str(pin), PYTHONUNBUFFERED='1')
results = []

def digest(path):
    return hashlib.file_digest(open(path, 'rb'), 'sha256').hexdigest()

def source_manifest():
    files = subprocess.check_output(['git', 'ls-files', '-z'], cwd=wt).decode().split('\0')
    return {f: digest(wt / f) for f in files if f and
            (f.endswith('.rs') or f.endswith('Cargo.toml') or f.endswith('Cargo.lock'))}

manifest = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            'pin': str(pin), 'pin_sha256': digest(pin), 'sources': source_manifest()}
assert manifest['pin_sha256'] == digest(target / 'release/anubis'), 'pin/build mismatch'
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
(out / 'source.diff').write_bytes(subprocess.check_output(['git', 'diff', '--binary']))

def run(name, command, cwd=wt, required=True):
    print('START', name, flush=True)
    log = out / (name + '.log')
    with log.open('wb') as stream:
        rc = subprocess.run(command, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT).returncode
    row = {'name': name, 'command': command, 'cwd': str(cwd), 'rc': rc, 'required': required}
    results.append(row)
    (out / 'steps.json').write_text(json.dumps(results, indent=2) + '\n')
    print('END', name, 'rc=', rc, flush=True)
    return log.read_text(errors='replace'), row

for name, command in (
    ('tests-release', ['cargo', 'test', '--workspace', '--release', '--no-fail-fast']),
    ('solver-debug', ['cargo', 'test', '-p', 'anubis-solver', '--no-fail-fast']),
):
    text, row = run(name, command)
    summaries = re.findall(r'^test result: .+$', text, re.M)
    row['summaries'] = summaries
    row['complete'] = bool(summaries) and not re.search(r'^test result: FAILED', text, re.M)
    for line in summaries:
        print(line, flush=True)

# Preserve the actual grader and all per-case output. Only its scratch destination
# and cleanup differ from the repository runner; its classifier is unchanged.
matrix = out / 'matrix'
matrix.mkdir()
shutil.copytree(wt / 'tests/soundness/matrix/cases', matrix / 'cases')
shutil.copy2(wt / 'tests/soundness/matrix/registry.tsv', matrix / 'registry.tsv')
runner = (wt / 'tests/soundness/matrix/run.sh').read_text()
old = 'OUT=$(mktemp -d "${TMPDIR:-/tmp}/anubis-matrix.XXXXXX")'
assert runner.count(old) == 1 and runner.count('rm -rf "$OUT"') == 1
runner = runner.replace(old, 'OUT="$HERE/raw"; mkdir "$OUT"').replace('rm -rf "$OUT"', ': # outputs retained by verify_unit.py')
(matrix / 'run.sh').write_text(runner)
text, row = run('matrix', ['bash', str(matrix / 'run.sh'), str(pin)])
rows = [l.split('\t') for l in (matrix / 'raw/results.tsv').read_text().splitlines()[1:]]
expected = [l for l in (matrix / 'registry.tsv').read_text().splitlines()[1:] if l]
primary = [r for r in rows if r[1] != 'direct']
row['complete'] = len(primary) == len(expected) and len({r[0] for r in primary}) == len(expected)
row['summary'] = re.findall(r'^ALL:.*$', text, re.M)
print('\n'.join(row['summary']), flush=True)

corpus = out / 'corpus'
files = subprocess.check_output(['git', 'ls-files', '-z', 'examples', 'tests/fixtures']).decode().split('\0')
for f in filter(None, files):
    dest = corpus / f
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(wt / f, dest)
run('corpus', ['bash', '/home/sicarii/.cache/anubis-item21/handoff/scripts/corpus_cert.sh',
               str(pin), str(out / 'corpus.tsv'), str(corpus)])
def table(path):
    return {r[0]: r[1:] for r in (l.split('\t') for l in path.read_text().splitlines())}
base = table(Path('/home/sicarii/.cache/anubis-item21/handoff/scripts/corp_old.tsv'))
current = table(out / 'corpus.tsv')
comparison = {'baseline_rows': len(base), 'current_rows': len(current),
              'missing': sorted(base.keys() - current.keys()), 'added': sorted(current.keys() - base.keys()),
              'changes': {k: {'before': base[k], 'after': current[k]} for k in base.keys() & current.keys() if base[k] != current[k]}}
(out / 'corpus-comparison.json').write_text(json.dumps(comparison, indent=2) + '\n')
print('CORPUS', json.dumps(comparison), flush=True)

local_bin = wt / 'target/release/anubis'
local_bin.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(pin, local_bin.with_suffix('.verify-new'))
local_bin.with_suffix('.verify-new').chmod(0o755)
local_bin.with_suffix('.verify-new').replace(local_bin)
for gate in ('run_nexus_gate.sh', 'run_package_gate.sh', 'run_security_fixtures.sh',
             'run_language_fixtures.sh', 'run_docs_drift_gate.sh',
             'run_walker_completeness_gate.sh', 'run_phase3_label_census.sh',
             'test_phase_metrics_ledger.sh'):
    text, row = run(gate, ['bash', 'scripts/' + gate])
    print('\n'.join(text.splitlines()[-3:]), flush=True)

assert digest(pin) == manifest['pin_sha256'], 'pin changed during verification'
assert source_manifest() == manifest['sources'], 'source changed during verification'
(out / 'steps.json').write_text(json.dumps(results, indent=2) + '\n')
failed = [r['name'] for r in results if r['required'] and (r['rc'] != 0 or not r.get('complete', True))]
if comparison['changes'] or comparison['missing']:
    failed.append('corpus-comparison')
print('VERIFICATION_COMPLETED', json.dumps({'failed_steps': failed, 'matrix_all_pass': all(r[-1] == 'PASS' for r in primary)}), flush=True)
sys.exit(bool(failed))
