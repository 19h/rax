from pathlib import Path
import json,hashlib,subprocess,shutil
base=Path(__file__).parent
rax=Path('/Users/int/hexrays/kvasir/vendor/rax')
e=rax/'docs/specifications/windows/native-wait-packet-create'
def manifest(root):
 files=[{'path':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(root.iterdir()) if p.is_file() and p.name!='manifest.json']
 (root/'manifest.json').write_text(json.dumps({'files':files},indent=2)+'\n')
manifest(e);results=[]
for name in ['positive-final','manifest-omission','raw-corruption','semantic-status','empty-filtered-gate','forged-summary','optimized-python']:
 target=e
 if name!='positive-final':
  target=base/('final-negative-'+name);shutil.copytree(e,target)
 if name=='manifest-omission':
  p=target/'manifest.json';m=json.loads(p.read_text());m['files']=m['files'][1:];p.write_text(json.dumps(m)+'\n')
 elif name in ['raw-corruption','semantic-status']:
  p=target/'native-wait-packet-create-probe-arm64.log';s=p.read_text();p.write_text(s.replace('status=00000000','status=C0000005',1))
  if name=='semantic-status':manifest(target)
 elif name in ['empty-filtered-gate','forged-summary']:
  p=target/'gate-results.json';g=json.loads(p.read_text());r=next(r for r in g if r['host']=='macos' and r['gate']=='targeted')
  changed=r['test-results'][0].replace('7 passed','0 passed')
  if name=='empty-filtered-gate':r['test-results'][0]=changed
  log=target/r['original-path'];log.write_text(log.read_text().replace('test result: ok. 7 passed','test result: ok. 0 passed'));p.write_text(json.dumps(g)+'\n');manifest(target)
 cmd=['python3']+(['-O'] if name=='optimized-python' else [])+[str(target/'check_native_wait_packet.py'),'--evidence-root',str(target),'--source-root',str(rax)]
 with (base/('final-'+name+'.log')).open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 results.append({'gate':name,'exit-code':r.returncode,'command':cmd});print(name,r.returncode,flush=True)
 if (r.returncode==0)!=(name=='positive-final'):raise RuntimeError('wrong evidence gate outcome')
(base/'final-evidence-check-results.json').write_text(json.dumps(results,indent=2)+'\n')
for row in results:
 name='final-'+row['gate']+'.log';(e/name).write_bytes((base/name).read_bytes())
(e/'evidence-check-results.json').write_bytes((base/'final-evidence-check-results.json').read_bytes())
(e/'finalize-evidence.py').write_bytes(Path(__file__).read_bytes())
manifest(e)
subprocess.run(['python3',str(e/'check_native_wait_packet.py'),'--source-root',str(rax)],check=True)
