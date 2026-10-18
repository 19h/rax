"""Require semantic rejection after altered copy-proof identities are rebound."""
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
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def edit_json(root, name, mutate):
    path = root / name
    value = load(path)
    mutate(value)
    path.write_text(json.dumps(value, separators=(',', ':')) + '\n')


def replace(root, name, before, after):
    path = root / name
    text = path.read_text()
    if before not in text:
        raise RuntimeError('missing corruption target')
    path.write_text(text.replace(before, after, 1))


def wrong_machine(root):
    for suffix in ['before', 'after']:
        edit_json(root, 'copy-native/bno-copy-arm64-' + suffix + '.json',
                  lambda v: v.update(machine=332))


def invented_result(root):
    replace(root, 'copy-native/bno-copy-arm64-ascii.log',
            'status=C0000005 returned=A5A5A5A5', 'status=00000000 returned=A5A5A5A5')
    edit_json(root, 'copy-cases.json',
              lambda v: v['runs'][0]['cases'][0].update(status='00000000'))


cases = [
    ('native-copy-source', lambda r: replace(r, 'copy-native/native-bno-copy-boundaries.cpp',
                                           '#include', '// changed\n#include')),
    ('native-copy-machine', wrong_machine),
    ('missing-copy-profile', lambda r: edit_json(r, 'copy-native/bno-copy-outcomes.json',
                                               lambda v: v.pop())),
    ('copy-handle-leak', lambda r: replace(r, 'copy-native/bno-copy-arm64-ascii.log',
                                        'closed-handles=1', 'closed-handles=0')),
    ('missing-copy-row', lambda r: edit_json(r, 'copy-cases.json',
                                          lambda v: v['runs'][0]['cases'].pop())),
    ('invented-copy-success-raw-and-corpus', invented_result),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='bno-copy-proof-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutate(root)
        path = root / 'native-inputs.json'
        manifest = load(path)
        for row in manifest['files']:
            raw = (root / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
        path.write_text(json.dumps(manifest) + '\n')
        result = subprocess.run([sys.executable, '-B', str(root / 'check-copy-native.py')],
                                capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted altered copy proof: ' + name)
        results.append({'case': name, 'rejected': True,
                        'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
