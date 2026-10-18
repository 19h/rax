from pathlib import Path
import datetime,json,os,subprocess,traceback,re
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'session-final-launch.json';lock=B/'session-final-driver.lock'
if record.exists():raise RuntimeError('system-time native launch already exists')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'starting','source':'src-token-session-id','source-inputs':5713}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 for stage in ['core']:
  argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/('native-session-final-'+stage+'.ps1'))]
  with(S/('session-final-'+stage+'-driver.log')).open('wb')as f:
   child=subprocess.Popen(argv,stdout=f,stderr=subprocess.STDOUT);r.update({'state':'running-'+stage,'child-pid':child.pid,'argv':argv});save();code=child.wait()
  r[stage+'-exit-code']=code;save()
  if code:raise RuntimeError('native token '+stage+' failed: '+str(code))
 r.update({'state':'finished','exit-code':0})
except BaseException:r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
