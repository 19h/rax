from pathlib import Path
import hashlib
import json
import shutil
import tarfile

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = B / 'src-bno-isolation-prototype'
saved = B / 'bno-prototype-epoch1-retained'
if saved.exists():
    raise RuntimeError('native source revision already attempted')
old = json.loads((S / 'bno-epoch1-source.json').read_text())
for row in old['files']:
    raw = (R / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise RuntimeError('old native prototype differs: ' + row['path'])
archive = S / 'bno-epoch5-overrides.tar.gz'
if hashlib.sha256(archive.read_bytes()).hexdigest() != '723e748c51a31db6d15d61712150b2d50a2bdaddf6d67b9e3815ea1030e79a8c':
    raise RuntimeError('new source archive changed')
with tarfile.open(archive, 'r:gz') as files:
    members = files.getmembers()
    if len(members) != 19:
        raise RuntimeError('source override count changed')
    for member in members:
        if not member.isfile() or member.name.startswith('/') or '..' in Path(member.name).parts:
            raise RuntimeError('invalid source member')
    saved.mkdir()
    shutil.copyfile(S / 'bno-epoch1-source.json', saved / 'source.json')
    for member in members:
        p = R / member.name
        if p.exists():
            target = saved / member.name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, target)
    files.extractall(R, filter='data')
manifest = json.loads((S / 'bno-epoch5-source.json').read_text())
for row in manifest['files']:
    raw = (R / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise RuntimeError('revised native prototype differs: ' + row['path'])
if hashlib.sha256((R / '.cargo/config.toml').read_bytes()).hexdigest() != '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':
    raise RuntimeError('native Cargo configuration changed')
print('Prepared and verified BNO epoch5 native inputs:', len(manifest['files']), flush=True)
