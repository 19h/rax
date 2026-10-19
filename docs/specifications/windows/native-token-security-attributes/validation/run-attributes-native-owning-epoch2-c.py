from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,time,traceback

B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-attributes-candidate-epoch2';ROOT=B/'assist'
RECORD=S/'attributes-native-owning-epoch2-c.json';LOCK=B/'attributes-native-owning-epoch2.lock'
assert not RECORD.exists(),'epoch4 native owning already attempted'
fd=os.open(LOCK,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
state={'pid':os.getpid(),'state':'waiting-for-focus','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':2,'steps':[]}
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
 path=S/('attributes-native-owning-epoch2-c-'+name+'.log');started=datetime.datetime.now(datetime.timezone.utc).isoformat()
 with path.open('wb')as log:
  child=subprocess.Popen(args,cwd=ROOT/'rust/assist-rs',env=env,stdout=log,stderr=subprocess.STDOUT);state.update({'current-step':name,'child-pid':child.pid});save();code=child.wait()
 raw=path.read_bytes();state['steps'].append({'name':name,'argv':args,'exit-code':code,'started-utc':started,'log':path.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});save();assert code==0,name
 return path
save()
try:
 deadline=time.monotonic()+1800
 while True:
  path=S/'attributes-native-epoch2-gates.json'
  if not path.exists():
   assert time.monotonic()<deadline,'native broad gate not started before deadline'
   time.sleep(3)
   continue
  status=load(path)
  assert status['state']!='failed',status.get('error','native integration gate failed')
  if any(row['name']=='focus' and row['exit-code']==0 for row in status['steps']):break
  assert time.monotonic()<deadline,'native focus validation timed out';time.sleep(3)
 candidate=load(S/'attributes-candidate-epoch2-run-b.json')
 assert candidate['state']=='finished' and candidate['exit-code']==0
 manifest=load(S/'attributes-owning-source-hashes-epoch2.json')
 link=ROOT/'vendor/rax';old=B/'src-idle-candidate-epoch2'
 assert link.is_junction() and link.resolve() in [old.resolve(),R.resolve()]
 before=os.readlink(link)
 if link.resolve()!=R.resolve():
  os.rmdir(link)
  try:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),str(R)],check=True)
  except BaseException:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),before],check=True);raise
 state['junction']={'before':before,'after':os.readlink(link)}
 target=ROOT/'target/native-attributes'
 assert target.is_dir() and not (R/'target/owned').exists()
 (target/'helpers').mkdir(exist_ok=True)
 shutil.copy2(S/'attributes-owned-cpp-build-epoch2.py',target/'helpers/attributes-owned-cpp-build-epoch2.py')
 dependency=ROOT/'scripts/static_text/lzfse_lzvn.py'
 assert not dependency.exists() or hashlib.sha256(dependency.read_bytes()).hexdigest()=='862f51f6c3bcd68092045827eeb024a8aef252cad649f1f207658226a3824d75'
 if not dependency.exists():shutil.copy2(S/'attributes-carrier-lzfse-lzvn.py',dependency)
 assert hashlib.sha256(dependency.read_bytes()).hexdigest()=='862f51f6c3bcd68092045827eeb024a8aef252cad649f1f207658226a3824d75'
 state['carrier-build-helper']={'path':str(dependency),'sha256':'862f51f6c3bcd68092045827eeb024a8aef252cad649f1f207658226a3824d75','bytes':3200,'scope':'Exact retained carrier helper restored after recorded missing-input import failure; no frozen RAX inputs changed.'}
 verify();state.update({'state':'building','verified-source-inputs':len(manifest['sources'])});save()
 cpp=load(S/'current-cpp-consumers-wait-root.json')
 for row in cpp['files']:
  raw=(ROOT/row['path']).read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 state['compared-cpp-inputs']=len(cpp['files'])
 env=dict(os.environ);env.pop('CARGO_ENCODED_RUSTFLAGS',None);env.update(load(S/'mutant-native-compiler-env-epoch4.json'))
 env.update({'TMP':str(ROOT/'target/native-attributes/tmp'),'TEMP':str(ROOT/'target/native-attributes/tmp')})
 (ROOT/'target/native-attributes/tmp').mkdir(exist_ok=True)
 env.update({'PATH':str(B/'llvm/bin')+';'+env['PATH'],'RUSTC':str(B/'toolchain/bin/rustc.exe'),'CARGO_HOME':str(B/'cargo-home'),'CARGO_TARGET_DIR':str(ROOT/'target/native-attributes'),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','RUSTFLAGS':'-C target-feature=+crt-static','ASSIST_RESOURCE_PYTHON':str(B/'python/python.exe')})
 cargo=str(B/'toolchain/bin/cargo.exe');python=str(B/'python/python.exe')
 run('toolchain',[cargo,'--version'],env)
 command=[cargo,'build','--locked','--offline','--features','rax','--message-format=json']
 messages=run('artifacts-before',command,env);verify()
 collector=[python,'-B',str(S/'collect-attributes-archives-epoch2.py'),'--host','windows','--source',str(R),'--manifest',str(S/'attributes-owning-source-hashes-epoch2.json')]
 run('archive-before',[*collector,'--messages',str(messages),'--output',str(S/'native-attributes-epoch2-c-pre-cpp-archives.json')],env)
 run('cpp',[python,'-B',str(ROOT/'target/native-attributes/helpers/attributes-owned-cpp-build-epoch2.py')],env);verify()
 messages=run('artifacts-after',command,env)
 run('archive-after',[*collector,'--messages',str(messages),'--output',str(S/'native-attributes-epoch2-c-final-owning-archive-identities.json')],env);verify()
 before=load(S/'native-attributes-epoch2-c-pre-cpp-archives.json');after=load(S/'native-attributes-epoch2-c-final-owning-archive-identities.json')
 assert before['archives']==after['archives'],'owning archives changed across C++ validation'
 state.update({'state':'finished','exit-code':0,'post-source-inputs':len(manifest['sources']),'qualification':'Actual current Cargo archives, selected880 inputs, matching archive identities before and after five C++ consumers. Retained Assist Rust ABI184; eight C++ inputs were compared to root ABI264; no whole current-root GUI/plugin/package claim.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();LOCK.unlink()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
