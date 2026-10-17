"""Exercise semantic rejection after rebinding altered evidence digests."""
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


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def change_json(root, name, mutate):
    path = root / 'validation' / name
    value = load(path)
    mutate(value)
    save(path, value)


def change_text(root, name, before, after):
    path = root / 'validation' / name
    text = path.read_text(encoding='utf-8-sig')
    if before not in text:
        raise ValueError('missing stress target ' + before)
    path.write_text(text.replace(before, after, 1))


def bind(root):
    path = root / 'validation/inputs.json'
    value = load(path)
    for row in value['files']:
        data = (root / row['path']).read_bytes()
        row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    save(path, value)


cases = [
    ('missing-ignored-test', lambda r: change_text(r, 'native-system-time-final-full.log',
      'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ignored',
      'test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ok')),
    ('different-native-failure', lambda r: change_text(r, 'native-system-time-final-full.log',
      '    smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
      '    unrecorded_new_failure')),
    ('invented-native-integration', lambda r: change_text(r, 'native-system-time-final-integration.log',
      '0 passed; 0 failed; 0 ignored;', '544 passed; 0 failed; 0 ignored;')),
    ('source-drift', lambda r: change_json(r, 'core-source-bridge.json',
      lambda v: v['final-inputs'][0].update(sha256='0' * 64))),
    ('enabled-core-defaults', lambda r: change_json(r, 'native-time-final-owning-archive-identities.json',
      lambda v: v.update({'core-features': ['default']}))),
    ('invented-program-success', lambda r: change_json(r, 'ordinary-analysis.json',
      lambda v: v[0].update({'terminal-lines': ['terminal turn=97977 Complete(Exited(0))']}))),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='system-time-validation-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        bind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-validation.py'),
                                 '--source-root', str(P)], capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted altered proof ' + name)
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
