from pathlib import Path
import datetime,json,os,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');P=S/'access-check-initial-launch.json';L=B/'access-check-initial.lock'
if P.exists():raise RuntimeError('initial access check already requested')
prior=json.loads((S/'token-query-epoch2-launch.json').read_text())
if prior.get('state')!='finished' or prior.get('exit-code')!=0:raise RuntimeError('prior native driver incomplete')
f=os.open(L,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(f)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'starting','base':'23ac34c5d0d0f6afff762431442d2fab52cda7f9'}
def save():P.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 with (S/'access-check-initial-driver.log').open('wb') as out:
  a=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-access-check-initial-gates.ps1')]
  c=subprocess.Popen(a,stdout=out,stderr=subprocess.STDOUT);r.update({'state':'running','child-pid':c.pid,'argv':a});save();r['exit-code']=c.wait();r['state']='finished'
except BaseException:r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();L.unlink()
print(json.dumps(r));raise SystemExit(r['exit-code'])
