from pathlib import Path
import datetime,json,os,subprocess,traceback
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');B=Path('C:/Windows/Temp/assist-native-rax-20261010')
record=S/'session-one-directories-v1-launch.json';lock=B/'session-one-directories-v1.lock'
if record.exists():raise RuntimeError('already launched')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'state':'starting','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 with(S/'session-one-directories-v1-driver.log').open('wb')as f:
  p=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-session-one-directories-v1.ps1')],stdout=f,stderr=subprocess.STDOUT);r.update(state='running',child=p.pid);save();code=p.wait()
 r.update(state='finished',exit_code=code)
except BaseException:r.update(state='failed',exit_code=1,error=traceback.format_exc())
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit_code'])
