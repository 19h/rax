from pathlib import Path
import hashlib,json,shutil
K=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-token-20261019');D=R/'docs/specifications/windows/native-token-security-attributes';V=D/'validation'
driver=K/('attributes-portable-driver-b.json' if (K/'attributes-portable-driver-b.json').exists() else 'attributes-portable-driver.json')
assert json.loads(driver.read_text())['state']=='finished'
envelope=D/'evidence-manifest.json';raw=envelope.read_bytes();count=len(json.loads(raw)['files'])
assert not (V/'checked-evidence-envelope.json').exists()
(V/'checked-evidence-envelope.json').write_bytes(raw)
rows=[]
for host in ['macos','linux','windows']:
 base=S if host=='windows' else K;p=base/('attributes-final-check-'+host+'.json');r=json.loads(p.read_text());assert r['state']=='finished' and r['exit-code']==0 and r['checked-envelope-files']==count
 for name in [p.name,r['log']]:shutil.copy2(base/name,V/name)
 rows.append({'host':host,'receipt':p.name,'checked-envelope-files':count})
shutil.copy2(K/'attributes-final-proof-archive.json',V/'attributes-final-proof-archive.json')
(V/'proof-receipts.json').write_text(json.dumps({'checked-envelope-sha256':hashlib.sha256(raw).hexdigest(),'checked-envelope-files':count,'receipts':rows,'scope':'All original checked files remain byte-identical; portable receipts and staging audit extend the current envelope without changing the original checked envelope.'},indent=2)+'\n')
d=json.loads(raw);d['files']=[]
for p in sorted(D.rglob('*')):
 if p.is_file() and p!=envelope:
  data=p.read_bytes();d['files'].append({'path':p.relative_to(D).as_posix(),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
envelope.write_text(json.dumps(d,indent=2)+'\n');print('Collected3 proof receipts; original',count,'bound inputs preserved')
