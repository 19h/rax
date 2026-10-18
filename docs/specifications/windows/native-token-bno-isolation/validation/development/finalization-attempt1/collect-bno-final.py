"""Collect exact final validation inputs after every driver has completed."""
from pathlib import Path
import hashlib
import json
import shutil

B = Path(__file__).resolve().parent
N = B.parent / 'native-base-named-objects-20261018'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = B / 'rax-source/docs/specifications/windows/native-token-bno-isolation'
V = D / 'validation'


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def copy(path, destination=V):
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(path, destination / path.name)


for name in ['bno-native-epoch5-launch.json', 'bno-final-owned-launch.json']:
    row = load(S / name)
    if row['state'] != 'finished' or row['exit-code'] != 0:
        raise RuntimeError('incomplete driver: ' + name)
if not (S / 'bno-final-post-run-completion.json').exists():
    raise RuntimeError('native final checks incomplete')
for host, own_count in [('macos', 7), ('linux', 4)]:
    for kind, count in [('epoch5', 4), ('owning', own_count)]:
        rows = load(B / (host + '-bno-' + kind + '-gates.json'))
        if len(rows) != count or any(row['exit-code'] for row in rows):
            raise RuntimeError('incomplete gates: ' + host + '/' + kind)
    for prefix in [host + '-bno-epoch5-', host + '-bno-owning-']:
        for path in B.glob(prefix + '*'):
            if path.is_file() and path.suffix in ['.log', '.json', '.jsonl']:
                copy(path)
for prefix in ['native-bno-epoch5-', 'native-bno-final-', 'native-bno-owning-',
               'bno-native-epoch5-', 'bno-final-', 'bno-owner-observer',
               'assist-native-bno-final-']:
    for path in S.glob(prefix + '*'):
        if path.is_file() and path.suffix in ['.log', '.json', '.jsonl']:
            copy(path)
for name in ['bno-epoch5-source.json', 'bno-epoch5-prepared.json', 'final-port.json',
             'bno-owning-prepared.json', 'bno-owning-source-hashes.json',
             'current-cpp-consumers-bno-root.json', 'owned-source-plan.json',
             'bno-ordinary-reviewed.json', 'prototype-macos-focus-epoch5.log',
             'prototype-linux-focus-epoch5.log', 'prototype-macos-focus-epoch5.json',
             'prototype-linux-focus-epoch5.json']:
    copy(B / name)
copy(N / 'base-final-main-state.json')
for name in ['collect-bno-final.py', 'prepare-bno-final-tools.py', 'port-bno-final.py',
             'prepare-bno-owning.py', 'run-bno-owning-gates.py', 'collect-bno-archives.py',
             'owning-linux-cpp-build.py', 'macos-bno-owning-commands.json',
             'linux-bno-owning-commands.json', 'run-bno-native-epoch5.py',
             'bno-native-epoch5-focus.ps1', 'prepare-bno-native-epoch5.py',
             'run-bno-epoch5-observations.py', 'run-bno-unix-epoch5.py',
             'verify-bno-final-source.py', 'verify-bno-post-run.py', 'finish-bno-final.py']:
    copy(B / name, V / 'drivers')
for path in (B / 'owning-windows').iterdir():
    if path.is_file():
        copy(path, V / 'windows')
for name in ['run-bno-owner-observer.py', 'bno-owner-observer-build.ps1']:
    copy(B / name, V / 'owner-diagnostic')
copy(S / 'bno-owner-observer.rs', V / 'owner-diagnostic')
rows = load(V / 'native-bno-epoch5-ordinary-outcomes.json')
(V / 'ordinary-analysis.json').write_text(json.dumps(rows, indent=2) + '\n')
files = []
for path in sorted(V.rglob('*')):
    if path.is_file() and path.name != 'inputs.json':
        raw = path.read_bytes()
        files.append({'path': str(path.relative_to(D)), 'bytes': len(raw),
                      'sha256': hashlib.sha256(raw).hexdigest()})
(V / 'inputs.json').write_text(json.dumps({'files': files}, indent=2) + '\n')
print('Collected', len(files), 'final BNO validation inputs')
