from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import time

ROOT=Path('/Users/int/.assist-session-alias-20261020')
CACHE=Path(__file__).parent
RECORD=CACHE/'session-alias-windows-cross-epoch1.json'
assert not RECORD.exists()
paths=sorted(set(subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).splitlines()+subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=ROOT,text=True).splitlines()))
def snapshot():return [{'path':name,'bytes':(ROOT/name).stat().st_size,'sha256':hashlib.sha256((ROOT/name).read_bytes()).hexdigest()}for name in paths]
state={'state':'running','inputs-before':snapshot(),'steps':[],'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
RECORD.write_text(json.dumps(state,indent=2)+'\n')
for arch in ['aarch64','x86_64']:
 argv=['cargo','+1.95.0','check','--locked','--offline','--no-default-features','--all-targets','--target',arch+'-pc-windows-msvc'];env=dict(os.environ);env.update(CARGO_TARGET_DIR=str(ROOT/'target/windows-cross'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0')
 log=CACHE/('session-alias-windows-cross-epoch1-'+arch+'.log');start=time.monotonic()
 with log.open('wb')as out:code=subprocess.run(argv,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT).returncode
 raw=log.read_bytes();state['steps'].append({'arch':arch,'argv':argv,'exit-code':code,'seconds':time.monotonic()-start,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});state['inputs-after']=snapshot();state['inputs-unchanged']=state['inputs-before']==state['inputs-after'];state.update({'state':'failed'if code else'running','exit-code':code});RECORD.write_text(json.dumps(state,indent=2)+'\n');print(arch,code,flush=True)
 if code:raise SystemExit(code)
state.update({'state':'finished','exit-code':0,'scope':'macOS-hosted Windows MSVC all-target type/build check, including Windows-only native acquisition regression. No native kernel execution is claimed.'});RECORD.write_text(json.dumps(state,indent=2)+'\n')
