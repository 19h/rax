from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,sys,time,traceback
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue';V=D/'validation/epoch3';record=Q/'continue-epoch3-proof-controller-b.json';assert not record.exists()
r={'state':'waiting-for-collected-current-ledger','pid':os.getpid(),'steps':[]}
def save():r['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();record.write_text(json.dumps(r,indent=2)+'\n')
def load(p):
 try:return json.loads(p.read_text())
 except(FileNotFoundError,json.JSONDecodeError):return{}
def run(name,args):
 log=Q/('continue-epoch3-proof-b-'+name+'.log');r.update(state='running',step=name);save()
 with log.open('wb')as out:code=subprocess.run(args,stdout=out,stderr=subprocess.STDOUT).returncode
 r['steps'].append({'name':name,'exit-code':code,'log':log.name});save();assert code==0,log.read_text(errors='replace')
def envelope():
 files=[]
 for p in sorted(D.rglob('*')):
  if p.is_file()and p.name!='evidence-manifest.json':
   raw=p.read_bytes();files.append({'path':p.relative_to(D).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 (D/'evidence-manifest.json').write_text(json.dumps({'files':files},indent=2)+'\n')
save()
try:
 while not(V/'quality-gates.json').exists()or'## Current epoch3 validation'not in(D/'README.md').read_text():time.sleep(5)
 run('negative-controls',[sys.executable,'-B',str(Q/'stress-continue-epoch3.py')])
 for name in ['run-continue-host-verification-epoch3.py','run-continue-native-verification-epoch3.py','package-continue-verification-docs-epoch3.py','run-continue-epoch3-proof-b.py']:shutil.copy2(Q/name,V/name)
 for number in[1,2]:
  envelope();run('package-round'+str(number),[sys.executable,'-B',str(Q/'package-continue-verification-docs-epoch3.py'),str(number)])
  children=[]
  for host in['macos','linux']:
   children.append((host,subprocess.Popen([sys.executable,'-B',str(Q/'run-continue-host-verification-epoch3.py'),host,str(number)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)))
  children.append(('windows',subprocess.Popen(['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d','C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe','-B','C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\run-continue-native-verification-epoch3.py',str(number)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)))
  for host,child in children:
   code=child.wait();r['steps'].append({'name':host+'-round'+str(number)+'-transport','exit-code':code});save()
   if host!='windows':assert code==0,host
  for host in['macos','linux','windows']:
   base=S if host=='windows'else Q;name='continue-epoch3-final-check-'+host+'-round'+str(number)+'.json'
   while load(base/name).get('state')not in['finished','failed']:time.sleep(5)
   result=load(base/name);assert result['state']=='finished'and result['exit-code']==0,json.dumps(result)
   if number==1:
    shutil.copy2(base/name,V/name)
    for row in result['steps']:shutil.copy2(base/row['log'],V/row['log'])
  if number==1:
   summary=load(V/'summary.json');summary['portable-proof-round1']={host:{'exit-code':0,'envelope':load(V/('continue-epoch3-final-check-'+host+'-round1.json'))['envelope']}for host in['macos','linux','windows']};(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 r.update(state='finished',**{'exit-code':0})
except BaseException:r.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:save()
print(json.dumps(r,indent=2));raise SystemExit(r['exit-code'])
