from pathlib import Path
import datetime,json,os,subprocess
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');record=s/'session-id-epoch2-launch.json';lock=b/'session-id-epoch2.lock'
if record.exists():raise RuntimeError('existing token epoch launch')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running'}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 args=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-session-id-epoch2.ps1')]
 with (s/'session-id-epoch2-driver.log').open('wb')as f:
  p=subprocess.Popen(args,stdout=f,stderr=subprocess.STDOUT);r.update({'child-pid':p.pid,'argv':args});save();r['exit-code']=p.wait()
 r['state']='finished'
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
