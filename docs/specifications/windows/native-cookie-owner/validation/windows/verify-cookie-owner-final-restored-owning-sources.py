from pathlib import Path
import json,hashlib,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=Path('C:/Windows/Temp/assist-native-rax-20261010/src');j=json.loads((s/'cookie-owner-final-restored-owning-source-hashes.json').read_text());rows=[]
for row in j['sources']:
 data=(r/row['path']).read_bytes()
 if len(data)!=row['bytes'] or hashlib.sha256(data).hexdigest()!=row['sha256']:raise ValueError('console-owner owning native source '+row['path'])
 rows.append(row)
expected='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
if hashlib.sha256((r/'.cargo/config.toml').read_bytes()).hexdigest()!=expected:raise ValueError('native Cargo adapter drift')
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':2,'verified-count':len(rows),'sources':rows,'native-cargo-adapter-sha256':expected,'scope':'309selected current console-owner inputs;13ownedRust and4native replay captures;not a whole root/source claim'}
(s/'native-cookie-owner-final-restored-owning-source-identities.json').write_text(json.dumps(record,indent=2)+'\n');print('Verified',len(rows),'actual console-owner owning source identities',flush=True)
