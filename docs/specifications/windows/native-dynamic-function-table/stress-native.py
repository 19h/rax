"""Reject incomplete or semantically changed native evidence after digest rebinding."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def parse(raw):
    result = []
    for line in raw.decode('utf-8-sig').splitlines():
        fields = dict(token.split('=', 1) for token in line.split() if '=' in token)
        if fields:
            result.append({'record': line.split()[0].split('=')[0], **fields})
    return result


def edit_producer(root, mutate):
    path = root / 'native/producer-identities.json'
    value = load(path)
    mutate(value)
    save(path, value)


def edit_log(root, kind, mutate):
    producer = next(p for p in load(root / 'native/producer-identities.json')['identities']
                    if p['kind'] == kind and p['profile'] == 'arm64')
    path = root / 'native' / producer['logs'][0]['path']
    before = path.read_text(encoding='utf-8-sig')
    after = mutate(before)
    if after == before:
        raise ValueError('stress mutation had no effect: ' + kind)
    path.write_text(after)


def bind(root):
    path = root / 'native/producer-identities.json'
    value = load(path)
    executions = []
    for producer in value['identities']:
        for row in producer['logs']:
            raw = (root / 'native' / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
            executions.append({'kind': producer['kind'], 'profile': producer['profile'],
                               'log': row['path'], 'records': parse(raw),
                               'contract': producer['kind'] != 'ranges'})
    save(path, value)
    corpus = load(root / 'native/corpus.json')
    corpus['executions'] = executions
    save(root / 'native/corpus.json', corpus)
    for name in ['native-inputs.json', 'primary/sources.json']:
        value = load(root / name)
        for row in value['files']:
            raw = (root / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
        save(root / name, value)


cases = [
    ('missing-profile', lambda r: edit_producer(r, lambda v: v['identities'].pop())),
    ('failed-producer', lambda r: edit_producer(r, lambda v:
        v['identities'][0]['logs'][0].update({'exit-code': 1}))),
    ('wrong-machine', lambda r: edit_producer(r, lambda v:
        v['identities'][0].update({'machine': 0x14c}))),
    ('wrong-fault-status', lambda r: edit_log(r, 'boundary', lambda t:
        t.replace('status=00000000', 'status=C0000005', 1))),
    ('wrong-guard-lifetime', lambda r: edit_log(r, 'boundary', lambda t:
        t.replace('after0=4 after1=4', 'after0=104 after1=4', 1))),
    ('missing-sequence-operation', lambda r: edit_log(r, 'sequence', lambda t:
        '\n'.join(line for line in t.splitlines()
                  if not line.startswith('operation ordinal=0 ')) + '\n')),
    ('wrong-last-error', lambda r: edit_log(r, 'lifetime', lambda t:
        t.replace('last-error=4A2E19F7', 'last-error=00000000', 1))),
    ('wrong-status-conversion', lambda r: edit_log(r, 'status-map', lambda t:
        t.replace('error=559 ', 'error=558 ', 1))),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='dynamic-table-native-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutate(root)
        bind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-native.py')],
                                capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted altered native proof: ' + name)
        results.append({'case': name, 'rejected': True,
                        'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
