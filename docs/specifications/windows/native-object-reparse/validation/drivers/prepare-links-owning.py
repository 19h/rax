"""Prepare frozen Assist consumers while preserving the current-root distinction."""
from pathlib import Path
import datetime
import hashlib
import json
import os

M = Path(__file__).resolve().parent
P = M.parent / 'native-bno-isolation-20261018'
R = M / 'prototype-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
if (M / 'links-owning-prepared.json').exists():
    raise RuntimeError('link owning checks already prepared')
manifest = json.loads((M / 'links-source-epoch1.json').read_text())
for row in manifest['files']:
    raw = (R / row['path']).read_bytes()
    if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
        raise RuntimeError('link source changed ' + row['path'])
old = json.loads((P / 'bno-owning-source-hashes.json').read_text())
paths = {row['path'] for row in old['sources']}
paths.update(name for name in json.loads((M / 'links-owned-source-plan.json').read_text())['paths'] if name.endswith('.rs'))
paths.update('docs/specifications/windows/native-object-reparse/native-object-reparse-v2-' + p + '.log' for p in ['arm64','x64','x86','x86-laa'])
rows = []
for name in sorted(paths):
    raw = (R / name).read_bytes()
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
assert len(rows) == 434
old.update(sources=rows, epoch='native-object-reparse-epoch1', feature='Captured object-manager links',
           **{'source-parent-rax': 'ce29b4a4bca74a668d7f029e8a69da6a2e0340e7'})
cpp = json.loads((P / 'current-cpp-consumers-bno-root.json').read_text())
cpp['scope'] = 'Nine frozen compared-source files; current root is separately compared for eight RAX/consumer files. Static-text drift and RustABI184 versus259 remain explicit.'
cpp['published-parent'] = json.loads((P / 'bno-final-main-state.json').read_text())
for dest in [M,S]:
    (dest / 'links-owning-source-hashes.json').write_text(json.dumps(old,indent=2)+'\n')
    (dest / 'current-cpp-consumers-links-root.json').write_text(json.dumps(cpp,indent=2)+'\n')


def transform(text):
    return (text.replace(str(P),str(M)).replace(str(M/'rax-source'),str(R))
            .replace('src-bno-isolation-prototype','src-object-reparse-epoch1')
            .replace('native-bno-isolation-epoch5','native-object-reparse-epoch1')
            .replace('bno-','links-').replace('428 selected','434 selected')
            .replace('against current root','against compared source snapshot'))


for name in ['run-bno-owning-gates.py','collect-bno-archives.py','owning-linux-cpp-build.py',
             'macos-bno-owning-commands.json','linux-bno-owning-commands.json']:
    target = M / transform(name)
    target.write_text(transform((P / name).read_text()))
    if name == 'collect-bno-archives.py':
        (S / target.name).write_bytes(target.read_bytes())
W = M / 'owning-windows'
W.mkdir()
for path in (P / 'owning-windows').iterdir():
    if path.is_file():
        name = transform(path.name)
        text = transform(path.read_text())
        if path.name == 'prepare-bno-owning-junction.py':
            text = text.replace("old = b / 'src-base-named-objects'", "old = b / 'src-bno-isolation-prototype'")
        (W / name).write_text(text)
        (S / name).write_text(text)
link = M.parent / 'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not link.is_symlink() or link.resolve() != (P / 'rax-source').resolve():
    raise RuntimeError('unexpected frozen Unix source owner')
before = os.readlink(link)
link.unlink()
try:
    link.symlink_to(R)
except BaseException:
    link.symlink_to(before)
    raise
(M / 'links-owning-prepared.json').write_text(json.dumps({
    'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'sources':434,
    'source-epoch':1,'unix-link-before':before,'unix-link-after':str(R),
    'scope':'Frozen ABI184 carriers, with current-root source drift separated from their execution evidence.'},indent=2)+'\n')
print('Prepared434 source/fixture inputs for retained consumers')
