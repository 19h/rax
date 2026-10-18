"""Collect completed mutant evidence; preserve failed source/preparation epochs."""
from pathlib import Path
import hashlib
import json
import shutil

M = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = M / 'mutant-source/docs/specifications/windows/native-mutant'
V = D / 'validation'


def load(p):
    raw = p.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def copy(p, folder=V):
    folder.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(p, folder / p.name)


for host, root, filename in [
    ('macos', M, 'mutant-macos-epoch4-gates.json'),
    ('linux', M, 'mutant-linux-epoch4-gates.json'),
    ('native', S, 'mutant-native-epoch4c-gates.json'),
]:
    state = load(root / filename)
    if state['state'] != 'finished' or state['exit-code'] != 0:
        raise RuntimeError('Incomplete final core driver: ' + host)
for name in ['mutant-final-owned-launch.json', 'mutant-owner-observer-launch.json']:
    state = load(S / name)
    if state['state'] != 'finished' or state.get('exit-code', state.get('exit_code')) != 0:
        raise RuntimeError('Incomplete Windows owner driver: ' + name)
for host, count in [('macos', 7), ('linux', 4)]:
    rows = load(M / (host + '-mutant-owning-gates.json'))
    if len(rows) != count or any(r['exit-code'] for r in rows):
        raise RuntimeError('Incomplete retained owner gates: ' + host)

for root, patterns in [(M, ['mutant-*-epoch*-*', 'mutant-source-epoch*.json',
                           'mutant-focus-epoch*.*',
                           'mutant-baseline-*', '*-mutant-owning-*', '*-mutant-final-*',
                           '*-mutant-refresh-*', 'mutant-extra-cargo-metadata.json',
                           'mutant-owned-source-plan.json', 'mutant-owning-source-hashes.json',
                           'mutant-owning-prepared.json', 'current-cpp-consumers-mutant-root.json',
                           'mutant-upstream-source-comparison.json', 'mutant-root-base.json',
                           'mutant-owner-observer-source.json', 'native-names-v*-reviewed.json', 'mutant-native-stress.*',
                           'mutant-native-preparation-lineage.json']),
                       (S, ['mutant-native-*', 'mutant-final-*', 'native-mutant-final-*',
                            'assist-native-mutant-final-*', 'mutant-owner-*', 'mutant-ordinary-reviewed.json'])]:
    for pattern in patterns:
        for p in root.glob(pattern):
            if p.is_file() and p.suffix in ['.log', '.json', '.jsonl', '.toml']:
                copy(p)

shutil.copytree(S / 'mutant-native-restored-documents', V / 'native-restored-documents', dirs_exist_ok=True)

drivers = ['collect-mutant-final.py', 'run-mutant-unix-epoch4.py',
           'run-mutant-native-epoch3.py', 'run-mutant-native-epoch3b.py',
           'run-mutant-native-epoch4.py', 'run-mutant-native-epoch4b.py',
           'run-mutant-native-epoch4c.py', 'collect-mutant-archives.py',
           'run-mutant-owning-gates.py', 'prepare-mutant-owning.py',
           'owning-linux-cpp-build.py', 'macos-mutant-owning-commands.json',
           'linux-mutant-owning-commands.json', 'prepare-mutant-root.py']
for name in drivers:
    p = M / name
    if p.exists():
        copy(p, V / 'drivers')
for p in (M / 'owning-windows-mutant').iterdir():
    if p.is_file() and p.suffix in ['.py', '.ps1']:
        copy(p, V / 'windows')
for name in ['run-mutant-owner-observer.py', 'mutant-owner-observer-build.ps1', 'mutant-owner-observer.rs']:
    copy(M / name, V / 'owner-diagnostic')
copy(S / 'collect-mutant-native-restored-documents.py', V / 'drivers')

rows = []
for p in sorted(V.rglob('*')):
    if p.is_file() and p.name != 'inputs.json':
        raw = p.read_bytes()
        rows.append({'path': str(p.relative_to(D)), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
(V / 'inputs.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('Collected', len(rows), 'mutant validation files')
