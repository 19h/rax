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
OLD = B / 'src-registry-epoch1'
R = B / 'verify-registry-epoch1'
D = R / 'docs/specifications/windows/native-state-separation-registry'
RECORD = S / 'registry-final-check-windows.json'
assert not RECORD.exists() and not R.exists(), 'Inspect an earlier attempt; do not duplicate it.'
meta = json.loads((S / 'registry-final-proof-archive.json').read_text())
manifest = json.loads((S / 'registry-source-epoch1.json').read_text())
archive = S / meta['archive']
assert archive.stat().st_size == meta['bytes']
assert hashlib.sha256(archive.read_bytes()).hexdigest() == meta['sha256']
R.mkdir()
state = {'state': 'materializing', 'pid': os.getpid(),
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'steps': [],
         'checked-envelope-files': meta['bound-evidence-files'],
         'scope': 'New verification tree; all extracted proof/runtime documents detach from immutable parent hardlinks.'}

def save():
    RECORD.write_text(json.dumps(state, indent=2) + '\n')

save()
try:
    for row in manifest['files']:
        source, target = OLD / row['path'], R / row['path']
        raw = source.read_bytes()
        expected = ('9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
                    if row['path'] == '.cargo/config.toml' else row['sha256'])
        assert hashlib.sha256(raw).hexdigest() == expected, row['path']
        target.parent.mkdir(parents=True, exist_ok=True)
        os.link(source, target)
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        assert len(members) == meta['members']
        for member in members:
            name = Path(member.name)
            assert member.isfile() and not name.is_absolute() and '..' not in name.parts
            assert (member.name.startswith('docs/specifications/windows/native-state-separation-registry/')
                    or member.name == 'src/user/windows/native-runtime.md')
            target = R / member.name
            if target.exists():
                target.unlink()
        tar.extractall(R, filter='data')
    state.update(state='checking', **{'complete-inputs': len(manifest['files'])})
    save()
    log = S / 'registry-final-check-windows.log'
    with log.open('wb') as output:
        commands = [('native', [str(B / 'python/python.exe'), '-B', str(D / 'check_native.py'), str(D)]),
                    ('validation', [str(B / 'python/python.exe'), '-B', str(D / 'check_validation.py'), str(R)])]
        for name, argv in commands:
            code = subprocess.run(argv, cwd=R, stdout=output, stderr=subprocess.STDOUT).returncode
            state['steps'].append({'name': name, 'argv': argv, 'exit-code': code})
            save()
            assert code == 0, name
    raw = log.read_bytes()
    state.update(state='finished', **{'exit-code': 0, 'log': log.name,
                                     'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
except BaseException:
    state.update(state='failed', **{'exit-code': 1, 'error': traceback.format_exc()})
state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
save()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
