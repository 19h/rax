from pathlib import Path
import hashlib
import importlib.util
import json
import shutil
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE / 'idle-source/docs/specifications/windows/native-worker-factory-idle-timeout'
spec = importlib.util.spec_from_file_location('native_checker', ROOT / 'check_native.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
assert checker.check(ROOT)['native-observations'] == 8944


def row_change(root, version, profile, predicate, change):
    path = root / 'native' / f'native-worker-factory-idle-{version}-{profile}.log'
    lines = path.read_text().splitlines()
    for index, line in enumerate(lines[:-1]):
        row = dict(part.split('=', 1) for part in line.split())
        if predicate(row):
            change(row)
            lines[index] = ' '.join(f'{key}={value}' for key, value in row.items())
            path.write_text('\n'.join(lines) + '\n')
            return
    raise AssertionError('control did not select a native case')


def json_change(root, name, change):
    path = root / 'native' / name
    value = checker.load(path)
    change(value)
    path.write_text(json.dumps(value, indent=2) + '\n')


def replace_field(key, value):
    return lambda row: row.__setitem__(key, value)


def native_row(version, profile, predicate, key, value):
    return lambda root: row_change(root, version, profile, predicate, replace_field(key, value))


def change_after(row):
    raw = bytearray.fromhex(row['after'])
    raw[16:24] = (-300000000).to_bytes(8, 'little', signed=True)
    row['after'] = raw.hex().upper()


controls = [
    ('success-status', native_row('v1', 'arm64', lambda r: r['status'] == '00000000', 'status', 'C000000D')),
    ('sign-validation', native_row('v2', 'arm64', lambda r: r['case'] == '0', 'status', '00000000')),
    ('wow-length', native_row('v1', 'x86', lambda r: r['length'] == '00000009' and r['status'] == '00000000', 'status', 'C0000004')),
    ('alignment-before-guard', native_row('v2', 'arm64', lambda r: r['mode'] == '9' and r['length'] == '00000008', 'status', '80000001')),
    ('guard-not-consumed', native_row('v1', 'arm64', lambda r: r['status'] == '80000001', 'protection', '00000104')),
    ('second-guard-consumed-too', native_row('v3', 'arm64', lambda r: r['mode'] == '17' and r['length'] == '00000008', 'protection-second', '00000004')),
    ('first-access-denied-guard', native_row('v3', 'arm64', lambda r: r['mode'] == '16' and r['length'] == '00000008', 'status', '80000001')),
    ('normalized-clamp', lambda root: row_change(root, 'v2', 'arm64', lambda r: r['case'] == '2', change_after)),
    ('value-before-handle', native_row('v2', 'arm64', lambda r: r['handle-mode'] == '2' and r['value'] == '0000000000000000' and r['length'] == '00000008' and r['mode'] == '0', 'status', 'C000000D')),
    ('query-size', native_row('v1', 'x86', lambda r: True, 'returned-after', '120')),
    ('case-ordinal', native_row('v1', 'arm64', lambda r: r['case'] == '1', 'case', '0')),
    ('PE-machine', lambda root: json_change(root, 'native-worker-factory-idle-v1-x64-before.json', lambda value: value.__setitem__('machine', 332))),
    ('producer-failure', lambda root: json_change(root, 'native-worker-factory-idle-v1-launch.json', lambda value: value.__setitem__('exit-code', 9))),
    ('source-identity', lambda root: json_change(root, 'native-worker-factory-idle-v1-arm64-source-after.json', lambda value: value.__setitem__('Hash', '0' * 64))),
]
results = []
for name, mutate in controls:
    with tempfile.TemporaryDirectory(prefix='idle-native-control-') as scratch:
        root = Path(scratch)
        shutil.copytree(ROOT / 'native', root / 'native')
        shutil.copyfile(ROOT / 'native-manifest.json', root / 'native-manifest.json')
        mutate(root)
        manifest = checker.load(root / 'native-manifest.json')
        for item in manifest['files']:
            raw = (root / item['path']).read_bytes()
            item.update({'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
        (root / 'native-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        try:
            checker.check(root)
        except AssertionError:
            results.append({'name': name, 'rejected': True, 'outer-envelope-rebound': True})
        else:
            raise AssertionError(f'logical corruption accepted: {name}')
assert len(results) == 14
(HERE / 'idle-native-negative-controls.json').write_text(json.dumps({
    'state': 'finished', 'controls': results, 'rejected': len(results),
    'scope': 'Every modified native file has its outer manifest size and hash rebound before semantic validation.',
}, indent=2) + '\n')
print('rejected', len(results), 'rebound logical corruptions')
