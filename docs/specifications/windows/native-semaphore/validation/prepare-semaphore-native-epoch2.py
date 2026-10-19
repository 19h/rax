from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import tarfile


B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
OLD = B / 'src-semaphore-epoch1'
R = B / 'src-semaphore-epoch2'
CONFIG = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
manifest = json.loads((S / 'semaphore-source-epoch2.json').read_text())
meta = json.loads((S / 'semaphore-source-epoch2-archive.json').read_text())
parent_raw = (S / 'semaphore-source-epoch1.json').read_bytes()
assert hashlib.sha256(parent_raw).hexdigest() == meta['parent-manifest-sha256']
parent = json.loads(parent_raw)
assert not R.exists(), 'Inspect preceding semaphore materialization; do not repeat it.'
prior = json.loads((S / 'semaphore-native-epoch1-gates.json').read_text())
assert prior['state'] == 'finished' and prior['exit-code'] == 0
for row in parent['files']:
    raw = (OLD / row['path']).read_bytes()
    expected = CONFIG if row['path'] == '.cargo/config.toml' else row['sha256']
    assert hashlib.sha256(raw).hexdigest() == expected, row['path']
archive = S / meta['archive']
assert archive.stat().st_size == meta['bytes'] and hashlib.sha256(archive.read_bytes()).hexdigest() == meta['sha256']
changed = set(meta['separate-overrides'])
R.mkdir()
linked, copied = 0, []
for row in manifest['files']:
    if row['path'] in changed:
        continue
    source, target = OLD / row['path'], R / row['path']
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        os.link(source, target)
        linked += 1
    except OSError:
        shutil.copy2(source, target)
        copied.append(row['path'])
with tarfile.open(archive, 'r:gz') as tar:
    members = tar.getmembers()
    assert {m.name for m in members} == changed and len(members) == len(changed)
    assert all(m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts for m in members)
    assert '.cargo/config.toml' not in changed
    tar.extractall(R, filter='data')
for name in changed:
    os.utime(R / name, None)
for row in manifest['files']:
    raw = (R / row['path']).read_bytes()
    expected = CONFIG if row['path'] == '.cargo/config.toml' else row['sha256']
    assert hashlib.sha256(raw).hexdigest() == expected, row['path']
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'source-epoch': 2, 'complete-inputs': len(manifest['files']),
          'immutable-linked-inputs': linked, 'fallback-copies': copied,
          'separate-overrides': sorted(changed), 'deleted': meta['deleted'],
          'scope': 'Every parent input verified before reuse; changed files materialized separately with current mtimes. No parent hardlinked content is written.'}
(S / 'semaphore-native-epoch2-preparation.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
