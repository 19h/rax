from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess

s = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
b = Path('C:/Windows/Temp/assist-native-rax-20261010')
p = b / 'assist/vendor/rax'
old = b / 'src-system-time'
new = b / 'src-dynamic-table'
if not p.is_junction() or p.resolve() != old.resolve():
    raise ValueError('unexpected owning RAX junction')
m = json.loads((s / 'dynamic-owning-source-hashes.json').read_text())
for row in m['sources']:
    data = (new / row['path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('owning source ' + row['path'])
before = os.readlink(p)
os.rmdir(p)
try:
    subprocess.run(['cmd.exe', '/d', '/c', 'mklink', '/J', str(p), str(new)], check=True)
except BaseException:
    subprocess.run(['cmd.exe', '/d', '/c', 'mklink', '/J', str(p), str(old)], check=True)
    raise
if not p.is_junction() or p.resolve() != new.resolve():
    raise ValueError('new owning RAX junction')
(s / 'native-dynamic-owning-junction.json').write_text(json.dumps({
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'before': before, 'after': os.readlink(p), 'verified-sources': len(m['sources'])
}, indent=2) + '\n')
print('Retargeted verified task-owned RAX junction')
