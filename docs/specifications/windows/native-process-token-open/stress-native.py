"""Reject inconsistent native controls even after their digests are rebound."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent


def load(p):
    raw = p.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def save(p, value):
    p.write_text(json.dumps(value, indent=2) + '\n')


def producer(root, mutate):
    p = root / 'native/producer-identities.json'
    value = load(p)
    mutate(value)
    save(p, value)


def change(root, kind, mutate):
    selected = next(p for p in load(root / 'native/producer-identities.json')['producers']
                    if p['kind'] == kind and p['profile'] == 'arm64')
    path = root / 'native' / selected['log']['path']
    before = path.read_text()
    after = mutate(before)
    if before == after:
        raise ValueError('ineffective mutation ' + kind)
    path.write_text(after)


def bind(root):
    path = root / 'native/producer-identities.json'
    value = load(path)
    for p in value['producers']:
        row = p['log']
        raw = (root / 'native' / row['path']).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    save(path, value)
    path = root / 'native-inputs.json'
    value = load(path)
    for row in value['files']:
        raw = (root / row['path']).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    save(path, value)


cases = [
    ('missing-profile', lambda r: producer(r, lambda v: v['producers'].pop())),
    ('wrong-machine', lambda r: producer(r, lambda v: v['producers'][0]['exe'].update(machine=0x14c))),
    ('false-birth', lambda r: change(r, 'initial', lambda t: t.replace('births=1', 'births=0', 1))),
    ('wrong-grant', lambda r: change(r, 'access', lambda t: t.replace('grants=00000008,', 'grants=00000009,', 1))),
    ('wrong-status', lambda r: change(r, 'access', lambda t: t.replace('status=C0000022', 'status=C0000061', 1))),
    ('missing-row', lambda r: change(r, 'initial', lambda t: '\n'.join(
        line for line in t.splitlines() if not line.startswith('case ordinal=0 ')) + '\n')),
    ('wrong-lifetime', lambda r: change(r, 'lifetime', lambda t: t.replace('count=3 ', 'count=999 ', 1))),
    ('wrong-last-error', lambda r: change(r, 'initial', lambda t: t.replace('last-error=4A2E19F7', 'last-error=00000000', 1))),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='process-token-native-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutate(root)
        bind(root)
        p = subprocess.run([sys.executable, '-B', str(root / 'check-native.py')], capture_output=True, text=True)
        if p.returncode == 0:
            raise RuntimeError('accepted altered proof ' + name)
        results.append({'case': name, 'rejected': True, 'error': p.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
