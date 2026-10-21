from pathlib import Path
import hashlib,importlib.util,json,shutil
D=Path('/Users/int/.assist-native-process-mitigation-20261021/docs/specifications/windows/native-process-mitigation')
Q=Path(__file__).parent
spec=importlib.util.spec_from_file_location('process_mitigation_native_proof',D/'check_native_evidence.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
T=Q/'native-proof-stress-v1';assert not T.exists();T.mkdir()
results=[]
for name,version,role,case in [('wrong-grant',2,3,0),('wrong-probe-priority',2,0,18),('readonly-flags-guard-unvisited',4,0,15),('wrong-competing-guard',4,0,23),('ReturnLength-written',1,None,None),('incomplete-native',2,None,None)]:
 d=T/name;shutil.copytree(D,d,ignore=shutil.ignore_patterns('validation','__pycache__'))
 manifest_path=d/'native-producers.json';manifest=json.loads(manifest_path.read_text());producer=next(x for x in manifest['accepted'] if x['version']==version)
 launch_path=d/'native'/producer['launch'];launch=json.loads(launch_path.read_text())
 if name=='incomplete-native':launch['state']='running'
 else:
  for profile in ['arm64','x64','x86','x86-laa']:
   log=d/'native'/f'native-process-mitigation-v{version}-four-views-{profile}.log';lines=log.read_text().splitlines()
   def selected(line):
    if not line.startswith('QUERY '):return False
    f=m.fields(line)
    if version==1:return f['policy']=='11' and f['length']=='8'
    return f['role']==str(role) and f['case']==str(case) and f['length']=='8'
   changed=0
   for i,line in enumerate(lines):
    if not selected(line):continue
    if name=='wrong-grant':line=line.replace('status=C0000022','status=00000000')
    elif name=='wrong-probe-priority':line=line.replace('status=C0000005','status=80000001').replace('out-protection=00000104','out-protection=00000004')
    elif name=='readonly-flags-guard-unvisited':line=line.replace('status=80000001','status=C0000005').replace('next-protection=00000002','next-protection=00000102')
    elif name=='wrong-competing-guard':line=line.replace('ret-protection=00000004','ret-protection=00000104').replace('next-protection=00000102','next-protection=00000002')
    elif name=='ReturnLength-written':line=line.replace('returned=BBBBBBBB','returned=00000008')
    assert line!=lines[i],name;lines[i]=line;changed+=1
   assert changed==(2 if version==1 else 1)
   log.write_text('\n'.join(lines)+'\n');step=next(x for x in launch['steps'] if x['name']==profile);step.update(m.identity(log))
 launch_path.write_text(json.dumps(launch,indent=2)+'\n');identity=m.identity(launch_path);producer.update({'launch-bytes':identity['bytes'],'launch-sha256':identity['sha256']});manifest_path.write_text(json.dumps(manifest,indent=2)+'\n')
 try:m.check(d)
 except ValueError as e:results.append({'name':name,'state':'rejected','reason':str(e),'transitive-identities-repaired':True})
 else:raise RuntimeError('semantic corruption accepted: '+name)
 shutil.rmtree(d);shutil.copytree(D,d,ignore=shutil.ignore_patterns('validation','__pycache__'));assert m.check(d)['state']=='verified';results[-1]['fresh-positive-restoration']='verified'
(Q/'process-mitigation-native-proof-stress-v1.json').write_text(json.dumps({'state':'verified','controls':results,'scope':'All four execution views are changed consistently, with transitive log/launch hashes repaired, before semantic checks reject the corruption.'},indent=2)+'\n')
print(json.dumps(results))
