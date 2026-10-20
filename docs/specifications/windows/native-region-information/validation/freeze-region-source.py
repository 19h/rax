"""Freeze the compilation graph as exact bytes plus separate native overrides."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import tarfile

CACHE = Path(__file__).parent
PARENT = CACHE.parent / 'native-namespace-20261020'
ROOT = Path('/Users/int/.assist-region-20261020')
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
MANIFEST = CACHE / 'region-source-epoch1.json'
assert not MANIFEST.exists()


def load(path):
    return json.loads(path.read_text())


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def identity(path, kind):
    raw = os.readlink(path).encode() if kind == 'symlink-text' else path.read_bytes()
    return len(raw), hashlib.sha256(raw).hexdigest()


assert git('rev-parse', 'HEAD') == 'dbcea8afa4292b4f5afa620963fe6f81e9301cae'
assert not git('diff', '--cached', '--name-only')
parent = load(PARENT / 'namespace-source-epoch1.json')
previous = {row['path']: row for row in parent['files']}
paths = set(previous)
changed = git('diff', '--name-only').splitlines()
new = git('ls-files', '--others', '--exclude-standard').splitlines()
model = 'docs/specifications/windows/native-region-information/modelled-query-cases.json'
source_paths = sorted(name for name in changed + new if not name.startswith(
    'docs/specifications/windows/native-region-information/') or name == model)
ownership = load(CACHE / 'region-ownership.json')
# The first regression path belongs to this task; retain its earlier baseline receipt.
regression = 'src/user/windows/process/sched/tests/services_tests/region_information_tests.rs'
if regression not in ownership['owned-source-candidates']:
    ownership['owned-source-candidates'].append(regression)
assert set(source_paths) <= set(ownership['owned-source-candidates'])
paths.update(source_paths)
files = []
overrides = []
for name in sorted(paths):
    path = ROOT / name
    kind = 'symlink-text' if path.is_symlink() else 'file'
    size, sha = identity(path, kind)
    row = {'path': name, 'kind': kind, 'bytes': size, 'sha256': sha}
    files.append(row)
    if row != previous.get(name):
        overrides.append(name)
assert set(overrides) == set(source_paths), (overrides, source_paths)
value = {'source-epoch': 1, 'semantic-group': 'Allocation-wide region information and private-copy commitment',
         'base-repository-commit': git('rev-parse', 'HEAD'),
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'files': files, 'total-bytes': sum(row['bytes'] for row in files),
         'scope': 'Parent namespace compilation graph plus exact MRI source/test/model changes; derived proof is excluded. Windows uses the separately bound native Cargo configuration.'}
MANIFEST.write_text(json.dumps(value, indent=2) + '\n')
archive = ROOT / 'target/region-source-epoch1-overrides.tar.gz'
assert not archive.exists()
with tarfile.open(archive, 'w:gz') as output:
    for name in overrides:
        output.add(ROOT / name, arcname=name, recursive=False)
raw = archive.read_bytes()
metadata = {'archive': archive.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
            'parent-manifest': 'namespace-source-epoch1.json',
            'parent-manifest-sha256': hashlib.sha256((PARENT / 'namespace-source-epoch1.json').read_bytes()).hexdigest(),
            'source-files': len(files), 'source-bytes': value['total-bytes'],
            'separate-overrides': overrides, 'deleted': []}
(CACHE / 'region-source-epoch1-archive.json').write_text(json.dumps(metadata, indent=2) + '\n')
for path in [MANIFEST, CACHE / 'region-source-epoch1-archive.json', archive]:
    shutil.copy2(path, SHARE / path.name)
ownership.update({'previous-baseline': ownership['base'], 'base': value['base-repository-commit'],
                  'owned-source-paths': source_paths,
                  'published-parent-integration': 'region-namespace-parent-integration.json',
                  'source-freeze': MANIFEST.name})
(CACHE / 'region-ownership.json').write_text(json.dumps(ownership, indent=2) + '\n')
print(json.dumps({'files': len(files), 'bytes': value['total-bytes'], 'overrides': len(overrides),
                  'archive-bytes': len(raw), 'owned-source-paths': source_paths}, indent=2), flush=True)
