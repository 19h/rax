from pathlib import Path
import json,hashlib
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');B=Path('C:/Windows/Temp/assist-native-rax-20261010/src')
r=json.loads((S/'image-extension-final-v2-launch.json').read_text())
if r.get('state')!='finished'or r.get('exit-code')!=0:raise RuntimeError('previous native validation still active or failed')
p=B/'src/user/windows/process/sched/tests/services_tests/image_extension_owner_leaf_tests.rs'
if hashlib.sha256(p.read_bytes()).hexdigest()!='476ba837f0d9e3cda7dcea588766e31877373616ade37c58998f2d7c864bdd5a':raise RuntimeError('native prior leaf changed')
p.write_bytes((S/'image-extension-epoch6-owner-leaf.rs').read_bytes())
j=json.loads((S/'image-extension-epoch6-source-inputs.json').read_text())
for row in j['files']:
 raw=(B/row['path']).read_bytes()
 if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('epoch6 source '+row['path'])
(S/'image-extension-epoch6-source-verification.json').write_text(json.dumps({'count':len(j['files']),'files':j['files'],'scope':'one Windows-only test changed; shipping code unchanged'},indent=2)+'\n');print('Verified epoch6',len(j['files']),'source inputs')
