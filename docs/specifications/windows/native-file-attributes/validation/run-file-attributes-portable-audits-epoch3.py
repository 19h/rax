"""Run two audits on each host against the identical sealed public corpus."""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import datetime,hashlib,json,subprocess,time,traceback
Q=Path(__file__).parent;R=Path('/Users/int/.assist-file-attributes-20261020');S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=R/'docs/specifications/windows/native-file-attributes';F=Q/'file-attributes-portable-audits-epoch3.json';assert not F.exists()
envelope=D/'validation-envelope.json';seal=hashlib.sha256(envelope.read_bytes()).hexdigest();state={'state':'running','envelope-sha256':seal,'steps':[]}
def save():F.write_text(json.dumps(state,indent=2)+'\n')
def run(host,number,args):
 log=Q/('file-attributes-proof-'+host+'-round'+str(number)+'.log');started=time.monotonic()
 with log.open('wb')as out:code=subprocess.run(args,stdout=out,stderr=subprocess.STDOUT).returncode
 raw=log.read_bytes();row={'host':host,'round':number,'argv':args,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
 assert code==0,(host,number,log.name)
 value=json.loads(raw);assert value['state']=='verified'and value['source-files']==25097 and value['native-observations']==3408 and value['native-retained-bytes-verified']==(host=='windows'),value
 print(host,number,'verified',flush=True);return row
save()
try:
 a=str(D/'check_validation.py');winroot='C:\\Windows\\Temp\\assist-native-rax-20261010\\verify-file-attributes-epoch3';wincheck=winroot+'\\docs\\specifications\\windows\\native-file-attributes\\check_validation.py';commands={'macos':['python3','-B',a,'--source-root',str(R)],'linux':['docker','run','--rm','--platform','linux/amd64','-v',str(R)+':/work/vendor/rax:ro','assist-rax-validation-rustfmt:1.95-bullseye-20261015','python3','-B','/work/vendor/rax/docs/specifications/windows/native-file-attributes/check_validation.py','--source-root','/work/vendor/rax'],'windows':['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d','C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe','-B',wincheck,'--source-root',winroot]}
 for number in [1,2]:
  assert hashlib.sha256(envelope.read_bytes()).hexdigest()==seal
  with ThreadPoolExecutor(max_workers=3)as pool:rows=list(pool.map(lambda h:run(h,number,commands[h]),['macos','linux','windows']))
  state['steps'].extend(rows);save()
 assert hashlib.sha256(envelope.read_bytes()).hexdigest()==seal;state.update({'state':'finished','exit-code':0})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
