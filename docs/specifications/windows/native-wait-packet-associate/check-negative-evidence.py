from pathlib import Path
import hashlib,json,shutil,subprocess,sys
base=Path(__file__).parent
root=Path('/Users/int/hexrays/kvasir/vendor/rax')
evidence=root/'docs/specifications/windows/native-wait-packet-associate'
script=evidence/'check_native_wait_packet_associate.py'
results=[]
def rehash(directory,name):
 p=directory/'evidence-manifest.json';v=json.loads(p.read_text());f=directory/name;v[name]={'bytes':f.stat().st_size,'sha256':hashlib.sha256(f.read_bytes()).hexdigest()};p.write_text(json.dumps(v,indent=2)+'\n')
for kind in ['manifest-omission','raw-corruption','semantic-status','empty-filter','forged-summary','optimized-python']:
 d=base/('final-negative-'+kind);shutil.copytree(evidence,d)
 if kind=='manifest-omission':
  p=d/'evidence-manifest.json';v=json.loads(p.read_text());v.pop('native-wait-packet-association-probe-arm64.log');p.write_text(json.dumps(v))
 if kind=='raw-corruption':
  p=d/'native-wait-packet-association-probe-arm64.log';p.write_bytes(p.read_bytes()+b'corrupt\n')
 if kind=='semantic-status':
  name='native-wait-packet-association-validation-arm64.log';p=d/name;p.write_bytes(p.read_bytes().replace(b'status=00000000',b'status=FFFFFFFF',1));rehash(d,name)
  p=d/'native-wait-packet-association-producer-identities.json';v=json.loads(p.read_text(encoding='utf-8-sig'));row=next(x for x in v['producer'] if x['path'].replace('\\','/').endswith('/'+name));row['bytes']=(d/name).stat().st_size;row['sha256']=hashlib.sha256((d/name).read_bytes()).hexdigest();p.write_text(json.dumps(v,indent=2)+'\n');rehash(d,p.name)
 if kind=='empty-filter':
  name='macos-targeted.log';p=d/name;p.write_text('running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n');rehash(d,name)
 if kind=='forged-summary':
  p=d/'validation-summary.json';v=json.loads(p.read_text());v['tests']['targeted']['macos']=0;p.write_text(json.dumps(v,indent=2)+'\n');rehash(d,p.name)
 command=[sys.executable]+(['-O'] if kind=='optimized-python' else [])+[str(script),str(d),str(root)]
 r=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 log=base/('negative-'+kind+'.log');log.write_text(r.stdout)
 if r.returncode==0:raise ValueError('checker accepted invalid evidence '+kind)
 results.append({'case':kind,'exit-code':r.returncode,'result':'rejected','command':command,'error':r.stdout.splitlines()[-1]})
 print(kind,'rejected',flush=True)
(base/'negative-validation.json').write_text(json.dumps(results,indent=2)+'\n')
