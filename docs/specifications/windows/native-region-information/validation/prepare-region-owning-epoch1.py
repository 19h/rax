from pathlib import Path
import hashlib
import json
import os
import shutil

CACHE = Path(__file__).parent
PREVIOUS = CACHE.parent / 'native-namespace-20261020'
ROOT = Path('/Users/int/.assist-region-20261020')
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
manifest_path = CACHE / 'region-owning-source-hashes-epoch1.json'
assert not manifest_path.exists()


def adapt(text):
    return text.replace('.assist-namespace-20261020', '.assist-region-20261020').replace(
        'native-namespace-20261020', 'native-region-20261020').replace('namespace-epoch1', 'region-epoch1').replace(
        'namespace-owning-source', 'region-owning-source').replace('namespace-owned-cpp', 'region-owned-cpp').replace(
        'collect-namespace-archives', 'collect-region-archives').replace('run-namespace-owning', 'run-region-owning').replace(
        'run-namespace-native-owning', 'run-region-native-owning').replace('namespace-native', 'region-native')


for name in ['collect-namespace-archives-epoch1.py', 'namespace-owned-cpp-build-epoch1.py',
             'owning-linux-cpp-build-namespace-epoch1.py', 'run-namespace-owning-epoch1.py',
             'run-namespace-native-owning-epoch1.py']:
    output_name = name.replace('namespace', 'region')
    text = adapt((PREVIOUS / name).read_text())
    if name == 'run-namespace-native-owning-epoch1.py':
        text = text.replace("(B/'src-continue-epoch3').resolve()", "(B/'src-namespace-epoch1').resolve()")
        text = text.replace('Current directory-diagnostic source;', 'Frozen region-query source;')
    path = CACHE / output_name
    assert not path.exists()
    path.write_text(text)
    if name in ['collect-namespace-archives-epoch1.py', 'namespace-owned-cpp-build-epoch1.py',
                'run-namespace-native-owning-epoch1.py']:
        shutil.copy2(path, SHARE / output_name)
for host in ['macos', 'linux']:
    name = host + '-namespace-epoch1-owning-commands.json'
    (CACHE / name.replace('namespace', 'region')).write_text(adapt((PREVIOUS / name).read_text()))
manifest = json.loads((PREVIOUS / 'namespace-owning-source-hashes-epoch1.json').read_text())
paths = {row['path'] for row in manifest['sources']}
frozen = json.loads((CACHE / 'region-source-epoch1.json').read_text())
previous = {row['path'] for row in json.loads((PREVIOUS / 'namespace-source-epoch1.json').read_text())['files']}
new_paths = {row['path'] for row in frozen['files'] if row['path'] not in previous}
paths.update(new_paths)
rows = []
for name in sorted(paths):
    raw = (ROOT / name).read_bytes()
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest['sources'] = rows
manifest.update({'feature': 'Allocation-wide region query and private-copy commitment', 'epoch': 1,
                 'source-parent-rax': 'dbcea8afa4292b4f5afa620963fe6f81e9301cae',
                 'scope': 'Current frozen RAX compiled-source superset and retained Assist ABI184 linked consumers; complete current-root products remain separate.'})
for row in manifest['helpers']:
    if row['path'] == 'namespace-owned-cpp-build-epoch1.py':
        row['path'] = 'region-owned-cpp-build-epoch1.py'
        row['native-path'] = row['native-path'].replace('namespace', 'region')
        raw = (CACHE / row['path']).read_bytes()
        row.update({'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
shutil.copy2(manifest_path, SHARE / manifest_path.name)
retained = CACHE.parent / 'wait-completion-packet-create-20261012/assist-root-current'
link = retained / 'vendor/rax'
assert link.is_symlink() and link.resolve() == Path('/Users/int/.assist-namespace-20261020')
before = os.readlink(link)
link.unlink()
try:
    link.symlink_to(ROOT, target_is_directory=True)
except BaseException:
    link.symlink_to(before, target_is_directory=True)
    raise
(CACHE / 'region-macos-owning-link.json').write_text(json.dumps({
    'path': str(link), 'before': before, 'after': str(ROOT),
    'scope': 'Only the isolated retained-consumer dependency link changes; no user profile or current root checkout change.'}, indent=2) + '\n')
print('owning source inputs', len(rows), 'new', len(new_paths), 'runtime doc included',
      'src/user/windows/native-runtime.md' in paths, flush=True)
