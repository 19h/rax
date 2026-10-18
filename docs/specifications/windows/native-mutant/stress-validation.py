"""Reject altered final claims after their outer evidence digests are rebound."""
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


def edit_json(root, name, mutate):
    p = root / 'validation' / name
    value = load(p)
    mutate(value)
    save(p, value)


def edit_text(root, name, before, after):
    p = root / 'validation' / name
    text = p.read_text(encoding='utf-8-sig')
    if before not in text:
        raise ValueError('Missing stress target')
    p.write_text(text.replace(before, after, 1))


def fake_exit(root):
    p = root / 'validation/mutant-ordinary-reviewed.json'
    reviewed = load(p)
    original = reviewed['programs'][0]['terminal-line']
    invented = original.split(' Complete(')[0] + ' Complete(Exited(0))'
    reviewed['programs'][0]['terminal-line'] = invented
    save(p, reviewed)
    edit_text(root, 'mutant-owner-observer-0.log', original, invented)
    owner_p = root / 'validation/mutant-owner-observer-launch.json'
    owner = load(owner_p)
    raw = (root / 'validation/mutant-owner-observer-0.log').read_bytes()
    owner['outcomes'][0]['log-identity'].update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    save(owner_p, owner)


def rebind(root):
    for p in (root / 'validation').glob('*gates.json'):
        value = load(p)
        for row in value['steps'] if isinstance(value, dict) else value:
            if 'log' in row:
                raw = (root / 'validation' / row['log']).read_bytes()
                row.update(sha256=hashlib.sha256(raw).hexdigest())
                if 'bytes' in row:
                    row['bytes'] = len(raw)
        save(p, value)
    endings = load(root / 'line-ending-verification.json')
    for row in endings['files']:
        raw = (root / row['path']).read_bytes()
        blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
        row.update({'raw-git-blob': blob, 'autocrlf-true-git-blob': blob})
    save(root / 'line-ending-verification.json', endings)
    p = root / 'validation/inputs.json'
    manifest = load(p)
    for row in manifest['files']:
        raw = (root / row['path']).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    save(p, manifest)


cases = [
    ('changed-selected-source', lambda r: edit_json(r, 'mutant-source-epoch4.json', lambda v: v['files'][0].update(sha256='0' * 64)), 'source identity'),
    ('new-native-failure', lambda r: edit_text(r, 'mutant-native-epoch4c-full.log', '    smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime', '    unrecorded_native_failure'), 'exact five existing'),
    ('hidden-ignore', lambda r: edit_text(r, 'mutant-native-epoch4c-full.log', 'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ignored', 'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ok'), 'ignored identities'),
    ('missing-all-target-check', lambda r: edit_json(r, 'mutant-native-epoch4c-gates.json', lambda v: v['steps'].pop()), 'complete gate selection'),
    ('missing-native-fixture-closure', lambda r: edit_json(r, 'mutant-native-epoch4c-gates.json', lambda v: v['restored-baseline-files'].pop()), 'compilation input closure'),
    ('enabled-core-defaults', lambda r: edit_json(r, 'native-mutant-final-owning-archive-identities.json', lambda v: v.update({'core-features': ['default']})), 'empty-feature current archive'),
    ('wrong-actual-artifact', lambda r: edit_json(r, 'native-mutant-final-owning-archive-identities.json', lambda v: v['core-artifact'].update(package_id='invented')), 'actual selected artifacts'),
    ('false-current-source-equality', lambda r: edit_json(r, 'mutant-upstream-source-comparison.json', lambda v: v['files'][0].update(equal=False)), 'current C++ consumers'),
    ('invented-ordinary-guest-exit', fake_exit, 'explicit remaining native wait boundary'),
]
command = [sys.executable, '-B', str(D / 'check-validation.py'), '--source-root', str(P)]
subprocess.run(command, check=True)
results = []
for name, mutate, expected in cases:
    with tempfile.TemporaryDirectory(prefix='mutant-validation-stress-') as tmp:
        root = Path(tmp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        rebind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-validation.py'), '--source-root', str(P)], capture_output=True, text=True)
        if result.returncode == 0 or expected not in result.stderr:
            raise RuntimeError('Failed semantic rejection control: ' + name + '\n' + result.stderr[-2000:])
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
