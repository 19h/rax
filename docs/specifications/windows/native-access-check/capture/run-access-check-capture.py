from pathlib import Path
import json,datetime,subprocess,os,hashlib
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');B=Path('C:/Windows/Temp/assist-native-rax-20261010');P=S/'access-check-capture-launch.json';L=B/'access-check-capture.lock'
if P.exists():raise ValueError('capture already requested')
f=os.open(L,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(f)
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'pid':os.getpid(),'source-sha256':hashlib.sha256((S/'native-access-check-trace.rs').read_bytes()).hexdigest(),'state':'starting'}
def save():P.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 with (S/'access-check-capture-driver.log').open('wb') as out:
  c=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'run-access-check-capture.ps1')],stdout=out,stderr=subprocess.STDOUT)
  r.update({'state':'running','child-pid':c.pid});save();r['exit-code']=c.wait();r['state']='finished'
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();L.unlink()
print(json.dumps(r));raise SystemExit(r['exit-code'])
