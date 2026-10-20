from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,sys,time,traceback
CACHE=Path(__file__).parent;SHARE=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');ROOT=Path('/Users/int/.assist-session-alias-20261020');DOCS=ROOT/'docs/specifications/windows/native-session-directory-alias';V=DOCS/'validation';RECORD=CACHE/'session-alias-proof-controller.json';assert not RECORD.exists();state={'state':'preparing','pid':os.getpid(),'steps':[]}
def load(p):
 try:return json.loads(p.read_text())
 except(FileNotFoundError,json.JSONDecodeError):return{}
def save():state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();RECORD.write_text(json.dumps(state,indent=2)+'\n')
def seal():
 rows=[{'path':p.relative_to(DOCS).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}for p in sorted(DOCS.rglob('*'))if p.is_file()and p.name!='evidence-manifest.json'for raw in[p.read_bytes()]];(DOCS/'evidence-manifest.json').write_text(json.dumps({'files':rows},indent=2)+'\n')
save()
try:
 assert load(CACHE/'session-alias-proof-negative-controls.json')['all-rejected']
 for name in ['run-session-alias-proof-controller.py','package-session-alias-proof.py','run-session-alias-host-proof.py']:shutil.copy2(CACHE/name,V/name)
 shutil.copy2(SHARE/'run-session-alias-native-proof.py',V/'run-session-alias-native-proof.py')
 for number in [1,2]:
  seal();state.update({'state':'running','round':number});save();subprocess.run([sys.executable,'-B',str(CACHE/'package-session-alias-proof.py'),str(number)],check=True)
  children=[(host,subprocess.Popen([sys.executable,'-B',str(CACHE/'run-session-alias-host-proof.py'),host,str(number)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))for host in ['macos','linux']]
  children.append(('windows',subprocess.Popen(['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d','C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe','-B','C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\run-session-alias-native-proof.py',str(number)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)))
  for host,child in children:
   code=child.wait();state['steps'].append({'host':host,'round':number,'transport-exit':code});save()
   if host!='windows':assert code==0,host
  for host in ['macos','linux','windows']:
   base=SHARE if host=='windows'else CACHE;p=base/('session-alias-proof-'+host+'-round'+str(number)+'.json');deadline=time.monotonic()+900
   while load(p).get('state')not in ['finished','failed']:
    assert time.monotonic()<deadline,(host,'no completedreceipt');time.sleep(3)
   row=load(p);assert row['state']=='finished'and row['exit-code']==0,row
   if number==1:
    shutil.copy2(p,V/p.name)
    for step in row['steps']:shutil.copy2(base/step['log'],V/step['log'])
  if number==1:
   summary=load(V/'summary.json');summary['portable-proof-round1']={host:load(V/('session-alias-proof-'+host+'-round1.json'))['envelope']for host in ['macos','linux','windows']};(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 state.update({'state':'finished','exit-code':0})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
