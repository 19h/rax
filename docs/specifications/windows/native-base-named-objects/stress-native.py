"""Alter native claims, rebind their digests, and require semantic rejection."""
from pathlib import Path
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def edit_json(root, name, mutation):
    path = root / name
    value = load(path)
    mutation(value)
    save(path, value)


def replace_text(root, name, before, after):
    path = root / name
    text = path.read_text(encoding='utf-8-sig')
    if before not in text:
        raise RuntimeError('missing adversarial target ' + before)
    path.write_text(text.replace(before, after, 1))


def bad_machine(root):
    for suffix in ['before', 'after']:
        edit_json(root, 'native/current-session/session-directories-v1-arm64-' + suffix + '.json',
                  lambda value: value.update(machine=0x8664))


def bad_grant(root):
    path = root / 'session-directories-v1-arm64.log'
    text = path.read_text(encoding='utf-8-sig')
    pattern = r'^(open path=0 flags=00000040 access=0000000F status=00000000 .*?basic=)([0-9A-F]+)$'
    match = re.search(pattern, text, re.M)
    if match is None:
        raise RuntimeError('missing native grant row')
    data = bytearray.fromhex(match[2])
    data[4:8] = (3).to_bytes(4, 'little')
    text = text[:match.start(2)] + data.hex().upper() + text[match.end(2):]
    path.write_text(text)


def fake_openlink(root):
    path = root / 'session-directories-v1-arm64.log'
    text = path.read_text(encoding='utf-8-sig')
    good = re.search(r'^open path=0 flags=00000040 access=0000000F status=00000000 (.*)$', text, re.M)
    pattern = r'^open path=0 flags=00000100 access=0000000F status=C000000D$'
    text, count = re.subn(pattern, 'open path=0 flags=00000100 access=0000000F status=00000000 ' + good[1], text, count=1, flags=re.M)
    if count != 1:
        raise RuntimeError('missing native OPENLINK row')
    text = re.sub(r'(?m)^(complete paths=6 opens=)(\d+)', lambda match: match[1] + str(int(match[2]) + 1), text)
    path.write_text(text)


def fake_exit(root):
    path = root / 'diagnostics/directory-fast-0.log'
    text = path.read_text(encoding='utf-8-sig')
    text, count = re.subn(r'(?m)^terminal .*$', 'terminal turn=2149956 Complete(Exited(0))', text)
    if count != 1:
        raise RuntimeError('missing diagnostic terminal')
    path.write_text(text)


def rebind(root):
    for prefix in ['directory-fast', 'directory-frontier']:
        path = root / 'diagnostics' / (prefix + '-launch.json')
        value = load(path)
        for row in value['outcomes']:
            data = (root / 'diagnostics' / row['log']).read_bytes()
            row['log-identity'].update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
        save(path, value)
    path = root / 'native-inputs.json'
    manifest = load(path)
    for row in manifest['files']:
        data = (root / row['path']).read_bytes()
        row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    save(path, manifest)


cases = [
    ('hidden-churn-handle-leak', lambda root: replace_text(root, 'development/runtime-epoch1/namespace-churn-old.log', 'created=1008 closed=1008', 'created=1008 closed=1007')),
    ('missing-native-profile', lambda root: edit_json(root, 'native/current-session/session-directories-v1-outcomes.json', lambda rows: rows.pop())),
    ('wrong-PE-machine', bad_machine),
    ('invented-directory-grant', bad_grant),
    ('invented-OPENLINK-success', fake_openlink),
    ('hidden-handle-leak', lambda root: replace_text(root, 'session-directories-v1-arm64.log', 'handles-after=56', 'handles-after=57')),
    ('invented-snapshot-uniqueness', lambda root: replace_text(root, 'native/coherence/directory-coherence-v2-arm64.log', 'pass-complete pass=3 count=727 unique=0', 'pass-complete pass=3 count=727 unique=1')),
    ('invented-coherent-pair', lambda root: replace_text(root, 'native/coherence/directory-coherence-v2-arm64.log', 'pair attempt=1 coherent=0', 'pair attempt=1 coherent=1')),
    ('changed-diagnostic-image', lambda root: edit_json(root, 'diagnostics/directory-fast-launch.json', lambda value: (value['executable-before'].update(sha256='0' * 64), value['executable-after'].update(sha256='0' * 64)))),
    ('invented-guest-success', fake_exit),
]
results = []
for name, mutation in cases:
    with tempfile.TemporaryDirectory(prefix='native-base-proof-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutation(root)
        rebind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-native.py')], capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted altered proof: ' + name)
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
