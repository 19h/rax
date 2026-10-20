from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-file-attributes-epoch3';ROOT=B/'assist';RECORD=S/'file-attributes-native-owning-epoch3.json';assert not RECORD.exists();state={'state':'waiting-for-focus','pid':os.getpid(),'source-epoch':3,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[]}
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def verify():
 for row in manifest['sources']:
  raw=(R/row['path']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 for row in manifest['helpers']:
  raw=Path(row['native-path']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 assert(ROOT/'vendor/rax').resolve()==R.resolve()
def run(name,args,env):
 log=S/('file-attributes-native-owning-epoch3-'+name+'.log');started=time.monotonic()
 with log.open('wb')as out:
  child=subprocess.Popen(args,cwd=ROOT/'rust/assist-rs',env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'running','current-step':name,'child-pid':child.pid});save();code=child.wait()
 raw=log.read_bytes();state['steps'].append({'name':name,'argv':args,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});save();assert code==0,name
 return log
save()
try:
 deadline=time.monotonic()+7200
 while True:
  p=S/'file-attributes-native-epoch3-gates.json'
  if p.exists():
   d=load(p);assert d['state']!='failed',d.get('error','gate failed')
   if any(x['name']=='focus'and x['exit-code']==0 for x in d['steps']):break
  assert time.monotonic()<deadline;time.sleep(3)
 link=ROOT/'vendor/rax';assert link.is_junction();before=str(link.resolve());assert link.resolve()in[(B/'src-file-attributes-epoch2').resolve(),R.resolve()]
 if link.resolve()!=R.resolve():
  old=os.readlink(link);os.rmdir(link)
  try:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),str(R)],check=True)
  except BaseException:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),old],check=True);raise
 state['junction']={'before':before,'after':str(link.resolve())};manifest=load(S/'file-attributes-owning-source-hashes-epoch3.json');target=ROOT/'target/native-attributes';shutil.copy2(S/'file-attributes-owned-cpp-build-epoch3.py',target/'helpers/file-attributes-owned-cpp-build-epoch3.py');verify()
 cpp=load(S/'current-cpp-consumers-wait-root.json')
 for row in cpp['files']:
  raw=(ROOT/row['path']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 env=dict(os.environ);env.pop('CARGO_ENCODED_RUSTFLAGS',None);env.update(load(S/'mutant-native-compiler-env-epoch4.json'));env.update({'TMP':str(target/'tmp'),'TEMP':str(target/'tmp'),'PATH':str(B/'llvm/bin')+';'+env['PATH'],'RUSTC':str(B/'toolchain/bin/rustc.exe'),'CARGO_HOME':str(B/'cargo-home'),'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','RUSTFLAGS':'-C target-feature=+crt-static','ASSIST_RESOURCE_PYTHON':str(B/'python/python.exe')})
 cargo=str(B/'toolchain/bin/cargo.exe');python=str(B/'python/python.exe');run('toolchain',[cargo,'--version'],env);command=[cargo,'build','--locked','--offline','--features','rax','--message-format=json'];messages=run('artifacts-before',command,env);verify()
 before=S/'native-file-attributes-epoch3-pre-cpp-archives.json';after=S/'native-file-attributes-epoch3-final-owning-archive-identities.json';assert not before.exists()and not after.exists();collector=[python,'-B',str(S/'collect-file-attributes-archives-epoch3.py'),'--host','windows','--source',str(R),'--manifest',str(S/'file-attributes-owning-source-hashes-epoch3.json')]
 run('archive-before',[*collector,'--messages',str(messages),'--output',str(before)],env);run('cpp',[python,'-B',str(target/'helpers/file-attributes-owned-cpp-build-epoch3.py')],env);verify();messages=run('artifacts-after',command,env);run('archive-after',[*collector,'--messages',str(messages),'--output',str(after)],env);verify();assert load(before)['archives']==load(after)['archives'];state.update({'state':'finished','exit-code':0,'source-inputs':len(manifest['sources']),'archive-identities-stable':True,'scope':'Frozen read-only file-attribute source; retained Assist ABI184 linked consumers. Whole current-root ABI273 GUI/plugin/packages not claimed.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
