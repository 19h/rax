"""Require semantic failures after native-input digests are rebound."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def mutate_json(root, name, action):
    path = root / name
    value = load(path)
    action(value)
    path.write_text(json.dumps(value, indent=2) + '\n')


def mutate_text(root, name, before, after):
    path = root / name
    text = path.read_text()
    if before not in text:
        raise ValueError('missing stress case')
    path.write_text(text.replace(before, after, 1))


cases = [
    ('source-changed', lambda r: (r / 'native-object-reparse-v2.cpp').write_text('// changed producer\n')),
    ('invented-64th-success', lambda r: mutate_text(r, 'native-object-reparse-v2-arm64.log',
        'links=64 flags=00000000 status=C0000034', 'links=64 flags=00000000 status=00000000')),
    ('lost-handle-close', lambda r: mutate_text(r, 'native-object-reparse-v2-arm64.log',
        'opened=130 closed=130', 'opened=130 closed=129')),
    ('wrong-executable-machine', lambda r: (
        mutate_json(r, 'native-object-reparse-v2-arm64-before.json', lambda v: v.update(machine=0x8664)),
        mutate_json(r, 'native-object-reparse-v2-arm64-after.json', lambda v: v.update(machine=0x8664)))),
    ('missing-profile', lambda r: mutate_json(r, 'native-object-reparse-v2-outcomes.json', lambda v: v.pop())),
    ('invented-target-summary', lambda r: mutate_json(r, 'native-targets/observations.json',
        lambda v: v[0]['links'][0].update(target='\\Other'))),
    ('unclaimed-session-principal', lambda r: mutate_text(r, 'native-targets/native-base-links-arm64.log',
        'process-session=0', 'process-session=1')),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='object-reparse-native-stress-') as temporary:
        root = Path(temporary) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        manifest = load(root / 'native-inputs.json')
        for row in manifest['files']:
            raw = (root / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
        (root / 'native-inputs.json').write_text(json.dumps(manifest, indent=2) + '\n')
        result = subprocess.run([sys.executable, '-B', str(root / 'check-native.py')], capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted rebound corruption: ' + name)
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
