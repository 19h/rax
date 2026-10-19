from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil

N = Path(__file__).resolve().parent
M = N.parent / 'native-mutant-create-20261018'
R = N / 'wait-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
F = N.parent / 'wait-completion-packet-create-20261012/assist-root-current'
assert not (N / 'wait-owning-prepared.json').exists()
old = json.loads((M / 'mutant-owning-source-hashes.json').read_text())
paths = {row['path'] for row in old['sources']}
paths.update(p for p in json.loads((N / 'wait-owned-source-plan.json').read_text())['paths'] if p.endswith('.rs'))
paths.update(p.relative_to(R).as_posix() for p in (R / 'docs/specifications/windows/native-single-wait/replay').glob('*.tsv'))
rows = []
for name in sorted(paths):
    raw = (R / name).read_bytes()
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest = dict(old, sources=rows, feature='Native single wait and measured alertable APC semantics', epoch='native-wait-epoch3',
    **{'source-parent-rax': '889bc914e298cb58d8ab0b90c9181ef75083f97c',
       'scope': 'Selected affected runtime/CAPI/fixture inputs, not the complete compilation closure. The complete22059-file source snapshot is bound separately. Retained frozen Assist RustABI184 carrier.'})
manifest.pop('observer-sha256', None)
for root in [N, S]:
    (root / 'wait-owning-source-hashes.json').write_text(json.dumps(manifest, indent=2) + '\n')
collector = (M / 'collect-mutant-archives.py').read_text().replace('native-mutant-epoch4', 'native-wait-epoch3')
for root in [N, S]:
    (root / 'collect-wait-archives.py').write_text(collector)
shutil.copy2(M / 'owning-linux-cpp-build.py', N / 'owning-linux-cpp-build.py')
for host in ['macos', 'linux']:
    rows = json.loads((M / (host + '-mutant-owning-commands.json')).read_text())
    serialized = json.dumps(rows).replace(str(M / 'mutant-source'), str(R)).replace(str(M), str(N)).replace('mutant', 'wait')
    (N / (host + '-wait-owning-commands.json')).write_text(json.dumps(json.loads(serialized), indent=2) + '\n')
runner = (M / 'run-mutant-owning-gates.py').read_text().replace('mutant', 'wait')
(N / 'run-wait-owning-gates.py').write_text(runner)
link = F / 'vendor/rax'
before = os.readlink(link)
assert before == str(M / 'mutant-source')
focus = (N / 'wait-macos-epoch3b-focus.log').read_text()
assert '15 passed; 0 failed;' in focus
link.unlink()
try:
    link.symlink_to(R, target_is_directory=True)
except BaseException:
    link.symlink_to(before, target_is_directory=True)
    raise
assert link.resolve() == R.resolve()
(N / 'wait-owning-prepared.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'sources':len(manifest['sources']), 'unix-link-before':before, 'unix-link-after':os.readlink(link),
    'scope':manifest['scope']}, indent=2)+'\n')
print('Prepared', len(manifest['sources']), 'selected affected inputs and retargeted task-owned Unix carrier')
