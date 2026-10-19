from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess

s = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
b = Path('C:/Windows/Temp/assist-native-rax-20261010')
p = b / 'assist/vendor/rax'
old = b / 'src-mutant-epoch4'
new = b / 'src-wait-epoch3'
if not p.is_junction() or p.resolve() not in [old.resolve(), new.resolve()]:
    raise ValueError('unexpected owning RAX junction')
focus = (s / 'wait-native-epoch3b-focus.log').read_text()
if '16 passed; 0 failed;' not in focus:
    raise ValueError('native focused validation incomplete')
m = json.loads((s / 'wait-owning-source-hashes.json').read_text())
for row in m['sources']:
    data = (new / row['path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('owning source ' + row['path'])
before = os.readlink(p)
if p.resolve() != new.resolve():
    os.rmdir(p)
    try:
        subprocess.run(['cmd.exe', '/d', '/c', 'mklink', '/J', str(p), str(new)], check=True)
    except BaseException:
        subprocess.run(['cmd.exe', '/d', '/c', 'mklink', '/J', str(p), before], check=True)
        raise
if not p.is_junction() or p.resolve() != new.resolve():
    raise ValueError('new owning RAX junction')
(s / 'native-wait-owning-junction.json').write_text(json.dumps({
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'before': before, 'after': os.readlink(p), 'verified-sources': len(m['sources'])
}, indent=2) + '\n')
print('Retargeted verified task-owned RAX junction')
