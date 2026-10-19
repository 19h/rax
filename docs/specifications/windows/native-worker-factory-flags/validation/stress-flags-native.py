from pathlib import Path
import hashlib
import json
import runpy
import shutil
import tempfile


P = Path(__file__).resolve().parent
D = P / 'flags-source/docs/specifications/windows/native-worker-factory-flags'
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


LOG = 'native/native-worker-factory-flags-v2-arm64.log'
SEQ = 'native/native-worker-factory-flags-v3-arm64.log'
def change_guard(root):
    modify(root, LOG, lambda s: '\n'.join(line.replace('protection=00000004', 'protection=00000104') if 'mode=5 length=00000004' in line else line for line in s.splitlines())+'\n')
controls = [
 ('success-status', lambda r: modify(r, LOG, lambda s:s.replace('status=00000000','status=C000000D',1))),
 ('duplicate-case', lambda r: modify(r, LOG, lambda s:s.replace('case=1 label=', 'case=0 label=',1))),
 ('missing-case', lambda r: modify(r, LOG, lambda s:'\n'.join(s.splitlines()[1:])+'\n')),
 ('guard-not-consumed', change_guard),
 ('buffer-length-drift', lambda r: modify(r, LOG, lambda s:s.replace('label=buffer width=8 class=13 value=00000001 mode=0 length=00000000','label=buffer width=8 class=13 value=00000001 mode=0 length=00000004',1))),
 ('constructor-hard-drift', lambda r: modify(r, LOG, lambda s:s.replace('maximum-before=00000001','maximum-before=00000200',1))),
 ('basic-query-size-drift', lambda r: modify(r, LOG, lambda s:s.replace('returned-after=120','returned-after=96',1))),
 ('sequence-status', lambda r: modify(r, SEQ, lambda s:s.replace('status=00000000','status=C0000022',1))),
 ('PE-machine-drift', lambda r:alter_json(r,'native/native-worker-factory-flags-v2-arm64-before.json',lambda x:x.update(machine=34404))),
 ('producer-failed', lambda r:alter_json(r,'native/native-worker-factory-flags-v2-outcomes.json',lambda x:x[0].update({'exit-code':1}))),
 ('module-identity-drift', lambda r:alter_json(r,'native/native-worker-factory-flags-v2-modules-after.json',lambda x:x[0].update(Hash='A'*64))),
 ('source-identity-drift', lambda r:modify(r,'native/native-worker-factory-flags-v2.cpp',lambda s:s+'\n// altered\n')),
]
cases = controls
results = []
for name, change in cases:
    with tempfile.TemporaryDirectory(prefix='assist-filesystem-native-') as temporary:
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
(P / 'flags-native-negative-controls.json').write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps({'rejected': len(results)}))
