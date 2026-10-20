from pathlib import Path
import datetime,json,os,subprocess,time,traceback
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-file-attributes-20261020');F=Q/'file-attributes-proof-controller-epoch3.json';assert not F.exists();state={'state':'waiting-for-final-gates','pid':os.getpid(),'steps':[]}
def save():F.write_text(json.dumps(state,indent=2)+'\n')
def load(p):
 try:return json.loads(p.read_text())
 except (FileNotFoundError,json.JSONDecodeError):return None
def run(name,args,cwd=Q):
 log=Q/('file-attributes-proof-controller-epoch3-'+name+'.log');started=time.monotonic();state['current-step']=name;save()
 with log.open('wb')as output:code=subprocess.run(args,cwd=cwd,stdout=output,stderr=subprocess.STDOUT).returncode
 state['steps'].append({'name':name,'argv':args,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name});save();print(name,code,flush=True);assert code==0,name
save()
try:
 pending=[(Q,'file-attributes-macos-epoch3-gates.json'),(Q,'file-attributes-linux-epoch3-gates.json'),(Q,'macos-file-attributes-epoch4-owning-gates.json'),(Q,'linux-file-attributes-epoch4-owning-gates.json'),(Q,'file-attributes-windows-cross-xwin-epoch4.json'),(S,'file-attributes-native-epoch3-gates.json'),(S,'file-attributes-native-owning-epoch3.json'),(S,'file-attributes-native-ordinary-epoch3.json'),(S,'file-attributes-owning-capi-epoch3.json')]
 while pending:
  remaining=[]
  for base,name in pending:
   d=load(base/name)
   if d is not None:
    assert d.get('state')!='failed',(name,d.get('error'))
    if d.get('state')=='finished':assert d['exit-code']==0,name;print('ready',name,flush=True);continue
   remaining.append((base,name))
  pending=remaining;state['pending']=[name for _,name in pending];save()
  if pending:time.sleep(5)
 state['state']='auditing';save()
 name='retain-file-attributes-artifacts-epoch3.py';(S/name).write_bytes((Q/name).read_bytes())
 run('retain',['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d','C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe','-B','C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\'+name])
 run('pack',['python3','-B',str(Q/'pack-file-attributes-public-epoch3.py')])
 # README is written only after actual final program outcomes are present.
 text=(Q/'final-readme-draft.md').read_text().replace('Measured prototype smoke exits with code 0; whoami advances to NtOpenFile (0x33).','Final-source smoke exits with code 0; whoami advances to NtOpenFile (0x33).')
 (R/'docs/specifications/windows/native-file-attributes/README.md').write_text(text)
 run('seal',['python3','-B',str(Q/'seal-file-attributes-proof.py')])
 run('audit',['python3','-B',str(R/'docs/specifications/windows/native-file-attributes/check_validation.py'),'--source-root',str(R)])
 run('stress',['python3','-B',str(R/'docs/specifications/windows/native-file-attributes/stress_validation.py')])
 stress=json.loads((Q/'file-attributes-proof-controller-epoch3-stress.log').read_text());assert stress['all-rejected']and len(stress['controls'])==13
 (R/'docs/specifications/windows/native-file-attributes/validation/negative-controls.json').write_text(json.dumps(stress,indent=2)+'\n')
 run('final-seal',['python3','-B',str(Q/'seal-file-attributes-proof.py')])
 run('package',['python3','-B',str(Q/'package-file-attributes-proof-epoch3.py')])
 state.update({'state':'finished','exit-code':0})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
