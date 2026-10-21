from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');OLD=B/'src-file-open-epoch1';R=B/'fo-db3';F=S/'file-open-delete-baseline-unit-v3.json';assert not F.exists()and not R.exists();R.mkdir()
s={'state':'preparing','pid':os.getpid(),'steps':[],'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():F.write_text(json.dumps(s,indent=2)+'\n')
def identity(p):
 raw=p.read_bytes();return {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
save()
try:
 mp=S/'file-open-source-epoch1.json';assert identity(mp)['sha256']=='c6ee9720aac4533038b34203269db930fcef98b0f15d90251be1e9c591793be4';manifest=json.loads(mp.read_text());override=json.loads((S/'file-open-delete-baseline-unit-v3-source.json').read_text());assert identity(OLD/override['path'])['sha256']==override['original-sha256']
 for row in manifest['files']:
  path=row['path'];a,b=OLD/path,R/path;b.parent.mkdir(parents=True,exist_ok=True)
  if path==override['path']:shutil.copy2(S/'file-open-delete-baseline-unit-v3-tests.rs',b)
  else:
   try:os.link(a,b)
   except OSError:shutil.copy2(a,b)
 assert identity(R/override['path'])=={k:override[k]for k in ['bytes','sha256']};(R/'target/tmp').mkdir(parents=True)
 env=dict(os.environ)
 for k in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(k,None)
 env.update(json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')));env.update(CARGO_HOME=str(B/'cargo-home'),CARGO_TARGET_DIR=str(B/'src-file-open-epoch2/target/normal'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',RUSTC=str(B/'toolchain/bin/rustc.exe'),TMP=str(R/'target/tmp'),TEMP=str(R/'target/tmp'))
 args=[str(B/'toolchain/bin/cargo.exe'),'test','--locked','--offline','--no-default-features','--lib','native_file_open_failed_delete_sharing_preserves_file_after_final_close','--','--nocapture','--test-threads=1'];p=S/'file-open-delete-baseline-unit-v3.log';start=time.monotonic()
 with p.open('wb')as out:
  child=subprocess.Popen(args,env=env,cwd=R,stdout=out,stderr=subprocess.STDOUT);s.update(state='running',step='baseline-unit',child=child.pid);save();code=child.wait()
 s['steps'].append({'name':'baseline-unit','argv':args,'exit-code':code,'seconds':time.monotonic()-start,'log':p.name,**identity(p)});s['override-after']=identity(R/override['path']);assert s['override-after']=={k:override[k]for k in ['bytes','sha256']};text=p.read_text(errors='replace');assert code==101 and'0 passed; 1 failed;'in text and'code: 2'in text and'native_file_open_failed_delete_sharing_preserves_file_after_final_close ...'in text,text[-6000:]
 s.update(state='finished',**{'exit-code':0,'scope':'Compiled failing regression against frozen candidate1 product with exactly one test-only overlay. Shared Cargo target uses its normal lock; source roots remain separate, and no active artifact/source is overwritten. Expected missing fixture after a rejected sharing open is a baseline failure, not a validation pass.'})
except BaseException:s.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:s['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(s),flush=True);raise SystemExit(s['exit-code'])
