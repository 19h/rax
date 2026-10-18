from pathlib import Path
import hashlib
import json
import shutil

M = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = M / 'prototype-source/docs/specifications/windows/native-object-reparse'
V = D / 'validation'


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def copy(path, destination=V):
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(path, destination / path.name)


for name in ['links-native-full-epoch1.json','links-final-owned-launch.json']:
    row = load(S / name)
    if row['state'] != 'finished' or row['exit-code'] != 0:
        raise RuntimeError('native gate incomplete: ' + name)
owner = load(S / 'links-owner-observer-launch.json')
if owner['state'] != 'finished' or owner['exit_code']:
    raise RuntimeError('ordinary observer incomplete')
for host, count in [('macos',7),('linux',4)]:
    for name, expected in [('links-'+host+'-epoch1-gates.json',4),
                           (host+'-links-owning-gates.json',count)]:
        rows = load(M / name)
        if len(rows) != expected or any(r['exit-code'] for r in rows):
            raise RuntimeError('Unix gate incomplete: '+name)
    for pattern in ['links-'+host+'-epoch1-*',host+'-links-owning-*',host+'-links-final-*',
                    'links-'+host+'-focus-epoch1.*']:
        for path in M.glob(pattern):
            if path.is_file() and path.suffix in ['.log','.json','.jsonl']:
                copy(path)
for pattern in ['links-native-*','links-final-*','links-owner-*','native-links-*','assist-native-links-*']:
    for path in S.glob(pattern):
        if path.is_file() and path.suffix in ['.log','.json','.jsonl']:
            copy(path)
for name in ['links-source-epoch1.json','links-owned-source-plan.json','links-native-stress-epoch1.json',
             'links-owning-source-hashes.json','current-cpp-consumers-links-root.json',
             'links-owning-prepared.json','links-upstream-source-comparison.json',
             'links-native-full-epoch1-reviewed.json']:
    copy(M / name)
copy(S / 'links-owner-cache-cleanup.json')
for name in ['prepare-links-native-epoch1.py','run-links-native-epoch1.py','links-native-focus-epoch1.ps1',
             'run-links-native-full-epoch1.py','links-native-full-epoch1.ps1','run-links-focus-epoch1.py',
             'run-links-unix-epoch1.py','prepare-links-owning.py','run-links-owning-gates.py',
             'collect-links-archives.py','owning-linux-cpp-build.py','macos-links-owning-commands.json',
             'linux-links-owning-commands.json','collect-links-final.py']:
    copy(M / name,V/'drivers')
for path in (M/'owning-windows').iterdir():
    if path.is_file():copy(path,V/'windows')
for name in ['run-links-owner-observer.py','links-owner-observer-build.ps1']:
    copy(M/name,V/'owner-diagnostic')
copy(S/'links-owner-observer.rs',V/'owner-diagnostic')
rows=[]
for path in sorted(V.rglob('*')):
    if path.is_file() and path.name!='inputs.json':
        raw=path.read_bytes();rows.append({'path':str(path.relative_to(D)),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(V/'inputs.json').write_text(json.dumps({'files':rows},indent=2)+'\n')
print('Collected',len(rows),'final link verification inputs')
