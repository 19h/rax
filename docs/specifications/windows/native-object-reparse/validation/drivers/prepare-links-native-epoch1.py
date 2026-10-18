from pathlib import Path
import hashlib
import json
import shutil
import tarfile

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
source = B / 'src-bno-isolation-prototype'
root = B / 'src-object-reparse-epoch1'
if root.exists():
    raise RuntimeError('native link source already exists')
previous = json.loads((S / 'bno-epoch5-source.json').read_text())
for row in previous['files']:
    raw = (source / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise RuntimeError('native parent source drift ' + row['path'])
shutil.copytree(source, root, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__'))
with tarfile.open(S / 'links-source-epoch1-overrides.tar.gz') as tar:
    if len(tar.getmembers()) != 10:
        raise RuntimeError('unexpected override count')
    tar.extractall(root, filter='data')
manifest = json.loads((S / 'links-source-epoch1.json').read_text())
for row in manifest['files']:
    raw = (root / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise RuntimeError('native current source drift ' + row['path'])
print('Verified5743 native object-manager link inputs; free bytes', shutil.disk_usage(root).free, flush=True)
