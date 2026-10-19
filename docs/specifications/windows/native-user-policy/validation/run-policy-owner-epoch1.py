from pathlib import Path
import datetime,hashlib,json,os,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'policy-owner-epoch1-launch.json';lock=B/'policy-owner-epoch1.lock'
if record.exists():raise RuntimeError('owner observer already launched')
def load(p):
 data=p.read_bytes();return json.loads(data.decode('utf-16')if data[:2]in[b'\xff\xfe',b'\xfe\xff']else data.decode('utf-8-sig'))
def identity(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
def native_control(p):
 result=subprocess.run([str(p)],capture_output=True,timeout=30)
 return {'exit-code':result.returncode,'stdout-hex':result.stdout.hex(),'stderr-hex':result.stderr.hex()}

if load(S/'policy-native-owning-epoch1.json')['state']!='finished':raise RuntimeError('owner validation incomplete')
meta=load(S/'native-policy-epoch1-final-owning-archive-identities.json');core=next(row for row in meta['archives']if row['path'].endswith('.rlib'))
if identity(Path(core['path']))['sha256']!=core['sha256']:raise RuntimeError('selected owner archive differs')
source=S/'policy-owner-epoch1.rs'
if identity(source)['sha256']!='c5b4c58bd15223f91bfa9bd48552e90462b5d977dbda731326af6f01e68506ed':raise RuntimeError('diagnostic source changed')
modules = load(S/'native-user-policy-v3-modules-before.json')
module_before = [identity(Path(row['Path'])) for row in modules]
if [row['sha256'] for row in module_before] != [row['Hash'].lower() for row in modules]:raise RuntimeError('installed runtime differs from final native producer')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'state':'starting','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Read-only diagnostic linked to the exact validated retained-owner core archive; all prefix turns execute normally. Controller exit is separate from guest exit.','archive':core,'source-before':identity(source)}
r['runtime-modules-before'] = module_before
r['svc-map'] = load(S/'native-timer2-set-svc-map.json')
assert r['svc-map']['module-sha256'] == next(row['Hash'].lower() for row in modules if row['Path'].lower().endswith('system32\\ntdll.dll'))
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 with(S/'policy-owner-epoch1-build.log').open('wb')as f:
  child=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'policy-owner-epoch1-build.ps1')],stdout=f,stderr=subprocess.STDOUT);r.update(state='building',child=child.pid);save();code=child.wait()
 if code:raise RuntimeError('owner observer compile failed '+str(code))
 exe=B/'policy-owner-epoch1.exe';r['executable-before']=identity(exe);r['outcomes']=[]
 for i,p in enumerate([B/'src-policy-candidate-epoch1/tests/fixtures/user/windows/bin/arm64/smoke.exe',Path('C:/Windows/System32/whoami.exe')]):
  row={'program-before':identity(p),'native-before':native_control(p),'log':'policy-owner-epoch1-'+str(i)+'.log'};start=time.monotonic()
  with(S/row['log']).open('wb')as f:
   child=subprocess.Popen([str(exe),str(p)],stdout=f,stderr=subprocess.STDOUT);r.update(state='observing-'+str(i),child=child.pid);save()
   try:row['exit-code']=child.wait(timeout=900)
   except subprocess.TimeoutExpired:
    child.terminate()
    try:child.wait(timeout=20)
    except subprocess.TimeoutExpired:child.kill();child.wait()
    raise
  row.update(seconds=time.monotonic()-start,**{'program-after':identity(p),'native-after':native_control(p),'log-identity':identity(S/row['log'])});assert row['native-before']==row['native-after'];r['outcomes'].append(row);save()
  if row['exit-code']:raise RuntimeError('owner observer returned failure')
 r.update(state='finished',exit_code=0,**{'source-after':identity(source),'executable-after':identity(exe),'archive-after':identity(Path(core['path']))})
 if r['source-after']!=r['source-before']or r['executable-after']!=r['executable-before']or r['archive-after']['sha256']!=core['sha256']:raise RuntimeError('post-run diagnostic identity differs')
 r['runtime-modules-after'] = [identity(Path(row['Path'])) for row in modules]
 if r['runtime-modules-after'] != module_before:raise RuntimeError('installed runtime changed during ordinary observations')
except BaseException:r.update(state='failed',exit_code=1,error=traceback.format_exc())
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True);raise SystemExit(r['exit_code'])
