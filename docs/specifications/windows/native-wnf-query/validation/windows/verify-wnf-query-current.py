from pathlib import Path
import hashlib,json,datetime
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=json.loads((s/'wnf-query-epoch3-retry-launch.json').read_text())
if r.get('state')!='finished' or r.get('exit-code')!=0:raise RuntimeError('corrected native focused gates must pass first')
m=json.loads((s/'wnf-query-final-owning-source-hashes.json').read_text())
for row in m['sources']:
 p=b/'src'/row['path'];raw=p.read_bytes()
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('source changed: '+row['path'])
for p,h in [(b/'src/.cargo/config.toml','9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'),(Path('C:/Windows/System32/ntdll.dll'),'502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f')]:
 if hashlib.sha256(p.read_bytes()).hexdigest()!=h:raise RuntimeError('native configuration/platform changed: '+str(p))
(s/'native-wnf-query-final-source-verification.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'sources':m['sources'],'selected-count':len(m['sources']),'scope':'same corrected focused source, verified before final non-test and full gates'},indent=2)+'\n')
print('Verified',len(m['sources']),'current final native inputs')
