from pathlib import Path
import datetime,hashlib,json,os,subprocess,traceback,zipfile
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=B/'proof-mutant-epoch4';R=B/'src-mutant-epoch4';record=S/'mutant-proof-windows-gates.json'
if record.exists() or D.exists():raise RuntimeError('native proof already attempted')
state={'state':'running','pid':os.getpid(),'steps':[],'source-root':str(R),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():record.write_text(json.dumps(state,indent=2)+'\n')
save()
try:
 archive=S/'mutant-final-proof-epoch4.zip';raw=archive.read_bytes();assert hashlib.sha256(raw).hexdigest()=='243a18622a9a2b39a1b1b7a3c0ec95dd20c7c3b99a35074e88b0f425e86bf366';state['archive-sha256']=hashlib.sha256(raw).hexdigest()
 with zipfile.ZipFile(archive)as src:src.extractall(D)
 for name,extra in [('native',['--source-root',str(R)]),('private-bno',[]),('validation',['--source-root',str(R)])]:
  path=S/('mutant-proof-windows-'+name+'.log')
  with path.open('wb')as log:result=subprocess.run([str(B/'python/python.exe'),'-B',str(D/('check-'+name+'.py')),*extra],stdout=log,stderr=subprocess.STDOUT)
  data=path.read_bytes();state['steps'].append({'name':name,'exit-code':result.returncode,'log':path.name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()});save()
  if result.returncode:raise RuntimeError('native proof check failed: '+name)
 state.update(state='finished',**{'exit-code':0})
except BaseException:state.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
