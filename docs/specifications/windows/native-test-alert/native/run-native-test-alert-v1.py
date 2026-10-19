from pathlib import Path
import datetime,hashlib,json,os,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');record=S/'native-test-alert-v1-launch.json';lock=B/'native-test-alert-v1.lock'
assert not record.exists()
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
r={'state':'running','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Private worker threads in the producer process only. No other-process APC queue, executable memory mutation, real Assist state or Defender policy change.'}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 raw=(S/'native-test-alert-v1.cpp').read_bytes();assert hashlib.sha256(raw).hexdigest()=='6393a325c24f1b1c93bed00b05d8d4ecb54fab80693962f3bc278560f0fe9172'
 r['source-before']={'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)};save()
 with (S/'native-test-alert-v1-driver.log').open('wb')as log:
  child=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-test-alert-v1.ps1')],stdout=log,stderr=subprocess.STDOUT);r['child']=child.pid;save();code=child.wait()
 r.update(state='finished',**{'exit-code':code})
 after=(S/'native-test-alert-v1.cpp').read_bytes();r['source-after']={'sha256':hashlib.sha256(after).hexdigest(),'bytes':len(after)};assert r['source-after']==r['source-before']
except BaseException:r.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True);raise SystemExit(r['exit-code'])
