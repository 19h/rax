from pathlib import Path
import datetime,hashlib,json,os,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'links-owner-observer-launch.json';lock=B/'links-owner-observer.lock'
if record.exists():raise RuntimeError('owner observer already launched')
def load(p):
 data=p.read_bytes();return json.loads(data.decode('utf-16')if data[:2]in[b'\xff\xfe',b'\xfe\xff']else data.decode('utf-8-sig'))
def identity(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
if load(S/'links-final-owned-launch.json')['state']!='finished':raise RuntimeError('owner validation incomplete')
meta=load(S/'native-links-final-owning-archive-identities.json');core=next(row for row in meta['archives']if row['path'].endswith('.rlib'))
if identity(Path(core['path']))['sha256']!=core['sha256']:raise RuntimeError('selected owner archive differs')
source=S/'links-owner-observer.rs'
if identity(source)['sha256']!='1a5d047c324cdcc365d697273bacca64536d0c4366ee58dc0e66764119efa907':raise RuntimeError('diagnostic source changed')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'state':'starting','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Additional diagnostic linked to the validated retained-owner core archive; final core-driver observations remain separate.','archive':core,'source-before':identity(source)}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 with(S/'links-owner-observer-build.log').open('wb')as f:
  child=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'links-owner-observer-build.ps1')],stdout=f,stderr=subprocess.STDOUT);r.update(state='building',child=child.pid);save();code=child.wait()
 if code:raise RuntimeError('owner observer compile failed '+str(code))
 exe=B/'links-owner-observer.exe';r['executable-before']=identity(exe);r['outcomes']=[]
 for i,p in enumerate([B/'src-object-reparse-epoch1/tests/fixtures/user/windows/bin/arm64/smoke.exe',Path('C:/Windows/System32/whoami.exe')]):
  row={'program-before':identity(p),'log':'links-owner-observer-'+str(i)+'.log'};start=time.monotonic()
  with(S/row['log']).open('wb')as f:
   child=subprocess.Popen([str(exe),str(p)],stdout=f,stderr=subprocess.STDOUT);r.update(state='observing-'+str(i),child=child.pid);save();row['exit-code']=child.wait(timeout=900)
  row.update(seconds=time.monotonic()-start,**{'program-after':identity(p),'log-identity':identity(S/row['log'])});r['outcomes'].append(row);save()
  if row['exit-code']:raise RuntimeError('owner observer returned failure')
 r.update(state='finished',exit_code=0,**{'source-after':identity(source),'executable-after':identity(exe),'archive-after':identity(Path(core['path']))})
 if r['source-after']!=r['source-before']or r['executable-after']!=r['executable-before']or r['archive-after']['sha256']!=core['sha256']:raise RuntimeError('post-run diagnostic identity differs')
except BaseException:r.update(state='failed',exit_code=1,error=traceback.format_exc())
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True);raise SystemExit(r['exit_code'])
