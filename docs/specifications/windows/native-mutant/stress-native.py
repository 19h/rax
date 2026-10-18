"""Rebind evidence digests, then reject semantically inconsistent native proof."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent
P = D.parents[3]


def load(p):
    raw = p.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def save(p, value):
    p.write_text(json.dumps(value, indent=2) + '\n')


def edit(root, name, before, after):
    p = root / name
    text = p.read_text()
    if before not in text:
        raise ValueError('Missing stress target: ' + before)
    p.write_text(text.replace(before, after, 1))


def bad_module(root):
    for when in ['before', 'after']:
        p = root / ('native/native-v2/native-mutant-v2-modules-' + when + '.json')
        value = load(p)
        value[0]['Hash'] = '0' * 64
        save(p, value)


def rebind(root):
    for name in ['native-inputs.json', 'private-bno-inputs.json']:
        manifest = load(root / name)
        for row in manifest['files']:
            raw = (root / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
        save(root / name, manifest)


cases = [
    ('query-projection', 'check-native.py', lambda r: edit(r, 'replay/query-arm64.tsv', '\tC0000004\t', '\tC0000005\t')),
    ('release-state', 'check-native.py', lambda r: edit(r, 'native/native-faults/native-mutant-faults-v2-arm64.log', 'after=00000000/00000008/010000000000A5A5', 'after=00000000/00000008/020000000000A5A5')),
    ('create-cartesian-index', 'check-native.py', lambda r: edit(r, 'native/native-create-faults-v2/native-mutant-create-faults-v2-arm64.log', 'case=0 ', 'case=1 ')),
    ('win32-nul-identity', 'check-native.py', lambda r: edit(r, 'native/native-names-v6/native-mutant-names-v6-arm64.log', 'same-native=0', 'same-native=1')),
    ('missing-handle-closure', 'check-native.py', lambda r: edit(r, 'native/native-names-v6/native-mutant-names-v6-arm64.log', 'closed=101', 'closed=100')),
    ('both-ntdll-identities', 'check-native.py', bad_module),
    ('private-global-routing', 'check-private-bno.py', lambda r: edit(r, 'private-bno-prerequisite/bno-routing/bno-routing-arm64-enabled-ascii.log', 'target=005C0042006100730065004E0061006D00650064004F0062006A0065006300740073', 'target=005C0042006100730065004E0061006D00650064004F0062006A00650063007400730058')),
]
for name in ['check-native.py', 'check-private-bno.py']:
    subprocess.run([sys.executable, '-B', str(D / name), *(['--source-root', str(P)] if name == 'check-native.py' else [])], check=True)
results = []
for name, checker, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='mutant-native-stress-') as tmp:
        root = Path(tmp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutate(root)
        rebind(root)
        command = [sys.executable, '-B', str(root / checker)]
        if checker == 'check-native.py':
            command += ['--source-root', str(P)]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('Accepted rebound native alteration: ' + name)
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
