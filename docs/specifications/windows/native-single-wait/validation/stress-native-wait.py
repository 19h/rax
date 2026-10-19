from pathlib import Path
import importlib.util
import json
import shutil
import tempfile

N = Path(__file__).resolve().parent
D = N / 'wait-source/docs/specifications/windows/native-single-wait'
spec = importlib.util.spec_from_file_location('native_wait_checker', D / 'check_native.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
profile = 'native-v5/native-wait-single-v5-arm64.log'

def replace_line(root, predicate, edit):
    path = root / profile
    lines = path.read_text().splitlines(keepends=True)
    indices = [i for i, line in enumerate(lines) if predicate(line)]
    if len(indices) != 1:
        raise ValueError('stress fixture selection')
    lines[indices[0]] = edit(lines[indices[0]])
    path.write_text(''.join(lines))

def row(root, prefix, index, before, after):
    replace_line(root, lambda line: line.startswith(prefix + ' case=' + str(index) + ' '),
        lambda line: line.replace(before, after))

def alter_pe(root):
    for suffix in ['before', 'after']:
        path = root / ('native-v5/native-wait-single-v5-arm64-' + suffix + '.json')
        data = module.load(path)
        data['machine'] = 34404
        path.write_text(json.dumps(data))

def alter_projection(root):
    path = root / 'replay/object-apc-arm64.tsv'
    raw = path.read_text()
    path.write_text(raw.replace('000000C0', '00000000', 1))

def alter_source(root):
    path = root / 'native-v5/native-wait-single-v5.cpp'
    path.write_bytes(path.read_bytes() + b'\n')

cases = [
    ('timeout-before-invalid-handle', lambda root: row(root, 'wait', 425, 'status=C0000005', 'status=C0000008')),
    ('native-apc-before-signal', lambda root: row(root, 'apc', 8, 'status=000000C0', 'status=00000000')),
    ('win32-alertable-low-byte', lambda root: row(root, 'win32-apc', 4, 'status=00000102', 'status=000000C0')),
    ('multiple-waitall-full-bool', lambda root: row(root, 'multiple-apc', 35, 'status=00000102', 'status=00000000')),
    ('sleep-low-byte', lambda root: row(root, 'sleep-apc', 4, 'status=00000000', 'status=000000C0')),
    ('owned-mutant-apc', lambda root: row(root, 'object-apc', 57, 'status=000000C0', 'status=00000000')),
    ('clock-provider', lambda root: replace_line(root, lambda line: line.startswith('clock-provider '), lambda line: line.replace('kernelbase=1', 'kernelbase=0'))),
    ('kernel-build', lambda root: replace_line(root, lambda line: line.startswith('context '), lambda line: line.replace('build=29683', 'build=29684'))),
    ('matrix-omission', lambda root: replace_line(root, lambda line: line.startswith('wait case=559 '), lambda line: '')),
    ('handle-leak', lambda root: replace_line(root, lambda line: line.startswith('done '), lambda line: line.replace('after=49', 'after=50'))),
    ('pe-machine-both-identities-rebound', alter_pe),
    ('replay-projection', alter_projection),
    ('producer-source', alter_source),
]
results = []
for name, mutate in cases:
    with tempfile.TemporaryDirectory(prefix='assist-wait-native-stress-') as temporary:
        root = Path(temporary) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        before = {path.relative_to(root): path.read_bytes() for path in root.rglob('*') if path.is_file()}
        mutate(root)
        changed = [str(path) for path, raw in before.items() if (root / path).read_bytes() != raw]
        if not changed:
            raise ValueError('stress mutation did not change bytes: ' + name)
        try:
            module.check_native(root)
        except ValueError as error:
            results.append({'name': name, 'changed': changed, 'result': 'rejected', 'reason': str(error)})
        else:
            raise ValueError('semantic checker accepted mutation: ' + name)
(N / 'wait-native-stress.json').write_text(json.dumps({'scope': 'Semantic checks run independently of outer evidence digests; rebinding an outer manifest cannot bypass these controls.', 'cases': results}, indent=2) + '\n')
print('Rejected', len(results), 'independent native-evidence corruptions')
