from pathlib import Path
import json,os,subprocess,datetime
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');B=Path('C:/Windows/Temp/assist-native-rax-20261010');record=S/'image-extension-epoch6-launch.json';lock=B/'image-extension-epoch6.lock'
if record.exists():raise RuntimeError('epoch6 already requested')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
j={'state':'running','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():record.write_text(json.dumps(j,indent=2)+'\n')
save()
try:
 with (S/'image-extension-epoch6-driver.log').open('wb')as f:
  p=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-image-extension-epoch6.ps1')],stdout=f,stderr=subprocess.STDOUT);j['child-pid']=p.pid;save();j['exit-code']=p.wait()
 j['state']='finished'if j['exit-code']==0 else 'failed'
finally:j['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(j),flush=True)
raise SystemExit(j['exit-code'])
