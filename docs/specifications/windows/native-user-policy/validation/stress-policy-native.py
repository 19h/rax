from pathlib import Path
import hashlib
import json
import runpy
import shutil
import tempfile


P = Path(__file__).resolve().parent
D = P / 'policy-source/docs/specifications/windows/native-user-policy'
check = runpy.run_path(str(D / 'check_native.py'))['check']
check(D)


def modify(root, path, change):
    file = root / path
    file.write_text(change(file.read_text()))


def alter_json(root, path, change):
    file = root / path
    raw = file.read_bytes()
    data = json.loads(raw.decode('utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig'))
    change(data)
    file.write_text(json.dumps(data, indent=2) + '\n')


def rebind(root):
    file = root / 'native-manifest.json'
    data = json.loads(file.read_text())
    for row in data['files']:
        raw = (root / row['path']).read_bytes()
        row.update({'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    file.write_text(json.dumps(data, indent=2) + '\n')


LOG = 'native/native-user-policy-v3-arm64.log'
SID = 'native/native-sid-text-v1-arm64.log'
cases = [
    ('absent-status', lambda r: modify(r, LOG, lambda s: s.replace('status=C0000034', 'status=00000000', 1))),
    ('absent-output', lambda r: modify(r, LOG, lambda s: s.replace('out=0000000000000000', 'out=0000000000001234', 1))),
    ('duplicate-open', lambda r: modify(r, LOG, lambda s: s.replace('sample=1 path=0 desired=00000001', 'sample=0 path=0 desired=00000001', 1))),
    ('missing-open', lambda r: modify(r, LOG, lambda s: '\n'.join(s.splitlines()[:2] + s.splitlines()[3:]) + '\n')),
    ('handle-leak', lambda r: modify(r, LOG, lambda s: s.replace('handles-after=56', 'handles-after=57'))),
    ('wrong-PE-machine', lambda r: alter_json(r, 'native/native-user-policy-v3-arm64-before.json', lambda d: d.update({'machine': 34404}))),
    ('native-program-failure', lambda r: alter_json(r, 'native/native-user-policy-v3-outcomes.json', lambda d: d[0].update({'exit-code': 1}))),
    ('runtime-identity-change', lambda r: alter_json(r, 'native/native-user-policy-v3-modules-after.json', lambda d: d[0].update({'Hash': 'A' * 64}))),
    ('source-identity-change', lambda r: modify(r, 'native/native-user-policy-v3.cpp', lambda s: s + '\n// altered\n')),
    ('SID-native-format-drift', lambda r: modify(r, SID, lambda s: s.replace('text=S-1-0x100000000\n', 'text=S-1-4294967296\n', 1))),
    ('SID-count-drift', lambda r: modify(r, SID, lambda s: s.replace('authority=0 count=0', 'authority=0 count=1', 1))),
    ('SID-corpus-drift', lambda r: alter_json(r, 'sid-text-cases.json', lambda d: d['cases'][0].update({'text': 'S-1-5'}))),
]
results = []
for name, change in cases:
    with tempfile.TemporaryDirectory(prefix='assist-policy-native-') as temporary:
        root = Path(temporary) / 'evidence'
        shutil.copytree(D, root)
        change(root)
        rebind(root)
        try:
            check(root)
        except (AssertionError, KeyError, ValueError) as error:
            results.append({'name': name, 'rejected': True, 'failure-type': type(error).__name__,
                            'outer-envelope-rebound': True})
        else:
            raise AssertionError('logical corruption accepted: ' + name)
output = {'state': 'finished', 'controls': results, 'rejected': len(results),
          'scope': 'Each intentional logical corruption has its file and outer manifest hashes rebound before checking.'}
(P / 'policy-native-negative-controls.json').write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps({'rejected': len(results)}))
