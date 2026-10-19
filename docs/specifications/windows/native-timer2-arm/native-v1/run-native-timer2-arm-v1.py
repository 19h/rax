from pathlib import Path
import datetime,json,os,subprocess
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=S/'native-timer2-arm-v1-launch.json';lock=B/'native-timer2-arm-v1.lock'
if r.exists():raise RuntimeError('native semaphore probe already recorded')
f=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(f)
x={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','scope':'Anonymous private timer handles only; set/poll/cancel/close, finite waits, status/access/value matrix. No registry/security/power policy changes.'}
def save():r.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 with(S/'native-timer2-arm-v1-driver.log').open('wb')as log:
  p=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-timer2-arm-v1.ps1')],stdout=log,stderr=subprocess.STDOUT);x['child-pid']=p.pid;save();x['exit-code']=p.wait()
 x['state']='finished'
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(x),flush=True)
raise SystemExit(x['exit-code'])
