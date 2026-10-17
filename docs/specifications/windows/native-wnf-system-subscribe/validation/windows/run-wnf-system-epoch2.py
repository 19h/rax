from pathlib import Path
import datetime,json,os,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
lock=B/'wnf-system-epoch2-driver.lock';record=S/'wnf-system-epoch2-launch.json'
if record.exists():raise RuntimeError('existing launch must be inspected, not repeated')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','base':'44d21496b9c81e3911e7bbb4160eb13b22ca183d'}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-wnf-system-epoch2-gates.ps1')]
 with (S/'wnf-system-epoch2-driver.log').open('wb') as f:
  child=subprocess.Popen(argv,stdout=f,stderr=subprocess.STDOUT);r.update({'child-pid':child.pid,'argv':argv});save();r['exit-code']=child.wait()
 r['state']='finished'
except BaseException:
 r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:
 r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
