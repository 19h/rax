from pathlib import Path
import datetime,json,os,subprocess
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=S/'native-mutant-names-v2-launch.json';lock=B/'native-mutant-names-v2.lock'
if r.exists():raise RuntimeError('native mutant probe already recorded')
f=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(f)
x={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','scope':'Only process-private anonymous mutants and unique own named mutants/events. Existing foreign objects are not opened or changed. All acquired handles are closed and process handle counts compared.'}
def save():r.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 with(S/'native-mutant-names-v2-driver.log').open('wb')as log:
  p=subprocess.Popen(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-mutant-names-v2.ps1')],stdout=log,stderr=subprocess.STDOUT);x['child-pid']=p.pid;save();x['exit-code']=p.wait()
 x['state']='finished'
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(x),flush=True)
raise SystemExit(x['exit-code'])
