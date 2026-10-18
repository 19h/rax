"""Prepare retained-consumer checks only after the namespace group is published."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess

B = Path(__file__).resolve().parent
N = B.parent / 'native-base-named-objects-20261018'
R = B / 'rax-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
M = Path('/Users/int/hexrays/kvasir')
if (B / 'bno-owning-prepared.json').exists():
    raise RuntimeError('BNO owning checks already prepared')
published = json.loads((N / 'base-final-main-state.json').read_text())
manifest = json.loads((B / 'bno-epoch5-source.json').read_text())
for item in manifest['files']:
    raw = (R / item['path']).read_bytes()
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise RuntimeError('validated BNO source changed: ' + item['path'])
old = json.loads((N / 'base-owning-source-hashes.json').read_text())
paths = {r['path'] for r in old['sources']}
paths.update(json.loads((B / 'owned-source-plan.json').read_text())['paths'])
paths.update(['docs/specifications/windows/native-token-bno-isolation/cases.json',
              'docs/specifications/windows/native-token-bno-isolation/copy-cases.json'])
rows = []
for name in sorted(paths):
    raw = (R / name).read_bytes()
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
old.update(sources=rows, epoch='native-bno-isolation-epoch5',
           feature='Captured TokenBnoIsolation and exact query copies',
           **{'source-parent-rax': published['rax']})
for root in [B, S]:
    (root / 'bno-owning-source-hashes.json').write_text(json.dumps(old, indent=2) + '\n')

cpp = json.loads((N / 'current-cpp-consumers-base-root.json').read_text())
for item in cpp['files']:
    raw = subprocess.check_output(['git', 'show', published['root'] + ':' + item['path']], cwd=M)
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise RuntimeError('published C++ consumer changed: ' + item['path'])
header = subprocess.check_output(['git', 'show', published['root'] + ':rust/assist-rs/include/assist_rs.h'],
                                 cwd=M, text=True)
cpp['previously-verified-root-head'] = cpp['current-root-head']
cpp['current-root-head'] = published['root']
cpp['current-abi-minor'] = int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u', header)[1])
for root in [B, S]:
    (root / 'current-cpp-consumers-bno-root.json').write_text(json.dumps(cpp, indent=2) + '\n')


def transform(text):
    return (text.replace(str(N), str(B))
            .replace(str(B / 'rax-source'), str(R))
            .replace('src-base-named-objects', 'src-bno-isolation-prototype')
            .replace('base-final', 'bno-final').replace('base-owning', 'bno-owning')
            .replace('base-cpp', 'bno-cpp').replace('base-root', 'bno-root')
            .replace('collect-base-archives', 'collect-bno-archives')
            .replace('native-base-directories-final', 'native-bno-isolation-epoch5')
            .replace('419 selected', str(len(rows)) + ' selected'))


for name in ['run-base-owning-gates.py', 'collect-base-archives.py', 'owning-linux-cpp-build.py',
             'macos-base-owning-commands.json', 'linux-base-owning-commands.json']:
    target = B / transform(name)
    target.write_text(transform((N / name).read_text()))
    if name == 'collect-base-archives.py':
        (S / target.name).write_bytes(target.read_bytes())
W = B / 'owning-windows'
W.mkdir(exist_ok=True)
for name in ['assist-native-base-final-owned.ps1', 'run-base-final-owned.py',
             'verify-base-owning-sources.py', 'verify-base-cpp-consumers.py',
             'inspect-base-final-archives.py', 'prepare-base-owning-junction.py']:
    target = W / transform(name)
    text = transform((N / 'windows' / name).read_text())
    if name == 'assist-native-base-final-owned.ps1':
        text = text.replace("$env:CARGO_BUILD_JOBS='2'", "$env:CARGO_BUILD_JOBS='2';$env:CARGO_INCREMENTAL='0'")
    if name == 'prepare-base-owning-junction.py':
        text = text.replace("old = b / 'src-token-scalars'", "old = b / 'src-base-named-objects'")
    target.write_text(text)
    (S / target.name).write_text(text)

link = N.parent / 'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not link.is_symlink() or link.resolve() != (N / 'rax-source').resolve():
    raise RuntimeError('retained Unix RAX link has an unexpected owner')
before = os.readlink(link)
link.unlink()
try:
    link.symlink_to(R)
except BaseException:
    link.symlink_to(before)
    raise
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'namespace-publication': published, 'sources': len(rows),
          'source-epoch': 5, 'windows-validation-CARGO_INCREMENTAL': '0', 'unix-link-before': before, 'unix-link-after': str(R),
          'scope': 'Frozen ABI184 carriers; current C++ source equality is checked separately.'}
(B / 'bno-owning-prepared.json').write_text(json.dumps(record, indent=2) + '\n')
print('Prepared BNO retained-consumer checks:', len(rows), 'source/fixture inputs', flush=True)
