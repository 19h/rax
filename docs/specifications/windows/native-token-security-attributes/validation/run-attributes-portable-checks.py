from pathlib import Path
import concurrent.futures,datetime,hashlib,json,subprocess,sys,time,traceback
K=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-token-20261019');D=R/'docs/specifications/windows/native-token-security-attributes';V=D/'validation';record=K/'attributes-portable-driver.json'
assert not record.exists()
state={'state':'waiting-proof','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[]}
def save():record.write_text(json.dumps(state,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+2400
 while True:
  d=json.loads((K/'attributes-finalization.json').read_text());assert d['state']!='failed',d.get('error')
  if d['state']=='ready-for-portable-checks':break
  assert time.monotonic()<deadline,'Proof not ready before observation deadline';time.sleep(5)
 __import__('shutil').copy2(K/'run-attributes-final-windows-check.py',S/'run-attributes-final-windows-check.py')
 commands=[('macos',[sys.executable,'-B',str(K/'run-attributes-final-unix-check.py'),'macos']),('linux',[sys.executable,'-B',str(K/'run-attributes-final-unix-check.py'),'linux']),('windows',['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d','C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe','-B','C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\run-attributes-final-windows-check.py'])]
 state['state']='checking';save()
 def run(item):
  host,argv=item;path=K/('attributes-portable-driver-'+host+'.log')
  with path.open('wb')as out:code=subprocess.run(argv,stdout=out,stderr=subprocess.STDOUT).returncode
  return {'host':host,'exit-code':code,'log':path.name}
 with concurrent.futures.ThreadPoolExecutor(max_workers=3)as pool:
  for result in pool.map(run,commands):state['steps'].append(result);save();print(result,flush=True)
 assert all(x['exit-code']==0 for x in state['steps'])
 state['state']='finished';state['exit-code']=0
except BaseException:state.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()});save();raise
save();print('All three portable proof checks completed',flush=True)
