from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,time,traceback

B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-wait-epoch4';ROOT=B/'assist'
RECORD=S/'wait-native-owning-epoch4.json';LOCK=B/'wait-native-owning-epoch4.lock'
assert not RECORD.exists(),'epoch4 native owning already attempted'
fd=os.open(LOCK,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
state={'pid':os.getpid(),'state':'waiting-for-focus','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':4,'steps':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def verify():
 for row in manifest['sources']:
  raw=(R/row['path']).read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 for row in manifest['helpers']:
  raw=Path(row['native-path']).read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 assert (ROOT/'vendor/rax').resolve()==R.resolve()
 assert hashlib.sha256((R/'.cargo/config.toml').read_bytes()).hexdigest()=='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
def run(name,args,env):
 path=S/('wait-native-owning-epoch4-'+name+'.log');started=datetime.datetime.now(datetime.timezone.utc).isoformat()
 with path.open('wb')as log:
  child=subprocess.Popen(args,cwd=ROOT/'rust/assist-rs',env=env,stdout=log,stderr=subprocess.STDOUT);state.update({'current-step':name,'child-pid':child.pid});save();code=child.wait()
 raw=path.read_bytes();state['steps'].append({'name':name,'argv':args,'exit-code':code,'started-utc':started,'log':path.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});save();assert code==0,name
 return path
save()
try:
 deadline=time.monotonic()+1800
 while True:
  focus=S/'wait-native-epoch4-focus.log'
  if (S/'wait-native-epoch4-freshness.json').exists() and focus.exists() and '17 passed; 0 failed;' in focus.read_text():break
  status=load(S/'wait-native-epoch4-gates.json');assert status['state']!='failed',status.get('error','native epoch4 failed')
  assert time.monotonic()<deadline,'native epoch4 focus timed out';time.sleep(3)
 assert load(S/'wait-final-owned-b-launch.json')['state']=='finished'
 manifest=load(S/'wait-owning-source-hashes-epoch4.json')
 link=ROOT/'vendor/rax';old=B/'src-wait-epoch3'
 assert link.is_junction() and link.resolve() in [old.resolve(),R.resolve()]
 before=os.readlink(link)
 if link.resolve()!=R.resolve():
  os.rmdir(link)
  try:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),str(R)],check=True)
  except BaseException:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),before],check=True);raise
 state['junction']={'before':before,'after':os.readlink(link)}
 shutil.copy2(S/'wait-owned-cpp-build-epoch4.py',B/'wait-owned-cpp-build-epoch4.py')
 verify();state.update({'state':'building','verified-source-inputs':len(manifest['sources'])});save()
 cpp=load(S/'current-cpp-consumers-wait-root.json')
 for row in cpp['files']:
  raw=(ROOT/row['path']).read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 state['compared-cpp-inputs']=len(cpp['files'])
 env=dict(os.environ);env.pop('CARGO_ENCODED_RUSTFLAGS',None);env.update(load(S/'mutant-native-compiler-env-epoch4.json'))
 env.update({'PATH':str(B/'llvm/bin')+';'+env['PATH'],'RUSTC':str(B/'toolchain/bin/rustc.exe'),'CARGO_HOME':str(B/'cargo-home'),'CARGO_TARGET_DIR':str(B/'target-wait-owned'),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','RUSTFLAGS':'-C target-feature=+crt-static','ASSIST_RESOURCE_PYTHON':str(B/'python/python.exe')})
 cargo=str(B/'toolchain/bin/cargo.exe');python=str(B/'python/python.exe')
 run('toolchain',[cargo,'--version'],env)
 command=[cargo,'build','--locked','--offline','--features','rax','--message-format=json']
 messages=run('artifacts-before',command,env);verify()
 collector=[python,'-B',str(S/'collect-wait-archives-epoch4.py'),'--host','windows','--source',str(R),'--manifest',str(S/'wait-owning-source-hashes-epoch4.json')]
 run('archive-before',[*collector,'--messages',str(messages),'--output',str(S/'native-wait-epoch4-pre-cpp-archives.json')],env)
 run('cpp',[python,'-B',str(B/'wait-owned-cpp-build-epoch4.py')],env);verify()
 messages=run('artifacts-after',command,env)
 run('archive-after',[*collector,'--messages',str(messages),'--output',str(S/'native-wait-epoch4-final-owning-archive-identities.json')],env);verify()
 before=load(S/'native-wait-epoch4-pre-cpp-archives.json');after=load(S/'native-wait-epoch4-final-owning-archive-identities.json')
 assert before['archives']==after['archives'],'owning archives changed across C++ validation'
 state.update({'state':'finished','exit-code':0,'post-source-inputs':len(manifest['sources']),'qualification':'Actual current Cargo archives, selected474 inputs, matching archive identities before and after five C++ consumers. Retained Assist Rust ABI184; compared root ABI264 is separate.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();LOCK.unlink()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
