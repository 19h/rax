from pathlib import Path
import json,hashlib
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=json.loads((s/'wnf-query-epoch3-launch.json').read_text())
if r.get('state')!='finished' or r.get('exit-code')!=1:raise RuntimeError('retain the completed original driver failure')
m=json.loads((s/'wnf-query-epoch3-owning-source-hashes.json').read_text())
for row in m['sources']:
 p=b/'src'/row['path'];raw=p.read_bytes()
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('source changed '+row['path'])
print('Verified unchanged326 source inputs after the runner-only failure')
