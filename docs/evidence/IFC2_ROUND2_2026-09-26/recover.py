#!/usr/bin/env python3
"""Copy historical IFC review evidence; never invokes compiler or recovered harnesses."""
from pathlib import Path
import collections,csv,hashlib,io,json,shutil,tarfile
BASE=Path(__file__).resolve().parent
CACHE=Path('/home/sicarii/.cache/anubis-item21')
REVIEW=Path('/home/sicarii/.cache/anubis-review-esc/ifc2-r2')
manifest=[]
def digest(b):return hashlib.sha256(b).hexdigest()
def copy(src,rel):
    b=src.read_bytes(); dst=BASE/rel;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(b)
    manifest.append({'source':str(src),'recovered':rel,'sha256':digest(b),'bytes':len(b)})
for name in ['review-ifc2-r2-raw.json','review-ifc2-r2-raw.txt']:
    copy(CACHE/name,'original/'+name)
copy(CACHE/'handoff/workflows/review-ifc2-r2.js','original/review-ifc2-r2.js')
copy(CACHE/'ifc2/review2/found.json','original/partial-found.json')
for p in sorted((CACHE/'ifc2/review2/src').glob('*.anb')):
    copy(p,'original/partial-src/'+p.name)
for p in sorted((REVIEW/'verify').iterdir()):
    if p.is_file() and (p.suffix in {'.json','.py','.log'}):copy(p,'verifier/'+p.name)
for p in sorted((REVIEW/'verify/p').glob('*.anb')):copy(p,'verifier/p/'+p.name)
raw=json.loads((CACHE/'review-ifc2-r2-raw.txt').read_text())
findings=raw['result']['findings']; verdicts={x['id']:x for x in raw['result']['verdicts']}
results=json.loads((REVIEW/'verify/results.json').read_text())
# Retain original probe outputs, controls, traces, harnesses and pinned source snapshots.
# Generated native binaries/build trees and duplicate generated IR/evidence are excluded.
archive_entries=[];excluded=[];source_matches=collections.defaultdict(list)
with tarfile.open(BASE/'original/probe-text-evidence.tar.gz','w:gz') as tar:
    for p in sorted(REVIEW.rglob('*')):
        if not p.is_file():continue
        rel=p.relative_to(REVIEW);reason=None
        if p.is_symlink():reason='symlink'
        elif any(x in {'tmp','run','target'} for x in rel.parts):reason='generated runtime/build directory'
        elif p.name.endswith(('.mono.json','.ast.json','.rmeta','.rlib','.d','.sarif','.entitlements','.sha256','.timestamp','.smt2')):reason='generated compiler metadata'
        elif '.anubis/' in str(rel) or 'evidence/' in str(rel):reason='generated compiler evidence directory'
        elif p.suffix in {'.anubis','.so','.o'}:reason='generated compiler/binary artifact'
        if reason:
            excluded.append({'source':str(p),'reason':reason});continue
        b=p.read_bytes()
        try:t=b.decode('utf-8')
        except UnicodeDecodeError:t=None
        if b'\0' in b or t is None:
            excluded.append({'source':str(p),'reason':'binary or non-UTF8'});continue
        info=tarfile.TarInfo(str(rel));info.size=len(b);info.mode=0o644;info.mtime=0
        tar.addfile(info,io.BytesIO(b))
        archive_entries.append({'source':str(p),'member':str(rel),'sha256':digest(b),'bytes':len(b)})
        if p.suffix=='.anb':
            for f in findings:
                if t.strip()==f['program'].strip():source_matches[f['id']].append(str(rel))
archive=(BASE/'original/probe-text-evidence.tar.gz').read_bytes()
manifest.append({'source':str(REVIEW),'recovered':'original/probe-text-evidence.tar.gz','sha256':digest(archive),'bytes':len(archive),'selection':'archive-manifest.json'})

def hazards(f):
    p=f['program'];i=f['id'];h=[]
    if f['kind']=='robustness':h+=['Crash, resource exhaustion, or analysis-limit probe: disposable guest only, with memory/time/stack caps; do not host-check it.']
    if any(x in i for x in ('trap','panic','exit')) or any(x in p for x in ('panic(', 'exit(')):
        h+=['Intentional trap or process-exit witness: disposable guest only; preserve stdout, stderr and exit status.']
    if 'poc_kit' in i or any(x in p for x in ('cyclic(', 'p64(', 'p32(')):
        h+=['Exploit-kit-class builtin: mandatory disposable-guest isolation even if the old runtime did not require --allow-research.']
    if any(x in p for x in ('write_file(', 'append_file(', 'delete_file(', 'remove_file(', 'read_file(')):
        h+=['Filesystem fixture: inspect exact paths and verifier setup before execution; isolate and supply deterministic fixture files.']
    if any(x in p for x in ('/dev/','/proc/')):h+=['Special-device output path: never execute on the host; preserve descriptor behavior inside guest.']
    if any(x in p for x in ('input(', 'read_line(')):h+=['Stdin-consuming witness: use retained verifier stdin exactly; uncontrolled input changes the claim.']
    if any(x in p for x in ('proof_commit_', 'proof_input_')):h+=['Proof-runtime parity: native stub behavior is not evidence of zkVM journal behavior.']
    if not h:h=['Source appears to be an ordinary Safe-mode semantic probe; still inspect before execution and pin/cap the checker. Runtime differential replay should use a disposable guest.']
    return h
