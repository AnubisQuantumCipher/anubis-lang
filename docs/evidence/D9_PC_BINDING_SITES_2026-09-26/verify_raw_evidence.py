import csv
import hashlib
import json
from pathlib import Path

def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError(f'duplicate JSON key: {key}')
        obj[key] = value
    return obj

def read_json(path):
    return json.loads(path.read_text(), object_pairs_hook=unique_object)

bundle = Path(__file__).resolve().parent
root = bundle.parents[2]
receipt = read_json(bundle / 'gate-receipt.json')
pre = read_json(bundle / 'prepatch.json')
post = read_json(bundle / 'candidate.json')
raw_pre = read_json(bundle / 'raw-prepatch.json')
raw_post = read_json(bundle / 'raw-candidate.json')
diff = read_json(bundle / 'differential.json')
sha = lambda data: hashlib.sha256(data).hexdigest()
errors = []
for name, document, schema in [
    ('receipt', receipt, 'anubis-scoped-gate-receipt/1'),
    ('prepatch', pre, 'anubis-d9q-differential/1'),
    ('candidate', post, 'anubis-d9q-differential/1'),
    ('raw-prepatch', raw_pre, 'anubis-raw-safe-checks/1'),
    ('raw-candidate', raw_post, 'anubis-raw-safe-checks/1'),
    ('differential', diff, 'anubis-d9q-differential/1'),
]:
    if document.get('schema') != schema:
        errors.append(f'{name}:schema')
for name, entry in receipt['artifacts'].items():
    path = bundle / name
    if not path.is_file() or sha(path.read_bytes()) != entry['sha256']:
        errors.append(f'artifact_hash:{name}')
for source, expected in [('compiler/src/middle/mod.rs', receipt['compiler_middle_sha256']), ('Cargo.lock', receipt['cargo_lock_sha256']), ('tests/soundness/matrix/registry.tsv', receipt['registry_sha256'])]:
    if sha((root / source).read_bytes()) != expected:
        errors.append(f'current_source_hash:{source}')
with (root / 'tests/soundness/matrix/registry.tsv').open() as f:
    registry_rows = [row for row in csv.DictReader(f, delimiter='\t') if row['id'].startswith('d9q_')]
registry = {row['id']:row for row in registry_rows}
if len(registry) != len(registry_rows):
    errors.append('duplicate_registry_case_id')
pre_rows = {row['id']:row for row in pre['rows']}
post_rows = {row['id']:row for row in post['rows']}
if len(pre_rows) != len(pre['rows']) or len(post_rows) != len(post['rows']):
    errors.append('duplicate_manifest_case_id')
if receipt['prepatch_binary_sha256'] != pre['binary_sha256'] or receipt['candidate_binary_sha256'] != post['binary_sha256']:
    errors.append('receipt_binary_identity')
ids = set(registry)
for label, rows, raw in [('pre',pre_rows,raw_pre),('post',post_rows,raw_post)]:
    if set(rows) != ids or set(raw['rows']) != ids:
        errors.append(f'{label}:id_set')
    if raw['binary_sha256'] != (pre if label == 'pre' else post)['binary_sha256']:
        errors.append(f'{label}:binary')
    for id in ids & set(rows) & set(raw['rows']):
        row = rows[id]
        rawrow = raw['rows'][id]
        if type(row['exit']) is not int or type(rawrow['exit']) is not int:
            errors.append(f'{label}:{id}:exit_type')
        src = root / f'tests/soundness/matrix/cases/{id}.anb'
        if not src.is_file() or sha(src.read_bytes()) != row['source_sha256'] or registry[id]['intent'] != row['intent']:
            errors.append(f'{label}:{id}:source')
        if row['exit'] != rawrow['exit']:
            errors.append(f'{label}:{id}:exit')
        for stream in ('stdout','stderr'):
            if sha(rawrow[stream].encode()) != row[stream+'_sha256']:
                errors.append(f'{label}:{id}:{stream}_hash')
        decoded = [json.loads(line, object_pairs_hook=unique_object) for line in rawrow['stdout'].splitlines() if line.strip()]
        if any(d.get('$type') not in ('anubis.diagnostic', 'anubis.summary') for d in decoded):
            errors.append(f'{label}:{id}:unexpected_record_type')
        if any(d.get('schema') != 'anubis-diagnostics/1' for d in decoded):
            errors.append(f'{label}:{id}:diagnostic_schema')
        diag = [{'code': d['code'], 'message': d['message'], 'status':d['status']} for d in decoded if d.get('$type') == 'anubis.diagnostic']
        summaries = [d for d in decoded if d.get('$type') == 'anubis.summary']
        if len(summaries) != 1 or not decoded or decoded[-1].get('$type') != 'anubis.summary':
            errors.append(f'{label}:{id}:summary_cardinality')
        summary = summaries[0] if summaries else None
        if diag != row['diagnostics'] or summary != row['summary']:
            errors.append(f'{label}:{id}:parsed_json')
        if row['exit'] not in (0, 1) or (row['exit'] == 0) != (summary is not None and summary.get('verdict') == 'pass'):
            errors.append(f'{label}:{id}:exit_summary')
        if summary is not None:
            expected_counts = {'disproved': 0, 'refused': len(diag), 'replay_mismatch': 0, 'total': len(diag), 'undecided': 0}
            if set(summary['counts']) != set(expected_counts) or any(type(value) is not int for value in summary['counts'].values()) or summary['counts'] != expected_counts:
                errors.append(f'{label}:{id}:diagnostic_count')
        if row['exit'] == 1 and (not diag or any(d['status'] != 'refused' for d in diag)):
            errors.append(f'{label}:{id}:untyped_refusal')
