"""Materialize a separate immutable source root, then validate on native Windows."""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import datetime,hashlib,json,os,re,shutil,subprocess,tarfile,traceback,time
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');OLD=B/'src-file-open-epoch1';R=B/'src-file-open-epoch2';RECORD=S/'file-open-native-epoch2-gates.json';LOCK=B/'file-open-native-epoch2.lock'
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
  got=identity(R/x['path'])
  if x['path']=='.cargo/config.toml':assert x['bytes']==552 and x['sha256']=='a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd'
  expected=CONFIG if x['path']=='.cargo/config.toml'else x['sha256']
  assert got['sha256']==expected,x['path']
  assert x['path']=='.cargo/config.toml'or got['bytes']==x['bytes'],x['path']
 with ThreadPoolExecutor(max_workers=4)as workers:list(workers.map(check,manifest['files']))
def run(name,argv,env):
 log=S/('file-open-native-epoch2-'+name+'.log');started=time.monotonic()
 with log.open('wb')as out:
  child=subprocess.Popen(argv,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'running','current-step':name,'child-pid':child.pid});save();code=child.wait()
 raw=log.read_bytes();output=raw.decode('utf8','replace');state['steps'].append({'name':name,'argv':argv,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',output)});save();return code,output
save()
try:
 prior=json.loads((S/'file-open-native-epoch1-gates.json').read_text());assert prior['state']=='finished'and prior['exit-code']==0
 baseline_unit=json.loads((S/'file-open-delete-baseline-unit-v2.json').read_text());assert baseline_unit['state']=='finished'and baseline_unit['exit-code']==0
 assert identity(S/'file-open-source-epoch2.json')['sha256']=='f257d4a412aeed642d4351ca6e7d1dea89ec95173a09eb430e5036651ac625a5'
 manifest=json.loads((S/'file-open-source-epoch2.json').read_text());meta=json.loads((S/'file-open-source-epoch2-archive.json').read_text());assert hashlib.sha256((S/meta['parent-manifest']).read_bytes()).hexdigest()==meta['parent-manifest-sha256']
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
 normal=OLD/'target/normal';assert normal.is_dir()
 powershell=['powershell.exe','-NoProfile','-NonInteractive','-Command',"@(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(cargo|rustc|rust-lld|link)\\.exe$' -and $_.CommandLine -like '*src-file-open-epoch1*target*normal*' } | Select-Object ProcessId,Name,CommandLine) | ConvertTo-Json -Compress"]
 live=subprocess.run(powershell,capture_output=True,text=True,check=True).stdout.strip();assert live in ['', '[]'],live;state['cache-live-references']=[]
 baseline_log=(S/'file-open-delete-baseline-unit-v2.log').read_text(errors='replace')
 match=re.search(r'Running unittests [^\r\n]* \(([^)]+\.exe)\)',baseline_log);assert match
 baseline_image=Path(match.group(1));retained=B/'fo-db2/target/retained-baseline-v2';retained.mkdir(parents=True,exist_ok=False);copy=retained/'unit.exe';shutil.copy2(baseline_image,copy);a=identity(baseline_image);b=identity(copy);assert a['bytes']==b['bytes']and a['sha256']==b['sha256'];state['retained-failing-test']={'source':a,'retained':b};save()
 normal.rename(R/'target/normal')
 env=dict(os.environ)
 for key in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(key,None)
 env.update(json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
 env.update({'CARGO_HOME':str(B/'cargo-home'),'CARGO_TARGET_DIR':str(R/'target/normal'),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','RUSTC':str(B/'toolchain/bin/rustc.exe'),'TMP':str(R/'target/tmp'),'TEMP':str(R/'target/tmp')})
 cargo=str(B/'toolchain/bin/cargo.exe');code,output=run('toolchain',[cargo,'--version'],env);assert code==0 and'cargo 1.95.0'in output
 baseline=set(json.loads((S/'mutant-native-baseline-failures.json').read_text())['failures'])
 gates=[('focus',['test','--lib','native_file_']),('full',['test','--lib']),('capi',['test','-p','rax-capi']),('integration',['test','--test','user_windows','--test','user_windows_memory']),('all-targets',['check','--all-targets'])]
 for name,args in gates:
  argv=[cargo,args[0],'--locked','--offline','--no-default-features',*args[1:]]
  if args[0]=='test':argv+=['--','--nocapture','--test-threads=1']
  code,output=run(name,argv,env);verify()
  if name=='full':
   final=output[output.rfind('\nfailures:\n'):];failures=set(re.findall(r'^    (\S+)\r?$',final,re.M));assert code==101 and failures==baseline
   assert re.search(r'[0-9]+ passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;',output)
   assert '7590 passed; 5 failed; 2 ignored;'in output
   state['exact-existing-failures']=sorted(failures)
  else:assert code==0,name
  if name=='focus':
   assert '26 passed; 0 failed;' in output
   assert 'native_file_attributes_installed_leaves_match_native_metadata_and_ret8 ... ok' in output
  if name=='capi':assert '183 passed; 0 failed;' in output
  if name=='integration':assert '4 passed; 0 failed;' in output
  save()
 state.update({'state':'finished','exit-code':0,'post-source-inputs':len(manifest['files']),'native-cargo-configuration':{'bytes':374,'sha256':CONFIG},'scope':'Frozen file-open source and actual installed native/WoW64 NT leaf execution, complete native Rust/C API/integration/build gates; exactly five established lowerer failures remain explicit. Native Cargo configuration is bound separately.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();LOCK.unlink()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
