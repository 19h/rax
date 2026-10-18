from pathlib import Path
import datetime,json,os,subprocess
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=s/'bno-isolation-child-launch.json';lock=b/'bno-isolation-child.lock'
if r.exists():raise RuntimeError('existing probe launch')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
x={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','scope':'private child process BNO isolation controls; no existing process token or Defender settings changed; only child-owned token/event handles and memory'}
def save():r.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 a=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-bno-isolation-child.ps1')]
 with (s/'bno-isolation-child-driver.log').open('wb')as f:
  p=subprocess.Popen(a,stdout=f,stderr=subprocess.STDOUT);x.update({'child-pid':p.pid,'argv':a});save();x['exit-code']=p.wait()
 x['state']='finished'
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(x),flush=True)
raise SystemExit(x['exit-code'])
