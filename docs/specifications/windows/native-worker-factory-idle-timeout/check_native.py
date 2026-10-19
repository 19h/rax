"""Verify captured native class2 status, normalization, memory and identities."""
from pathlib import Path
import hashlib
import json
import re
import sys

PROFILES = {'arm64': (43620, 8, True), 'x64': (34404, 8, True),
            'x86': (332, 4, False), 'x86-laa': (332, 4, True)}
SOURCES = {
    'v1': '7cf3a2d712aec67d83038d3127644aaa17b6cc6ccf319b805995854e2d8814c4',
    'v2': '20825d5a564a57c81e66351284a9e811f73fcd5741b75e472d5e3a6e9d5899e0',
    'v3': '043ef3d693205372c790e1de735776af71bd1b92ee46b686fa6a71b8e6e1b19d',
}
VALUES1 = [0, 1, -1, -10000000, -300000000, 10000000, 300000000,
           -2147483648, 2147483647, 4294967295, -9223372036854775808,
           9223372036854775807, 4294967296, -4294967296]
VALUES2 = [0, 1, -1, -9999999, -10000000, -10000001, -5999999999,
           -6000000000, -6000000001, -9223372036854775808,
           9223372036854775807, -300000000]


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff')
                                 else 'utf-8-sig'))


def enumeration(version):
    cases = []
    if version == 'v1':
        cases += [('value', value, 0, 8, 0) for value in VALUES1]
        cases += [('buffer', -300000000, mode, length, 0)
                  for mode in range(9)
                  for length in [0, 1, 3, 4, 7, 8, 9, 16, 128, 0xFFFFFFFF]]
        cases += [('handle-order', -300000000, mode, length, handle)
                  for handle in range(18) for mode in [0, 1, 4, 5]
                  for length in [0, 8, 9]]
    elif version == 'v2':
        cases += [('clamp-boundary', value, 0, 8, 0) for value in VALUES2]
        cases += [('alignment-priority', value, mode, length, 0)
                  for mode in [6, 9, 10, 11] for value in [0, 1, -300000000]
                  for length in [0, 8, 9]]
        cases += [('value-access-priority', value, mode, length, handle)
                  for handle in range(18) for value in [0, 1, -300000000]
                  for mode in [0, 1, 5] for length in [0, 8, 9]]
    else:
        cases += [('cross-page', value, mode, length, handle)
                  for mode in range(12, 23) for value in [0, -300000000]
                  for handle in [0, 2, 5] for length in [0, 8, 9, 0xFFFFFFFF]]
    assert len(cases) == {'v1': 320, 'v2': 534, 'v3': 264}[version]
    return cases * 2


def protections(mode):
    first, second = 4, 4
    if mode in [3, 10, 19]:
        first = 2
    if mode in [4, 11, 16]:
        first = 1
    if mode in [5, 9, 15, 17, 18, 20, 21, 22]:
        first = 0x104
    if mode in [8, 15, 21]:
        second = 1
    if mode in [13, 20]:
        second = 2
    if mode in [14, 16, 17, 19, 22]:
        second = 0x104
    return first, second


def expected(width, mode, length, handle, value):
    first, second = protections(mode)
    invalid_length = length < 8 if width == 4 else length != 8
    if invalid_length:
        return 0xC0000004, first, second
    if mode in [2, 6, 9, 10, 11, 21, 22]:
        return 0x80000002, first, second
    if mode in [1, 4, 8, 16]:
        return 0xC0000005, first, second
    if first & 0x100:
        return 0x80000001, first & ~0x100, second
    if second & 0x100:
        return 0x80000001, first, second & ~0x100
    if handle in [1, 2]:
        return 0xC0000008, first, second
    if handle in [3, 4, 11, 12]:
        return 0xC0000024, first, second
    if handle not in [0, 8, 10]:
        return 0xC0000022, first, second
    return (0 if value < 0 else 0xC000000D), first, second


def check(root):
    root = Path(root)
    manifest = load(root / 'native-manifest.json')
    names = set()
    for item in manifest['files']:
        path = item['path']
        assert path not in names and not Path(path).is_absolute() and '..' not in Path(path).parts
        names.add(path)
        raw = (root / path).read_bytes()
        assert len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256'], path
    native = root / 'native'
    total = 0
    for version, digest in SOURCES.items():
        stem = 'native-worker-factory-idle-' + version
        assert hashlib.sha256((native / (stem + '.cpp')).read_bytes()).hexdigest() == digest
        launch = load(native / (stem + '-launch.json'))
        assert launch['state'] == 'finished' and launch['exit-code'] == 0
        outcomes = load(native / (stem + '-outcomes.json'))
        assert len(outcomes) == 4 and {item['profile'] for item in outcomes} == set(PROFILES)
        modules = load(native / (stem + '-modules-before.json'))
        assert modules == load(native / (stem + '-modules-after.json')) and len(modules) == 6
        assert len({item['Path'].lower() for item in modules}) == 6
        assert all(re.fullmatch('[0-9A-F]{64}', item['Hash']) for item in modules)
        cases = enumeration(version)
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
            assert len(lines) == len(cases) + 1 and lines[-1] == f'complete cases={len(cases)}'
            for index, line in enumerate(lines[:-1]):
                row = dict(part.split('=', 1) for part in line.split())
                label, value, mode, length, handle = cases[index]
                assert int(row['case']) == index and int(row['width']) == width and row['class'] == '2'
                assert (row['label'], int(row['value'], 16), int(row['mode']), int(row['length'], 16),
                        int(row['handle-mode'])) == (label, value & ((1 << 64) - 1), mode, length, handle)
                status, first, second = expected(width, mode, length, handle, value)
                assert int(row['status'], 16) == status, (version, profile, index, row)
                assert int(row['protection'], 16) == first
                if version == 'v3':
                    assert int(row['protection-second'], 16) == second
                assert row['exception'] == row['query-before'] == row['query-after'] == '00000000'
                extent = 120 if width == 8 else 96
                assert int(row['returned-before']) == int(row['returned-after']) == extent
                assert row['maximum-before'] == row['maximum-after'] == '00000200'
                assert row['total-before'] == row['total-after'] == '0'
                assert int(row['handles-before']) == int(row['handles-after']) > 0
                before, after = bytes.fromhex(row['before']), bytes.fromhex(row['after'])
                assert len(before) == len(after) == 128
                assert before[:16] == after[:16] and before[24:] == after[24:]
                assert int.from_bytes(before[16:24], 'little', signed=True) == -670000000
                normalized = max(-6000000000, min(-10000000, value)) if status == 0 else -670000000
                assert int.from_bytes(after[16:24], 'little', signed=True) == normalized
                assert int.from_bytes(before[32:36], 'little') == 0
                assert int.from_bytes(before[40:44], 'little') == 512
                assert int.from_bytes(before[52:56], 'little') == 0
                offset = 96 if width == 8 else 84
                sizing = 0 if index < len(cases) // 2 else 1
                assert int.from_bytes(before[offset:offset + width], 'little') == 1 << (20 - sizing)
                assert int.from_bytes(before[offset + width:offset + 2 * width], 'little') == [4096, 8192][sizing]
                assert before[extent:] == b'\xA5' * (128 - extent)
            total += len(cases)
    assert total == 8944
    return {'native-observations': total, 'ABI-profiles': 4, 'physical-kernels': 1,
            'bound-native-files': len(names), 'active-workers': 0}


if __name__ == '__main__':
    print(json.dumps(check(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent), indent=2))
