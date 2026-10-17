from pathlib import Path
import datetime,json,os,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'image-extension-final-owned-launch.json';lock=B/'image-extension-final-owned-driver.lock'
if record.exists():raise RuntimeError('owning validation already requested; inspect existing state')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'starting','base':'bb697c7da3400d3fc2905cfbcfd7b4e68306ceee','source-epoch':5}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'assist-native-image-extension-final-owned.ps1')]
 with(S/'image-extension-final-owned-driver.log').open('wb')as f:
  child=subprocess.Popen(argv,stdout=f,stderr=subprocess.STDOUT);r.update({'state':'running','child-pid':child.pid,'argv':argv});save();code=child.wait()
 if code:raise RuntimeError('native owning exit '+str(code))
 r.update({'state':'finished','exit-code':0})
except BaseException:r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
