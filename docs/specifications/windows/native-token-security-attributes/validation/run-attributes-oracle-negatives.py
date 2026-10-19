"""Reject logical corruption after keeping the replay corpus in agreement."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile
import sys

HERE = Path(__file__).resolve().parent
ROOT = Path('/Users/int/.assist-token-20261019')
SOURCE = ROOT / 'docs/specifications/windows/native-token-security-attributes'


def put(path, data):
    path.unlink()
    path.write_bytes(data)


def json_change(directory, name, edit):
    path = directory / name
    raw = path.read_bytes()
    value = json.loads(raw.decode('utf-16' if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else 'utf-8-sig'))
    edit(value)
    put(path, json.dumps(value).encode())


def row_change(directory, field, value, empty=False, upper=False):
    version, index = ('v9', 2) if upper else ('v7b', 2) if empty else ('v3', 482)
    path = directory / 'native' / f'native-token-security-attributes-{version}-arm64.log'
    lines = path.read_text().splitlines()
    for i, line in enumerate(lines):
        if line.startswith(f'case={index} '):
            row = dict(part.split('=', 1) for part in line.split())
            row[field] = value(row[field]) if callable(value) else value
            lines[i] = ' '.join(f'{key}={item}' for key, item in row.items())
            break
    put(path, ('\n'.join(lines) + '\n').encode())
    corpus = directory / ('upper-cases.json' if upper else 'empty-cases.json' if empty else 'cases.json')
    data = json.loads(corpus.read_text())
    group = next(g for g in data['groups'] if g['profile'] == 'arm64' and (empty or upper or g['version'] == version))
    group['cases'][index] = row
    put(corpus, json.dumps(data).encode())


def main():
    prefix = 'native/native-token-security-attributes-v1-arm64'
    controls = [
        ('source-identity', lambda d: put(d / 'native/native-token-security-attributes-v1.cpp', b'changed source')),
        ('launch-state', lambda d: json_change(d, 'native/native-token-security-attributes-v1-launch.json', lambda v: v.update(state='running'))),
        ('pe-pair', lambda d: json_change(d, prefix + '-after.json', lambda v: v.update(sha256='0' * 64))),
        ('pe-machine', lambda d: [json_change(d, prefix + suffix, lambda v: v.update(machine=0)) for suffix in ['-before.json', '-after.json']]),
        ('pe-laa', lambda d: [json_change(d, prefix + suffix, lambda v: v.update({'large-address-aware': False})) for suffix in ['-before.json', '-after.json']]),
        ('source-after', lambda d: json_change(d, prefix + '-source-after.json', lambda v: v.update(Hash='0' * 64))),
        ('module-drift', lambda d: json_change(d, 'native/native-token-security-attributes-v1-modules-after.json', lambda v: v[0].update(Hash='0' * 64))),
        ('status-rebound', lambda d: row_change(d, 'status', 'C000000D')),
        ('required-rebound', lambda d: row_change(d, 'returned', '00000069')),
        ('payload-rebound', lambda d: row_change(d, 'bytes', lambda v: '02' + v[2:])),
        ('guard-rebound', lambda d: row_change(d, 'out-protect', '00000104,00000004')),
        ('last-error-rebound', lambda d: row_change(d, 'last-error', '0000C0DF')),
        ('empty-tail-rebound', lambda d: row_change(d, 'bytes', lambda v: v[:32] + '00' + v[34:], empty=True)),
        ('profile-membership', lambda d: json_change(d, 'cases.json', lambda v: v['groups'].__setitem__(1, v['groups'][0]))),
    ]
    controls += [('upper-required-rebound', lambda d: row_change(d, 'returned', '00000069', upper=True))]
    controls.insert(0, ('uncorrupted-copy', lambda d: None))
    results = []
    sources = [SOURCE / name for name in ['check_layouts.py', 'derive-query-order.py', 'cases.json', 'empty-cases.json', 'upper-cases.json']]
    sources += [p for p in (SOURCE / 'native').iterdir() if p.is_file()]
    original = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    for name, edit in controls:
        with tempfile.TemporaryDirectory(prefix='attribute-oracle-', dir=ROOT / 'build-tmp') as temporary:
            directory = Path(temporary)
            for path in sources:
                destination = directory / path.relative_to(SOURCE)
                destination.parent.mkdir(parents=True, exist_ok=True)
                os.link(path, destination)
            edit(directory)
            result = subprocess.run([sys.executable, '-B', str(directory / 'check_layouts.py')], capture_output=True)
            assert (result.returncode == 0) if name == 'uncorrupted-copy' else (result.returncode != 0), name
            results.append({'control': name, 'exit-code': result.returncode,
                            'output-sha256': hashlib.sha256(result.stdout + result.stderr).hexdigest(),
                            'detail': (result.stdout + result.stderr).decode(errors='replace')[-1200:]})
    for path, expected in original.items():
        assert hashlib.sha256(Path(path).read_bytes()).hexdigest() == expected, path
    (HERE / 'attributes-oracle-negative-results.json').write_text(json.dumps({'checker-sha256': original[str(SOURCE / 'check_layouts.py')], 'controls': results}, indent=2) + '\n')
    print('valid-copy passed;', len(results) - 1, 'corruptions rejected; source files unchanged')


if __name__ == '__main__':
    main()
