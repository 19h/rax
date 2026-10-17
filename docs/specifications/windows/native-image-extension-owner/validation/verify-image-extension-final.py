from pathlib import Path
import hashlib,json,zipfile
b=Path('C:/Windows/Temp/assist-native-rax-20261010/src');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');t=s/'image-extension-epoch5-transfer'
m=json.loads((t/'source-inputs.json').read_text());changed=0
for row in m['files']:
 p=b/row['path'];raw=p.read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
(s/'image-extension-epoch5-source-verification.json').write_text(json.dumps({'base':m['base'],'verified':len(m['files']),'changed':changed,'sources':m['files']},indent=2)+'\n');print('Verified',len(m['files']),'source inputs; copied',changed)
