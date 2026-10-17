from pathlib import Path
import hashlib,json
b=Path('C:/Windows/Temp/assist-native-rax-20261010/src');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');t=s/'wnf-system-epoch2-transfer'
m=json.loads((t/'initial-source-inputs.json').read_text());whole=json.loads((t/'initial-owning-source-hashes.json').read_text())
for row in m['files']:
 p=t/row['path'];raw=p.read_bytes()
 if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('transfer mismatch '+row['path'])
for row in m['files']:
 p=b/row['path'];p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((t/row['path']).read_bytes())
for row in whole['sources']:
 p=b/row['path'];raw=p.read_bytes()
 if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('selected source mismatch '+row['path'])
(s/'wnf-system-epoch2-source-verification.json').write_text(json.dumps({'base':m['base'],'verified':len(whole['sources']),'sources':whole['sources']},indent=2)+'\n');print('Verified',len(whole['sources']),'actual source inputs')
