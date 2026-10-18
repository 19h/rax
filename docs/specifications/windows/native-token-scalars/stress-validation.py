"""Require semantic rejection even when altered validation digests are rebound."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent
P = D.parents[3]


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def edit_json(root, name, mutate):
    path = root / 'validation' / name
    value = load(path)
    mutate(value)
    save(path, value)


def edit_text(root, name, before, after):
    path = root / 'validation' / name
    text = path.read_text(encoding='utf-8-sig')
    if before not in text:
        raise ValueError('missing stress target: ' + before)
    path.write_text(text.replace(before, after, 1))


def bind(root):
    for path in (root / 'validation').glob('*gates.json'):
        value = load(path)
        for row in value:
            if 'sha256' in row:
                row['sha256'] = hashlib.sha256(
                    (root / 'validation' / row['log']).read_bytes()).hexdigest()
        save(path, value)
    path = root / 'validation/inputs.json'
    value = load(path)
    for row in value['files']:
        raw = (root / row['path']).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    save(path, value)


cases = [
    ('removed-byte-preservation', lambda r: (r / '.gitattributes').write_text('* text\n')),
    ('different-native-failure', lambda r: edit_text(r, 'native-scalar-final-full.log',
        '    smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
        '    unrecorded_new_failure')),
    ('missing-ignored-test', lambda r: edit_text(r, 'native-scalar-final-full.log',
        'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ignored',
        'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ok')),
    ('invented-native-integration', lambda r: edit_text(r, 'native-scalar-final-integration.log',
        '0 passed; 0 failed; 0 ignored;', '544 passed; 0 failed; 0 ignored;')),
    ('changed-source', lambda r: edit_json(r, 'scalar-final-source.json',
        lambda v: v['files'][0].update(sha256='0' * 64))),
    ('enabled-core-defaults', lambda r: edit_json(r, 'native-scalar-final-owning-archive-identities.json',
        lambda v: v.update({'core-features': ['default']}))),
    ('wrong-core-configuration', lambda r: edit_json(r, 'native-scalar-final-gates.json',
        lambda v: v[0].update(command=v[0]['command'].replace('--no-default-features', '--all-features')))),
    ('wrong-owning-artifact', lambda r: edit_json(r, 'native-scalar-final-owning-archive-identities.json',
        lambda v: v['core-artifact'].update(package_id='unrecorded-artifact'))),
    ('invented-program-success', lambda r: (
        edit_json(r, 'ordinary-analysis.json',
            lambda v: v[0].update({'terminal-lines': ['terminal turn=999999 Complete(Exited(0))']})),
        edit_json(r, 'ordinary-contract.json',
            lambda v: v[0].update({'terminal': 'terminal turn=999999 Complete(Exited(0))'})))),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='token-scalars-validation-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        bind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-validation.py'),
                                 '--source-root', str(P)], capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted altered proof: ' + name)
        results.append({'case': name, 'rejected': True,
                        'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
