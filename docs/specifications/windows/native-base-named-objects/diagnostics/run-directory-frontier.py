from pathlib import Path
import datetime,hashlib,json,os,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=B/'directory-frontier';D.mkdir(exist_ok=True)
record=S/'directory-frontier-launch.json';lock=D/'driver.lock'
if record.exists():raise RuntimeError('diagnostic already launched; inspect its record')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'state':'starting','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'new read-only diagnostic observer; original W observer unchanged'}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
def identity(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
save()
try:
 source=S/'directory-frontier-trace.rs';r['source-before']=identity(source);save()
 with(S/'directory-frontier-build.log').open('wb')as out:
  child=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'directory-frontier-build.ps1')],stdout=out,stderr=subprocess.STDOUT);r.update(state='building',child=child.pid);save();code=child.wait()
 if code:raise RuntimeError('diagnostic compile failed '+str(code))
 exe=D/'directory-frontier-trace.exe';r['executable-before']=identity(exe);r['outcomes']=[]
 for i,target in enumerate([B/'src-token-scalars/tests/fixtures/user/windows/bin/arm64/smoke.exe',Path('C:/Windows/System32/whoami.exe')]):
  entry={'program':identity(target),'log':'directory-frontier-'+str(i)+'.log'};start=time.monotonic()
  with(S/entry['log']).open('wb')as out:
   child=subprocess.Popen([str(exe),str(target)],stdout=out,stderr=subprocess.STDOUT);r.update(state='observing-'+str(i),child=child.pid);save();entry['exit-code']=child.wait()
  entry['seconds']=time.monotonic()-start;entry['log-identity']=identity(S/entry['log']);entry['program-after']=identity(target);r['outcomes'].append(entry);save()
  if entry['exit-code']:raise RuntimeError('diagnostic observer failed')
 r['source-after']=identity(source);r['executable-after']=identity(exe)
 if r['source-after']!=r['source-before']or r['executable-after']!=r['executable-before']:raise RuntimeError('diagnostic identities changed')
 r.update(state='finished',exit_code=0)
except BaseException:r.update(state='failed',exit_code=1,error=traceback.format_exc())
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit_code'])
