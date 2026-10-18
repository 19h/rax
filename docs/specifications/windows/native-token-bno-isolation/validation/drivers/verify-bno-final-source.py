from pathlib import Path
import hashlib, json
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=Path('C:/Windows/Temp/assist-native-rax-20261010/src-bno-isolation-prototype')
m=json.loads((s/'bno-epoch5-source.json').read_text())
for row in m['files']:
 raw=(r/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:
  raise ValueError('native final source mismatch '+row['path'])
print('Verified final native BNO source inputs:',len(m['files']))
