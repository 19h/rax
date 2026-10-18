"""Rebind altered input digests and require rejection of changed native claims."""
from pathlib import Path
import json
import hashlib
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent

def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))

def edit_json(root, name, mutate):
    path = root/name
    value = load(path)
    mutate(value)
    path.write_text(json.dumps(value, indent=2)+'\n')

def edit_text(root, name, old, new):
    path = root/name
    text = path.read_text(encoding='utf-8-sig')
    assert old in text
    path.write_text(text.replace(old, new, 1))

def bind(root):
    path = root/'native-inputs.json'
    value = load(path)
    for row in value['files']:
        raw = (root/row['path']).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    path.write_text(json.dumps(value, indent=2)+'\n')

cases = [
    ('missing-private-profile', lambda r: edit_json(r, 'native/bno-isolation-child-outcomes.json', lambda v: v.pop())),
    ('forged-query-success', lambda r: edit_text(r, 'native/bno-isolation-survey-arm64-class44.log', 'status=C0000023', 'status=00000000')),
    ('forged-padding', lambda r: edit_json(r, 'cases.json', lambda v: v['groups'][0]['rows'][0].update({'output-bytes-cells':'00'*32}))),
    ('changed-native-source', lambda r: edit_text(r, 'native/native-bno-isolation-survey.cpp', 'ULONG selected_class = 0', 'ULONG selected_class = 44')),
    ('wrong-native-machine', lambda r: [edit_json(r, 'native/bno-isolation-survey-arm64-'+suffix+'.json', lambda v: v.update(machine=332)) for suffix in ['identity-before', 'identity']]),
    ('unclosed-child-handles', lambda r: edit_text(r, 'native/bno-isolation-child-arm64-enabled-ascii.log', 'exit=0 closed=1', 'exit=0 closed=0')),
    ('changed-post-run-artifact', lambda r: edit_json(r, 'native/bno-native-post-identities.json', lambda v: v['files'][1].update(sha256='0'*64))),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='bno-native-stress-') as temporary:
        root = Path(temporary)/'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        bind(root)
        run = subprocess.run([sys.executable, '-B', str(root/'check-native.py')], capture_output=True, text=True)
        if run.returncode == 0:
            raise RuntimeError('accepted changed claim: '+name)
        results.append({'case':name,'rejected':True,'error':run.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
