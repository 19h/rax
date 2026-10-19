"""Check independent dormant WorkerFactoryFlags observations and identities."""
from pathlib import Path
import hashlib
import json
import re
import sys


PROFILES = {'arm64': (43620, 8, True), 'x64': (34404, 8, True),
            'x86': (332, 4, False), 'x86-laa': (332, 4, True)}
SOURCES = {'v2': 'f04b1e330502754d3978480b91edbc40aa837fdb1d66e3c78f0bf4e35ff1db59',
           'v3': 'f57bb467f88b6672240f5208298d3646562b5922091c8a76dca6e4d68e21cb22'}
VALUES = [0] + [1 << bit for bit in range(32)] + [3, 7, 0x7FFFFFFF, 0xFFFFFFFF]
LENGTHS = [0, 1, 3, 4, 5, 8, 128, 0xFFFFFFFF]


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig'))


def status(mode, length, handle):
    if length != 4:
        return 0xC0000004
    if mode in [1, 2, 4, 8]:
        return 0xC0000005
    if mode == 5:
        return 0x80000001
    if handle in [1, 2]:
        return 0xC0000008
    if handle in [3, 4, 11, 12]:
        return 0xC0000024
    return 0 if handle in [0, 8, 10] else 0xC0000022


def check(root):
    root = Path(root)
    manifest = load(root / 'native-manifest.json')
    names = set()
    for row in manifest['files']:
        name = row['path']
        assert name not in names and not Path(name).is_absolute() and '..' not in Path(name).parts
        names.add(name)
        raw = (root / name).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], name
    native = root / 'native'
    total = 0
    for version, digest in SOURCES.items():
        stem = 'native-worker-factory-flags-' + version
        assert hashlib.sha256((native / (stem + '.cpp')).read_bytes()).hexdigest() == digest
        launch = load(native / (stem + '-launch.json'))
        assert launch['state'] == 'finished' and launch['exit-code'] == 0
        outcomes = load(native / (stem + '-outcomes.json'))
        assert len(outcomes) == 4 and {r['profile'] for r in outcomes} == set(PROFILES)
        modules = load(native / (stem + '-modules-before.json'))
        assert modules == load(native / (stem + '-modules-after.json')) and len(modules) == 6
        assert len({r['Path'].lower() for r in modules}) == 6
        assert all(re.fullmatch('[0-9A-F]{64}', r['Hash']) for r in modules)
        for outcome in outcomes:
            profile = outcome['profile']
            machine, width, laa = PROFILES[profile]
            assert outcome['exit-code'] == 0 and outcome['log'] == stem + '-' + profile + '.log'
            prefix = stem + '-' + profile
            pe = load(native / (prefix + '-before.json'))
            assert pe == load(native / (prefix + '-after.json'))
            assert pe['profile'] == profile and pe['machine'] == machine and pe['large-address-aware'] is laa
            assert pe['bytes'] > 0 and re.fullmatch('[0-9a-f]{64}', pe['sha256'])
            assert load(native / (prefix + '-source-after.json'))['Hash'].lower() == digest
            assert not re.search(r'error C[0-9]+|fatal error|error LNK', (native / (prefix + '-build.log')).read_text(errors='replace'))
            lines = (native / outcome['log']).read_text().splitlines()
            count = 650 if version == 'v2' else 9
            assert len(lines) == count + 1 and lines[-1] == f'complete cases={count}'
            for index, line in enumerate(lines[:-1]):
                row = dict(part.split('=', 1) for part in line.split())
                assert int(row['case']) == index and int(row['width']) == width and row['class'] == '13'
                mode, length, handle, value = (int(row[key], base) for key, base in
                                              [('mode', 10), ('length', 16), ('handle-mode', 10), ('value', 16)])
                if version == 'v2':
                    n = index % 325
                    maximum, bindings = (1 if index < 325 else 512), 0
                    if n < 37:
                        expected = ('value', VALUES[n], 0, 4, 0)
                    elif n < 109:
                        k = n - 37
                        expected = ('buffer', 1, k // 8, LENGTHS[k % 8], 0)
                    else:
                        k = n - 109
                        expected = ('handle-order', 1, [0, 1, 4, 5][k % 12 // 3], [0, 4, 5][k % 3], k // 12)
                else:
                    seed = index // 3
                    maximum, bindings = (512 if seed == 0 else 1), (4 if seed == 2 else 0)
                    expected = (['initial', 'limits', 'limits-binding'][seed], [0, 1, 0xFFFFFFFF][index % 3], 0, 4, 0)
                assert (row['label'], value, mode, length, handle) == expected
                assert int(row['status'], 16) == status(mode, length, handle)
                assert row['exception'] == '00000000'
                assert row['query-before'] == row['query-after'] == '00000000'
                assert int(row['returned-before']) == int(row['returned-after']) == (120 if width == 8 else 96)
                assert int(row['maximum-before'], 16) == int(row['maximum-after'], 16) == maximum
                assert row['total-before'] == row['total-after'] == '0'
                assert int(row['handles-before']) == int(row['handles-after']) > 0
                before, after = bytes.fromhex(row['before']), bytes.fromhex(row['after'])
                assert before == after and len(before) == 128
                assert int.from_bytes(before[32:36], 'little') == bindings
                assert int.from_bytes(before[40:44], 'little') == maximum
                assert int.from_bytes(before[52:56], 'little') == 0
                offset = 96 if width == 8 else 84
                assert int.from_bytes(before[offset:offset + width], 'little') == 1 << 20
                assert int.from_bytes(before[offset + width:offset + 2 * width], 'little') == 4096
                assert before[120 if width == 8 else 96:] == b'\xA5' * (8 if width == 8 else 32)
                protection = 2 if mode == 3 else 1 if mode == 4 else 0x104 if mode == 5 and length != 4 else 4
                assert int(row['protection'], 16) == protection
            total += count
    assert total == 2636
    return {'native-setter-cases': 2600, 'native-interleaving-controls': 36,
            'native-ABI-profiles': 4, 'physical-Windows-kernels': 1, 'bound-native-files': len(names)}


if __name__ == '__main__':
    print(json.dumps(check(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent), indent=2))
