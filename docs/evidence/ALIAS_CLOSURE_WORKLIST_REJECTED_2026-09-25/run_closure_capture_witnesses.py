import hashlib,json,subprocess,time
from pathlib import Path
root=Path('/home/sicarii/.cache/anubis-item21/codex-continuation')
out=root/'closure-capture-runtime'
out.mkdir(exist_ok=False)
pin=Path('/home/sicarii/.cache/anubis-item21/pins/anubis-codex-closure-cycle1')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
binary_sha=sha(pin)
rows=[]
for source in sorted((root/'closure-capture-controls-input').glob('*.anb')):
 for secret in ['42','123456']:
  run_dir=out/(source.stem+'-'+secret)
  run_dir.mkdir()
  case=run_dir/source.name
  text=source.read_text()
  assert text.count('42')==1
  case.write_text(text.replace('42',secret))
  command=[str(pin),'run',str(case),'--out',str(run_dir/'native'),'--json']
  log=run_dir/'run.log'
  start=time.monotonic()
  with log.open('wb') as stream:
   try:
    rc=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,timeout=120).returncode
   except subprocess.TimeoutExpired:
    rc='timeout'
  row={'case':source.name,'secret':secret,'command':command,'rc':rc,'elapsed_seconds':time.monotonic()-start,'source_sha256':sha(case),'log_sha256':sha(log)}
  rows.append(row)
  (out/'results.json').write_text(json.dumps({'pin':str(pin),'pin_sha256':binary_sha,'runs':rows},indent=2)+'\n')
  print(json.dumps(row),flush=True)
  if rc!=0: raise SystemExit('runtime witness failed; no retry')
assert sha(pin)==binary_sha
print('runtime witness commands completed; inspect outputs',flush=True)
