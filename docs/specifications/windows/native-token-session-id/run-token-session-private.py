from pathlib import Path
import datetime,json,os,subprocess
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=s/'token-session-private-launch.json';lock=b/'token-session-private.lock'
if r.exists():raise RuntimeError('existing probe launch')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
x={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','scope':'query nonzero session IDs on child-owned duplicate tokens; existing privilege enabled only on a private token and selected only on the child thread'}
def save():r.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 a=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-token-session-private.ps1')]
 with (s/'token-session-private-driver.log').open('wb')as f:
  p=subprocess.Popen(a,stdout=f,stderr=subprocess.STDOUT);x.update({'child-pid':p.pid,'argv':a});save();x['exit-code']=p.wait()
 x['state']='finished'
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(x),flush=True)
raise SystemExit(x['exit-code'])
