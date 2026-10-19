"""Materialize a separate immutable source root, then validate on native Windows."""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import datetime,hashlib,json,os,re,shutil,subprocess,tarfile,traceback,time
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');OLD=B/'src-alert-epoch2';R=B/'src-alert-epoch3';RECORD=S/'alert-native-epoch3-gates.json';LOCK=B/'alert-native-epoch3.lock'
CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
assert not RECORD.exists() and not R.exists()
fd=os.open(LOCK,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
state={'state':'preparing','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 h=hashlib.sha256();n=0
 with path.open('rb')as f:
  for chunk in iter(lambda:f.read(1048576),b''):h.update(chunk);n+=len(chunk)
 return {'path':str(path),'bytes':n,'sha256':h.hexdigest()}
def verify():
 def check(x):
  got=identity(R/x['path']);expected=CONFIG if x['path']=='.cargo/config.toml'else x['sha256']
  assert got['sha256']==expected,x['path']
  assert x['path']=='.cargo/config.toml'or got['bytes']==x['bytes'],x['path']
 with ThreadPoolExecutor(max_workers=4)as workers:list(workers.map(check,manifest['files']))
def run(name,argv,env):
 log=S/('alert-native-epoch3-'+name+'.log');started=time.monotonic()
 with log.open('wb')as out:
  child=subprocess.Popen(argv,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'running','current-step':name,'child-pid':child.pid});save();code=child.wait()
 raw=log.read_bytes();output=raw.decode('utf8','replace');state['steps'].append({'name':name,'argv':argv,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',output)});save();return code,output
save()
try:
 prior=json.loads((S/'alert-native-epoch2-gates.json').read_text());assert prior['state']=='failed'and prior['steps'][-1]['name']=='focus'and prior['steps'][-1]['exit-code']==101
 manifest=json.loads((S/'alert-source-epoch3.json').read_text());meta=json.loads((S/'alert-source-epoch3-archive.json').read_text());assert hashlib.sha256((S/meta['parent-manifest']).read_bytes()).hexdigest()==meta['parent-manifest-sha256']
 changed=set(meta['separate-overrides']);R.mkdir()
 for row in manifest['files']:
  if row['path']in changed:continue
  assert row['kind']in ['file','symlink-text'],row['path']
  source,target=OLD/row['path'],R/row['path'];target.parent.mkdir(parents=True,exist_ok=True)
  try:os.link(source,target)
  except OSError:shutil.copy2(source,target)
 archive=S/meta['archive'];assert identity(archive)['sha256']==meta['sha256']
 with tarfile.open(archive,'r:gz')as tar:
  members=tar.getmembers();assert len(members)==len(changed)and{m.name for m in members}==changed
  assert all(m.isfile()and not Path(m.name).is_absolute()and'..'not in Path(m.name).parts for m in members)
  tar.extractall(R,filter='data')
 verify();state['verified-source-inputs']=len(manifest['files']);state['verified-source-bytes']=manifest['total-bytes'];save()
 (R/'target/tmp').mkdir(parents=True)
 normal=OLD/'target/normal';assert normal.is_dir();normal.rename(R/'target/normal')
 env=dict(os.environ)
 for key in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(key,None)
 env.update(json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
 env.update({'CARGO_HOME':str(B/'cargo-home'),'CARGO_TARGET_DIR':str(R/'target/normal'),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','RUSTC':str(B/'toolchain/bin/rustc.exe'),'TMP':str(R/'target/tmp'),'TEMP':str(R/'target/tmp')})
 cargo=str(B/'toolchain/bin/cargo.exe');code,output=run('toolchain',[cargo,'--version'],env);assert code==0 and'cargo 1.95.0'in output
 baseline=set(json.loads((S/'mutant-native-baseline-failures.json').read_text())['failures'])
 gates=[('focus',['test','--lib','native_test_alert']),('full',['test','--lib']),('capi',['test','-p','rax-capi']),('integration',['test','--test','user_windows','--test','user_windows_memory']),('all-targets',['check','--all-targets'])]
 for name,args in gates:
  argv=[cargo,args[0],'--locked','--offline','--no-default-features',*args[1:]]
  if args[0]=='test':argv+=['--','--nocapture','--test-threads=1']
  code,output=run(name,argv,env);verify()
  if name=='full':
   final=output[output.rfind('\nfailures:\n'):];failures=set(re.findall(r'^    (\S+)\r?$',final,re.M));assert code==101 and failures==baseline
   assert '7521 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;'in output
   state['exact-existing-failures']=sorted(failures)
  else:assert code==0,name
  if name=='focus':assert'9 passed; 0 failed;'in output
  save()
 state.update({'state':'finished','exit-code':0,'post-source-inputs':len(manifest['files']),'native-cargo-configuration':{'bytes':374,'sha256':CONFIG},'scope':'Four ABI native observations; native host/WoW64 installed leaves and full Rust core with exactly five prior lowerer failures. No successful ordinary guest exit claim.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();LOCK.unlink()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
