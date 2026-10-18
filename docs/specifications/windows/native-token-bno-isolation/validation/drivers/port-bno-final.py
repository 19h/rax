"""Create the BNO change on the published namespace parent without mixing groups."""
from pathlib import Path
import datetime
import hashlib
import json
import shutil
import subprocess

B = Path(__file__).resolve().parent
N = B.parent / 'native-base-named-objects-20261018'
R = B / 'rax-source'
P = B / 'prototype-source'
if R.exists() or (B / 'final-port.json').exists():
    raise RuntimeError('final BNO port already exists')
published = json.loads((N / 'base-final-main-state.json').read_text())
parent = published['rax']
source = N / 'rax-source'
if subprocess.check_output(['git', 'ls-remote', 'origin', 'refs/heads/master'],
                           cwd=source, text=True).split()[0] != parent:
    raise RuntimeError('namespace parent is not the current published RAX head')
owned = json.loads((B / 'owned-source-plan.json').read_text())
for item in owned['baseline-owned-files']:
    raw = subprocess.check_output(['git', 'show', parent + ':' + item['path']], cwd=source)
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise RuntimeError('BNO baseline overlap: ' + item['path'])
subprocess.run(['git', 'worktree', 'add', '--detach', str(R), parent], cwd=source, check=True)
status = subprocess.check_output(['git', 'status', '--short', '--branch'], cwd=R, text=True)
for name in owned['paths']:
    target = R / name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(P / name, target)
relative = Path('docs/specifications/windows/native-token-bno-isolation')
shutil.copytree(P / relative, R / relative)
history = R / relative / 'development'
shutil.copytree(B / 'development', history, dirs_exist_ok=True)
shared = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
first = history / 'native-epoch1'
first.mkdir(exist_ok=True)
for pattern in ['bno-native-epoch1-*', 'bno-epoch1-source.json']:
    for path in shared.glob(pattern):
        if path.is_file():
            shutil.copyfile(path, first / path.name)
for name in ['bno-epoch5-source.json', 'bno-epoch5-prepared.json',
             'prototype-macos-focus-epoch5.json', 'prototype-linux-focus-epoch5.json']:
    shutil.copyfile(B / name, history / name)
manifest = json.loads((B / 'bno-epoch5-source.json').read_text())
for item in manifest['files']:
    raw = (R / item['path']).read_bytes()
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise RuntimeError('ported compiled input differs: ' + item['path'])
(B / 'final-port.json').write_text(json.dumps({
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'parent': parent,
    'baseline-status': status, 'compiled-inputs': len(manifest['files']),
    'owned-rust-paths': owned['paths'],
    'scope': 'All 5737 tested Rust/workspace/data inputs match; only owned BNO files were ported.'
}, indent=2) + '\n')
print('Ported BNO onto', parent, 'with', len(manifest['files']), 'matching compiled inputs')
