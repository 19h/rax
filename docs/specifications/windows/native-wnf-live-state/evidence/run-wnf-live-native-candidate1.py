"""Frozen test-only WNF repair: preserve parent control, compile and run native tests."""
from pathlib import Path
import datetime, hashlib, json, os, re, shutil, subprocess, tarfile, time, traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R=B/'src-display-candidate1'
F=S/'wnf-live-native-candidate1.json'
assert not F.exists()
state={'state':'preparing','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[]}
def save(): F.write_text(json.dumps(state,indent=2)+'\n')
def identity(p):
 h=hashlib.sha256();n=0
 with p.open('rb') as f:
  for block in iter(lambda:f.read(1<<20),b''):h.update(block);n+=len(block)
 return {'path':str(p),'bytes':n,'sha256':h.hexdigest()}
def pair(row):return row['bytes'],row['sha256']
def verify():
 for row in meta['files']:
  expected=pair(row)
  if row['path']=='.cargo/config.toml':expected=(374,'9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c')
  assert pair(identity(R/row['path']))==expected,row['path']
def run(name,args):
 log=S/('wnf-live-native-candidate1-'+name+'.log');assert not log.exists();start=time.monotonic()
 with log.open('wb') as out:
  p=subprocess.Popen(args,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT);state.update(state='running',step=name,child=p.pid);save();code=p.wait()
 text=log.read_text(errors='replace');state['steps'].append({'name':name,'argv':args,'exit-code':code,'seconds':time.monotonic()-start,'log':log.name,**{k:v for k,v in identity(log).items() if k!='path'},'results':re.findall(r'test result: ([^\r\n]+)',text)});save();assert code==0,name
 return text
save()
try:
 manifest=S/'wnf-live-candidate1-source.json';assert identity(manifest)['sha256']=='b3972f8e469afcb552050835030e461319293433b3cf41abe555cf99c828d38f';meta=json.loads(manifest.read_bytes())
 prior=json.loads((S/'process-basic-candidate1-windows-resume1.json').read_bytes());image=Path(prior['test-artifact']['executable']);assert identity(image)==prior['test-image-before']
 retained=R/'target/wnf-live-parent-control-v1.exe';assert not retained.exists();shutil.copy2(image,retained);assert pair(identity(retained))==pair(prior['test-image-before']);state['retained-parent-image']=identity(retained)
 old={r['path']:r for r in json.loads((S/'process-basic-candidate1-source.json').read_bytes())['files']}
 for row in meta['overrides']:assert pair(identity(R/row['path']))==pair(old[row['path']]),row['path']
 archive=S/meta['archive'];assert pair(identity(archive))==pair(meta)
 temp=R/'target/wnf-live-overlay-v1';assert not temp.exists();temp.mkdir()
 with tarfile.open(archive,'r:gz') as bundle:
  members=bundle.getmembers();assert {m.name for m in members}=={r['path'] for r in meta['overrides']}
  for i,m in enumerate(members):
   assert m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts
   raw=bundle.extractfile(m).read();row=next(r for r in meta['overrides'] if r['path']==m.name);assert (len(raw),hashlib.sha256(raw).hexdigest())==pair(row)
   temporary=temp/str(i);temporary.write_bytes(raw);os.replace(temporary,R/m.name)
 temp.rmdir();verify();state['source-inputs']=len(meta['files']);state['manifest']=identity(manifest);state['disk-before']=shutil.disk_usage(R).free
 env=dict(os.environ)
 for key in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CCACHE_DIR','SCCACHE_DIR']:env.pop(key,None)
 env.update(json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
 env.update(CARGO_HOME=str(B/'cargo-home'),CARGO_TARGET_DIR=str(R/'target/normal-candidate1'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',RUSTC=str(B/'toolchain/bin/rustc.exe'),TMP=str(R/'target/tmp'),TEMP=str(R/'target/tmp'),TMPDIR=str(R/'target/tmp'),CCACHE_DISABLE='1',SCCACHE_DISABLE='1')
 cargo=str(B/'toolchain/bin/cargo.exe')
 text=run('focus',[cargo,'test','--locked','--offline','--no-default-features','--lib','user::windows::wnf::','--','--nocapture','--test-threads=1'])
 assert 'wnf-native-system attempt=' in text and 'oracle=Data' in text
 assert any('0 failed; 0 ignored;' in value and not value.startswith('ok. 0 passed;') for value in state['steps'][-1]['results'])
 verify()
 run('all-targets',[cargo,'check','--locked','--offline','--no-default-features','--all-targets']);verify()
 # The pure CI-runner tests preserve native-command failure propagation.
 run('ci-runner',[str(B/'python/python.exe'),'-B','-m','unittest','discover','-s','tools/ci']);verify()
 state.update(state='finished',**{'exit-code':0,'retained-parent-image-after':identity(retained),'scope':'Test/CI coverage only; no native acquisition, guest model, ABI, permission or package change.'});assert state['retained-parent-image']==state['retained-parent-image-after']
except BaseException:state.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps({k:v for k,v in state.items() if k!='steps'}))
raise SystemExit(state['exit-code'])
