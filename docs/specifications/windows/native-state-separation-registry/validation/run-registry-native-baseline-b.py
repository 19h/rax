from pathlib import Path
import datetime,hashlib,json,os,re,shutil,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
P=B/'src-semaphore-epoch2'
R=B/'src-registry-baseline'
RECORD=S/'registry-native-baseline-b.json'
CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
assert not RECORD.exists() and R.exists()
state={'state':'preparing','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[],'scope':'New native presence regression with product registry implementation unchanged from semaphore parent.'}
def save(): RECORD.write_text(json.dumps(state,indent=2)+'\n')
save()
try:
 overrides=json.loads((S/'registry-baseline-overrides.json').read_text())
 for row in overrides:
  assert hashlib.sha256((R/row['path']).read_bytes()).hexdigest()==row['sha256']
 state['overrides']=overrides;save()
 env=dict(os.environ)
 for k in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(k,None)
 env.update(CARGO_HOME=str(B/'cargo-home'),CARGO_TARGET_DIR=str(B/'target-wait-epoch3'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',RUSTC=str(B/'toolchain/bin/rustc.exe'))
 env.update(json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
 args=[str(B/'toolchain/bin/cargo.exe'),'test','--locked','--offline','--no-default-features','--lib','installed_redirection_keys_snapshot_matches_independent_native_presence_all_views','--','--nocapture','--test-threads=1']
 log=S/'registry-native-baseline-b.log'
 with log.open('wb') as f:
  child=subprocess.Popen(args,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT);state.update(state='testing',pid=child.pid,argv=args);save();code=child.wait()
 raw=log.read_bytes();text=raw.decode('utf-8','replace')
 state.update({'exit-code':code,'log-sha256':hashlib.sha256(raw).hexdigest(),'results':re.findall(r'test result: ([^\r\n]+)',text)})
 assert code==101 and '0 passed; 1 failed;' in text,text[-3000:]
 assert 'registry.lookup(&guest)' in text,'expected missing selection assertion not observed'
 state['state']='expected-regression-failed'
except BaseException:state.update(state='failed',error=traceback.format_exc())
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True)
raise SystemExit(0 if state['state']=='expected-regression-failed' else 1)
