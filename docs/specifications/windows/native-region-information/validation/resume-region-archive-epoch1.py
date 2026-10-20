from pathlib import Path
import hashlib
import json
import os
import shutil
import tarfile

CACHE = Path(__file__).parent
PARENT = CACHE.parent / 'native-namespace-20261020'
ROOT = Path('/Users/int/.assist-region-20261020')
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
manifest_path = CACHE / 'region-source-epoch1.json'
manifest = json.loads(manifest_path.read_text())
previous = {row['path']: row for row in json.loads((PARENT / 'namespace-source-epoch1.json').read_text())['files']}
overrides = []
for row in manifest['files']:
    path = ROOT / row['path']
    raw = os.readlink(path).encode() if row['kind'] == 'symlink-text' else path.read_bytes()
    assert (len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256']), row['path']
    if row != previous.get(row['path']):
        overrides.append(row['path'])
archive = ROOT / 'target/region-source-epoch1-overrides.tar.gz'
archive.parent.mkdir(parents=True, exist_ok=True)
assert not archive.exists()
with tarfile.open(archive, 'w:gz') as output:
    for name in overrides:
        output.add(ROOT / name, arcname=name, recursive=False)
raw = archive.read_bytes()
metadata = {'archive': archive.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
            'parent-manifest': 'namespace-source-epoch1.json',
            'parent-manifest-sha256': hashlib.sha256((PARENT / 'namespace-source-epoch1.json').read_bytes()).hexdigest(),
            'source-files': len(manifest['files']), 'source-bytes': manifest['total-bytes'],
            'separate-overrides': overrides, 'deleted': []}
meta_path = CACHE / 'region-source-epoch1-archive.json'
assert not meta_path.exists()
meta_path.write_text(json.dumps(metadata, indent=2) + '\n')
for path in [manifest_path, meta_path, archive]:
    shutil.copy2(path, SHARE / path.name)
p = CACHE / 'region-ownership.json'
ownership = json.loads(p.read_text())
for name in overrides:
    if name not in ownership['owned-source-candidates']:
        assert name.endswith('/region_information_tests.rs')
        ownership['owned-source-candidates'].append(name)
ownership.update({'previous-baseline': ownership['base'], 'base': manifest['base-repository-commit'],
                  'owned-source-paths': overrides, 'published-parent-integration': 'region-namespace-parent-integration.json',
                  'source-freeze': manifest_path.name})
p.write_text(json.dumps(ownership, indent=2) + '\n')
(CACHE / 'region-freeze-archive-retry.json').write_text(json.dumps({
    'initial-stop': 'The ignored MRI target directory did not yet exist; the source manifest was already written.',
    'recovery': 'Rehash every frozen input, create only that ignored task-owned directory, package exact changed paths and complete transfer metadata.',
    'source-files-unchanged': True}, indent=2) + '\n')
print(len(manifest['files']), manifest['total-bytes'], len(overrides), len(raw), flush=True)
