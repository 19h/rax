from pathlib import Path
import hashlib
import json

s = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r = Path('C:/Windows/Temp/assist-native-rax-20261010/src-dynamic-table')
m = json.loads((s / 'dynamic-epoch3-source-inputs.json').read_text())
for row in m['files']:
    raw = (r / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise ValueError('native final source mismatch ' + row['path'])
print('Verified final native dynamic source inputs:', len(m['files']))
