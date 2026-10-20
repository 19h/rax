from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time

ROOT=Path('/Users/int/.assist-session-alias-20261020')
CACHE=Path(__file__).parent
EPOCH=int(sys.argv[1])
RECORD=CACHE/('session-alias-focus-epoch'+str(EPOCH)+'.json')
assert not RECORD.exists()
paths=sorted(set(subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).splitlines()+subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=ROOT,text=True).splitlines()))
def snapshot():
 return [{'path':name,'bytes':(ROOT/name).stat().st_size,'sha256':hashlib.sha256((ROOT/name).read_bytes()).hexdigest()}for name in paths]
before=snapshot();argv=['cargo','+1.95.0','test','--locked','--offline','--no-default-features','--lib','session_directory_alias','--','--nocapture','--test-threads=1'];env=dict(os.environ);env.update(CARGO_TARGET_DIR=str(ROOT/'target'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0')
state={'state':'running','argv':argv,'inputs-before':before,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};RECORD.write_text(json.dumps(state,indent=2)+'\n');log=CACHE/('session-alias-focus-epoch'+str(EPOCH)+'.log');start=time.monotonic()
with log.open('wb')as out:code=subprocess.run(argv,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT).returncode
raw=log.read_bytes();state.update({'state':'finished','exit-code':code,'seconds':time.monotonic()-start,'inputs-after':snapshot(),'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',raw.decode(errors='replace'))});state['inputs-unchanged']=state['inputs-before']==state['inputs-after'];RECORD.write_text(json.dumps(state,indent=2)+'\n');print(json.dumps({k:v for k,v in state.items()if not k.startswith('inputs-')},indent=2));assert code==0 and state['inputs-unchanged']