changed_ids = sorted(id for id in ids if (pre_rows[id]['exit'],pre_rows[id]['diagnostics'],pre_rows[id]['summary']) != (post_rows[id]['exit'],post_rows[id]['diagnostics'],post_rows[id]['summary']))
claimed_changed_ids = sorted(row['id'] for row in diff['changed'])
if changed_ids != claimed_changed_ids:
    errors.append('differential_changed_ids')
expected_changed_rows = []
for id in changed_ids:
    before, after = pre_rows[id], post_rows[id]
    expected_changed_rows.append({
        'id': id,
        'intent': before['intent'],
        'pre_exit': before['exit'],
        'post_exit': after['exit'],
        'pre_codes': [[d['code'], d['status']] for d in before['diagnostics']],
        'post_codes': [[d['code'], d['status']] for d in after['diagnostics']],
        'pre_summary': before['summary'],
        'post_summary': after['summary'],
    })
if diff['changed'] != expected_changed_rows:
    errors.append('differential_changed_row_content')
expected_changed = {'d9q_pc_protected_shadow_renamed_valid', 'd9q_pc_shadow_protected_valid'}
if set(changed_ids) != expected_changed:
    errors.append('unexpected_verdict_change')
for id in expected_changed:
    if pre_rows[id]['exit'] != 1 or not any(d['code'] == 'ANUBIS_IMPLICIT_FLOW' and d['status'] == 'refused' for d in pre_rows[id]['diagnostics']):
        errors.append(f'{id}:prepatch_oracle')
    if post_rows[id]['exit'] != 0 or post_rows[id]['diagnostics'] or post_rows[id]['summary']['verdict'] != 'pass':
        errors.append(f'{id}:candidate_oracle')
required_direct = {'d9q_pc_public_inner_egress_invalid', 'd9q_pc_write_before_protected_shadow_invalid', 'd9q_pc_expr_write_before_shadow_invalid', 'd9q_pc_shadow_outer_invalid'}
for id in required_direct:
    if post_rows[id]['exit'] != 1 or not any(d['code'] == 'ANUBIS_IMPLICIT_FLOW' and d['status'] == 'refused' for d in post_rows[id]['diagnostics']):
        errors.append(f'{id}:direct_refusal')
if diff['pre_manifest_sha256'] != sha((bundle/'prepatch.json').read_bytes()) or diff['candidate_manifest_sha256'] != sha((bundle/'candidate.json').read_bytes()):
    errors.append('differential_manifest_hashes')
if diff['source_identity_errors'] or diff['tool_errors']:
    errors.append('differential_error_field')
unchanged_raw = [id for id in ids if raw_pre['rows'][id] == raw_post['rows'][id]]
refused_reject = [id for id in ids if registry[id]['intent'] == 'REJECT' and post_rows[id]['exit'] != 0]
valid_refused = [id for id in ids if registry[id]['intent'] == 'ACCEPT' and post_rows[id]['exit'] != 0]
counts = {'registered_d9q_cases':len(ids),'changed_verdicts':len(changed_ids),'remaining_byte_identical_raw':len(unchanged_raw),'reject_cases_refusing':len(refused_reject),'remaining_accept_intent_refusals':len(valid_refused)}
for key,value in counts.items():
    recorded = receipt['results'].get(key)
    if type(recorded) is not int or recorded != value:
        errors.append(f'count:{key}')
if receipt['results'].get('accept_to_pass') != len(expected_changed):
    errors.append('count:accept_to_pass')
if any(type(receipt['results'].get(key)) is not int or receipt['results'][key] != 0 for key in ('tool_errors', 'source_identity_errors')):
    errors.append('receipt_error_count')
for id in ids:
    if registry[id]['intent'] == 'REJECT' and (post_rows[id]['exit'] != 1 or not any(d['status'] == 'refused' for d in post_rows[id]['diagnostics'])):
        errors.append(f'{id}:reject_oracle')
print(json.dumps({'errors':errors,'counts':counts,'changed_ids':changed_ids,'valid_refused':valid_refused,'direct_refusal_codes': {id:[d['code'] for d in post_rows[id]['diagnostics']] for id in ('d9q_pc_public_inner_egress_invalid','d9q_pc_write_before_protected_shadow_invalid','d9q_pc_expr_write_before_shadow_invalid','d9q_pc_shadow_outer_invalid')},'sha256':{'receipt':sha((bundle/'gate-receipt.json').read_bytes()),'current_middle':sha((root/'compiler/src/middle/mod.rs').read_bytes()),'registry':sha((root/'tests/soundness/matrix/registry.tsv').read_bytes())}}, indent=2))
if errors:
    raise SystemExit(1)