verifier_inputs=collections.defaultdict(list)
for vp in sorted((REVIEW/'verify').glob('findings_*.json')):
    vd=json.loads(vp.read_text())
    if isinstance(vd,list):
        for item in vd:
            if isinstance(item,dict) and 'id' in item:verifier_inputs[item['id']].append({'path':'verifier/'+vp.name,'record':item})
rows=[]
for f in findings:
    i=f['id'];src=BASE/'src'/f'{i}.anb';src.parent.mkdir(exist_ok=True)
    source_text=f['program'];source_note='Exact program from original review.'
    if i=='r2ctl_R01_shift_register_hits_loop_limit':
        full=REVIEW/'control/p/r08_delay64.anb'
        source_text=full.read_text()
        assert source_text==(REVIEW/'verify/p'/f'{i}.anb').read_text()
        source_note='Review embeds pseudocode; recovered full original control/p/r08_delay64.anb is byte-identical to verifier source.'
        source_matches[i]=['control/p/r08_delay64.anb','verify/p/'+i+'.anb']
    src.write_text(source_text)
    v=verdicts.get(i);r=results.get(i)
    semantics={'leak':'REJECT this unauthorized explicit or implicit flow in IFC v2 and the full Safe-mode checker; runtime twin behavior must be compared only in approved isolation.', 'overrefusal':'ACCEPT the valid program in IFC v2; assess the full-check union separately because existing lanes may still refuse it.', 'robustness':'Finish realistic supported analysis within its resource envelope; a bounded named refusal is safety evidence but does not by itself close the reported precision/performance defect.'}[f['kind']]
    recheck='NOT RECHECKED. Pin the current source/binary; run IFC-only and full check and preserve immediate statuses and diagnostics. '
    if f['kind']=='robustness':recheck+='Run only inside the mandated disposable guest with caps, instrument validation, peak-resource evidence, and the original input; distinguish controlled limit from successful analysis.'
    elif any(x in i for x in ('trap','arity')):recheck+='After runtime redaction, IFC acceptance can be legitimate only when guest replay proves operand values/counts absent from every trap path and direct secret egress remains refused; test all native/guest runtime siblings.'
    elif f['kind']=='leak':recheck+='Require a direct rejection control and both secret twins in the guest; check the reported sibling mechanisms before declaring the family fixed.'
    else:recheck+='Compare identical non-empty guest outputs and check intended D1 secret-PC rejection controls so precision is not gained by hiding a real flow.'
    row={'id':i,'lens':f['lens'],'category':f['kind'],'source':'src/'+src.name,'source_sha256':digest(src.read_bytes()),'source_note':source_note,'original_review_pointer':'original/review-ifc2-r2-raw.txt#/result/findings[id='+i+']','original_program_members':source_matches[i],'original_finding':f,'expected_semantics':semantics,'reported_runtime_semantics':f['explanation'],'verifier_inputs':verifier_inputs[i],'independent_verdict':v,'independent_result':r,'independent_evidence_pointer':'verifier/results.json#/'+i,'hazards':hazards(f),'current_recheck_needed':recheck,'current_status':'unrechecked'}
    rows.append(row)
(BASE/'inventory.json').write_text(json.dumps(rows,indent=2)+'\n')
with (BASE/'inventory.tsv').open('w') as out:
    w=csv.writer(out,delimiter='\t',lineterminator='\n');w.writerow(['id','lens','category','source','original_evidence','verifier_evidence','historical_confirmed','historical_ifc_rc','historical_lanes_rc','expected_semantics','hazards','current_recheck_needed'])
    for r in rows:w.writerow([r['id'],r['lens'],r['category'],r['source'],r['original_review_pointer'],r['independent_evidence_pointer'],r['independent_verdict']['confirmed'],r['independent_result']['new_rc'],r['independent_result']['lanes_rc'],r['expected_semantics'],' '.join(r['hazards']),r['current_recheck_needed']])
(BASE/'source-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(BASE/'archive-manifest.json').write_text(json.dumps({'root':str(REVIEW),'entries':archive_entries,'exclusions':excluded},indent=2)+'\n')
summary={'findings':len(rows),'categories':dict(collections.Counter(r['category'] for r in rows)),'lenses':dict(collections.Counter(r['lens'] for r in rows)),'historical_confirmed':sum(r['independent_verdict']['confirmed'] for r in rows),'missing_verifier_results':[r['id'] for r in rows if not r['independent_result']],'missing_original_program_matches':[r['id'] for r in rows if not r['original_program_members']],'archive_files':len(archive_entries),'archive_bytes':len(archive),'partial_cached_findings':len(json.loads((CACHE/'ifc2/review2/found.json').read_text())),'no_programs_executed':True}
(BASE/'recovery-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
