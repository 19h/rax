from pathlib import Path
import hashlib, importlib.util, json, shutil
D=Path('/Users/int/.assist-native-nls-sorting-20261021/docs/specifications/windows/native-nls-sorting')
Q=Path(__file__).parent
spec=importlib.util.spec_from_file_location('native_nls_proof',D/'check_native_evidence.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
T=Q/'native-proof-stress-v1';assert not T.exists();T.mkdir()
results=[]
for name in ['wrong-root-status','different-view-value','duplicated-input','handle-leak','incomplete-launch']:
 d=T/name;shutil.copytree(D,d,ignore=shutil.ignore_patterns('validation','__pycache__'))
 p=d/'native/native-nls-sorting-v1-four-views-arm64.log'
 lines=p.read_text().splitlines()
 if name=='wrong-root-status':
  i=next(i for i,l in enumerate(lines) if l.startswith('OPEN sample=0 path=0 desired=00000001'))
  lines[i]=lines[i].replace('status=00000000','status=C0000034')
 elif name=='different-view-value':
  i=next(i for i,l in enumerate(lines) if l.startswith('VALUE view=00000100 kind=1 name= data='))
  lines[i]=lines[i].replace('data=3000','data=3100')
 elif name=='duplicated-input':
  indexes=[i for i,l in enumerate(lines) if l.startswith('OPEN ')]
  lines[indexes[0]]=lines[indexes[1]]
 elif name=='handle-leak':
  i=next(i for i,l in enumerate(lines) if l.startswith('SUMMARY '))
  fields=m.fields(lines[i]);after=int(fields['handles-after']);lines[i]=lines[i].replace('handles-after='+str(after),'handles-after='+str(after+1))
 if name!='incomplete-launch':p.write_text('\n'.join(lines)+'\n')
 launch_path=d/'native/native-nls-sorting-v1-four-views-launch.json'
 launch=json.loads(launch_path.read_text())
 if name=='incomplete-launch':launch['state']='running'
 else:
  step=next(s for s in launch['steps'] if s['name']=='arm64')
  step.update(m.identity(p))
 launch_path.write_text(json.dumps(launch,indent=2)+'\n')
 producer_path=d/'native-producer.json';producer=json.loads(producer_path.read_text());identity=m.identity(launch_path);producer.update({'launch-bytes':identity['bytes'],'launch-sha256':identity['sha256']});producer_path.write_text(json.dumps(producer,indent=2)+'\n')
 try:m.check(d)
 except ValueError as e:results.append({'name':name,'state':'rejected','reason':str(e),'transitive-identities-repaired':True})
 else:raise RuntimeError('corruption accepted: '+name)
 shutil.rmtree(d);shutil.copytree(D,d,ignore=shutil.ignore_patterns('validation','__pycache__'))
 positive=m.check(d);assert positive['state']=='verified';results[-1]['fresh-restoration']='verified'
(Q/'nls-sorting-native-proof-stress-v1.json').write_text(json.dumps({'state':'verified','negative-controls':results,'scope':'Semantic corruptions rejected after log/launch receipt hashes are repaired; each negative followed by fresh positive restoration.'},indent=2)+'\n')
print(json.dumps(results))
