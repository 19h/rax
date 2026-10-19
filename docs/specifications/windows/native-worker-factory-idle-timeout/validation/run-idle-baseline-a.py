from pathlib import Path
import datetime,hashlib,json,os,re,subprocess
q=Path(__file__).resolve().parent;r=q/'idle-source'
owned=json.loads((q/'idle-ownership.json').read_text())['source-owned-paths']
inputs=[{'path':p,'sha256':hashlib.sha256((r/p).read_bytes()).hexdigest(),'bytes':(r/p).stat().st_size}for p in owned if p.endswith('.rs') and (r/p).exists()]
args=['cargo','+1.95.0','test','--locked','--offline','--no-default-features','--lib','native_worker_factory_idle','--','--nocapture','--test-threads=1']
env=dict(os.environ,CARGO_TARGET_DIR=str(q.parent/'native-wait-20261019/target-macos-wait-epoch3'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0')
record=q/'idle-baseline-a.json';assert not record.exists()
state={'state':'running','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'inputs':inputs,'argv':args}
record.write_text(json.dumps(state,indent=2)+'\n')
p=q/'idle-baseline-a.log'
with p.open('xb')as log:code=subprocess.run(args,cwd=r,env=env,stdout=log,stderr=subprocess.STDOUT).returncode
raw=p.read_bytes();state.update({'state':'finished'if code==0 else'failed','exit-code':code,'log':p.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',raw.decode(errors='replace'))})
for row in inputs:assert hashlib.sha256((r/row['path']).read_bytes()).hexdigest()==row['sha256']
record.write_text(json.dumps(state,indent=2)+'\n');print(json.dumps(state,indent=2));raise SystemExit(code)
