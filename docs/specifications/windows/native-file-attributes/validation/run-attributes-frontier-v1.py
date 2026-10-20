"""Decode the next guest syscall input against the retained observed core."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import time
import traceback

BASE=Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD=SHARE/'attributes-frontier-v1.json'
assert not RECORD.exists()
TARGET=BASE/'attributes-frontier-v1'
TARGET.mkdir(exist_ok=True)
state={'state':'preparing','pid':os.getpid(),'steps':[],'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save(): RECORD.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 digest=hashlib.sha256();size=0
 with path.open('rb')as stream:
  for chunk in iter(lambda:stream.read(1<<20),b''):digest.update(chunk);size+=len(chunk)
 return {'path':str(path),'bytes':size,'sha256':digest.hexdigest()}
def run(name,argv,env):
 log=SHARE/('attributes-frontier-v1-'+name+'.log');started=time.monotonic()
 with log.open('wb')as out:
  child=subprocess.Popen(argv,cwd=TARGET,env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'running','step':name,'child-pid':child.pid});save();code=child.wait()
 row={'name':name,'argv':argv,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,**identity(log)};state['steps'].append(row);save();assert code==0,log.read_text(errors='replace')
 return log
save()
try:
 retained=json.loads((SHARE/'region-retained-owning-epoch1.json').read_text());row=next(row for row in retained['files']if row['role']=='core');core=Path(row['retained']);assert identity(core)['sha256']==row['sha256']
 # Dependency search remains the bound original directory; the linked core is the retained byte copy.
 deps=Path(row['source']).parent
 source=SHARE/'attributes-frontier-v1.rs';meta=json.loads((SHARE/'attributes-frontier-v1-source.json').read_text());assert identity(source)['sha256']==meta['sha256']
 program=Path('C:/Windows/System32/whoami.exe');modules=[Path(row['path'])for row in json.loads((SHARE/'region-native-ordinary-epoch1.json').read_text())['runtime-after']]
 inputs=[source,core,program,*modules];state['inputs-before']=[identity(path)for path in inputs];save()
 env=dict(os.environ);env.pop('CARGO_ENCODED_RUSTFLAGS',None);env.update(json.loads((SHARE/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
 executable=TARGET/'attributes-frontier-v1.exe'
 run('build',[str(BASE/'toolchain/bin/rustc.exe'),'--edition=2024','--crate-name','attributes_frontier_v1',str(source),'-C','target-feature=+crt-static','--extern','rax='+str(core),'-L','dependency='+str(deps),'-o',str(executable)],env)
 state['executable-before']=identity(executable)
 control=subprocess.run([str(program)],capture_output=True,timeout=30);state['native-before']={'exit-code':control.returncode,'stdout-hex':control.stdout.hex(),'stderr-hex':control.stderr.hex()}
 output=run('guest',[str(executable),str(program)],env).read_text();assert 'NtQueryAttributesFile (0x3d)'in output and 'object-name units='in output
 control=subprocess.run([str(program)],capture_output=True,timeout=30);state['native-after']={'exit-code':control.returncode,'stdout-hex':control.stdout.hex(),'stderr-hex':control.stderr.hex()}
 state['executable-after']=identity(executable);state['inputs-after']=[identity(path)for path in inputs];assert state['inputs-before']==state['inputs-after']and state['executable-before']==state['executable-after']and state['native-before']==state['native-after']
 state.update({'state':'finished','exit-code':0,'scope':'Exact existingC:\\app\\probe.exe embeddedimage configuration and native-runtime libraries; bounded4096instruction observation against retainedMRIowningcore; inputdiscovery only, not newserviceimplementation.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
