from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,time
Q=Path(__file__).parent;R=Q/'registry-source'
record=Q/'registry-macos-prototype.json';assert not record.exists()
paths=json.loads((Q/'registry-source-ownership.json').read_text())['source-owned-paths']
inputs=[{'path':p,'sha256':hashlib.sha256((R/p).read_bytes()).hexdigest()} for p in paths if (R/p).is_file()]
state={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','inputs-before':inputs,'steps':[]}
env=dict(os.environ,CARGO_TARGET_DIR=str(Q.parent/'native-wait-20261019/target-macos-wait-epoch3'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0')
for name,selector in [('new','redirection'),('existing-ifeo','ifeo')]:
 args=['cargo','+1.95.0','test','--locked','--offline','--no-default-features','--lib',selector,'--','--nocapture','--test-threads=1']
 log=Q/('registry-macos-prototype-'+name+'.log');start=time.monotonic()
 with log.open('wb') as f:
  p=subprocess.Popen(args,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT);state['child-pid']=p.pid;record.write_text(json.dumps(state,indent=2)+'\n');code=p.wait()
 raw=log.read_bytes();output=raw.decode('utf-8','replace')
 state['steps'].append({'name':name,'argv':args,'exit-code':code,'seconds':time.monotonic()-start,'log':log.name,'sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',output)})
 record.write_text(json.dumps(state,indent=2)+'\n')
 assert code==0,(name,code)
 if name=='new':assert '6 passed; 0 failed;' in output
state['inputs-after']=[{'path':p,'sha256':hashlib.sha256((R/p).read_bytes()).hexdigest()} for p in paths if (R/p).is_file()]
assert state['inputs-before']==state['inputs-after']
state.update(state='finished',**{'exit-code':0,'completed-utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(state,indent=2)+'\n');print(json.dumps(state),flush=True)
