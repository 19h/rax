from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import tarfile
import traceback


B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
OLD = B / 'src-semaphore-epoch2'
R = B / 'verify-semaphore-epoch2'
RECORD = S / 'semaphore-final-check-windows.json'
assert not RECORD.exists() and not R.exists(), 'Inspect preceding final check; do not duplicate.'
meta = json.loads((S / 'semaphore-final-proof-archive.json').read_text())
manifest = json.loads((S / 'semaphore-source-epoch2.json').read_text())
archive = S / meta['archive']
assert archive.stat().st_size == meta['bytes'] and hashlib.sha256(archive.read_bytes()).hexdigest() == meta['sha256']
R.mkdir()
state = {'state': 'materializing', 'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'scope': 'Read-only evidence check in a new source tree; every overwritten proof file is detached from its parent hardlink.'}
RECORD.write_text(json.dumps(state, indent=2) + '\n')
try:
    for row in manifest['files']:
        source, target = OLD / row['path'], R / row['path']
        raw = source.read_bytes()
        expected = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c' if row['path'] == '.cargo/config.toml' else row['sha256']
        assert hashlib.sha256(raw).hexdigest() == expected, row['path']
        target.parent.mkdir(parents=True, exist_ok=True)
        os.link(source, target)
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        assert all(m.isfile() and m.name.startswith('docs/specifications/windows/native-semaphore/') and
                   '..' not in Path(m.name).parts for m in members)
        for member in members:
            target = R / member.name
            if target.exists():
                target.unlink()
        tar.extractall(R, filter='data')
    state.update({'state': 'checking', 'complete-inputs': len(manifest['files']), 'proof-files': len(members)})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    log = S / 'semaphore-final-check-windows.log'
    with log.open('wb') as output:
        code = subprocess.run([str(B / 'python/python.exe'), '-B',
                               str(R / 'docs/specifications/windows/native-semaphore/check_validation.py')],
                              cwd=R, stdout=output, stderr=subprocess.STDOUT).returncode
    state.update({'state': 'finished' if code == 0 else 'failed', 'exit-code': code,
                  'log': log.name, 'sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
RECORD.write_text(json.dumps(state, indent=2) + '\n')
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
