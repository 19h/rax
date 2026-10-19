from pathlib import Path
import datetime,json,os,subprocess
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=S/'native-sid-text-v1-launch.json';lock=B/'native-sid-text-v1.lock'
if r.exists():raise RuntimeError('native User policy probe already recorded')
f=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(f)
x={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','scope':'Pure native SID formatting for bounded owned synthetic SID bytes; no token, registry, thread impersonation or security policy mutation.'}
def save():r.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 with(S/'native-sid-text-v1-driver.log').open('wb')as log:
  p=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-sid-text-v1.ps1')],stdout=log,stderr=subprocess.STDOUT);x['child-pid']=p.pid;save();x['exit-code']=p.wait()
 x['state']='finished'
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(x),flush=True)
raise SystemExit(x['exit-code'])
