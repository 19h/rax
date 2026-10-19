"""Native second-stack controls, separate from the compiled source epoch."""
from pathlib import Path
import hashlib
import json
import runpy
import sys


def check(root):
    root = Path(root)
    base = runpy.run_path(str(root / 'check_native.py'))
    load, status = base['load'], base['status']
    native = root / 'supplement'
    manifest = load(native / 'manifest.json')
    names = set()
    for row in manifest['files']:
        name = row['path']
        assert name not in names and Path(name).name == name
        names.add(name)
        raw = (native / name).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    stem = 'native-worker-factory-flags-v4'
    digest = '6850ea46c973efcd147f6cc219068ef708c2022cf8eb07716e4729973c429972'
    assert hashlib.sha256((native / (stem + '.cpp')).read_bytes()).hexdigest() == digest
    launch = load(native / (stem + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    modules = load(native / (stem + '-modules-before.json'))
    assert modules == load(native / (stem + '-modules-after.json')) and len(modules) == 6
    assert modules == load(root / 'native/native-worker-factory-flags-v2-modules-before.json')
    outcomes = load(native / (stem + '-outcomes.json'))
    assert len(outcomes) == 4 and {r['profile'] for r in outcomes} == set(base['PROFILES'])
    calls = 0
    for outcome in outcomes:
        profile = outcome['profile']
        machine, width, laa = base['PROFILES'][profile]
        assert outcome['exit-code'] == 0
        prefix = stem + '-' + profile
        assert outcome['log'] == prefix + '.log'
        pe = load(native / (prefix + '-before.json'))
        assert pe == load(native / (prefix + '-after.json'))
        assert pe['machine'] == machine and pe['large-address-aware'] is laa and pe['bytes'] > 0
        assert load(native / (prefix + '-source-after.json'))['Hash'].lower() == digest
        lines = (native / outcome['log']).read_text().splitlines()
        assert len(lines) == 651 and lines[-1] == 'complete cases=650'
        for index, line in enumerate(lines[:-1]):
            row = dict(part.split('=', 1) for part in line.split())
            assert int(row['case']) == index and int(row['width']) == width and row['class'] == '13'
            n = index % 325
            if n < 37:
                expected = ('value', base['VALUES'][n], 0, 4, 0)
            elif n < 109:
                k = n - 37
                expected = ('buffer', 1, k // 8, base['LENGTHS'][k % 8], 0)
            else:
                k = n - 109
                expected = ('handle-order', 1, [0, 1, 4, 5][k % 12 // 3], [0, 4, 5][k % 3], k // 12)
            mode, length, handle, value = (int(row[key], radix) for key, radix in
                                          [('mode', 10), ('length', 16), ('handle-mode', 10), ('value', 16)])
            assert (row['label'], value, mode, length, handle) == expected
            assert int(row['status'], 16) == status(mode, length, handle) and row['exception'] == '00000000'
            assert row['query-before'] == row['query-after'] == '00000000'
            assert int(row['returned-before']) == int(row['returned-after']) == (120 if width == 8 else 96)
            assert row['before'] == row['after']
            blob = bytes.fromhex(row['before'])
            assert len(blob) == 128 and int.from_bytes(blob[32:36], 'little') == 0
            maximum = 1 if index < 325 else 512
            assert int.from_bytes(blob[40:44], 'little') == maximum
            assert int(row['maximum-before'], 16) == int(row['maximum-after'], 16) == maximum
            assert row['total-before'] == row['total-after'] == '0'
            assert int(row['handles-before']) == int(row['handles-after']) > 0
            offset = 96 if width == 8 else 84
            assert int.from_bytes(blob[offset:offset + width], 'little') == 524288
            assert int.from_bytes(blob[offset + width:offset + 2 * width], 'little') == 8192
            assert blob[120 if width == 8 else 96:] == b'\xA5' * (8 if width == 8 else 32)
            protection = 2 if mode == 3 else 1 if mode == 4 else 0x104 if mode == 5 and length != 4 else 4
            assert int(row['protection'], 16) == protection
            calls += 1
    assert calls == 2600
    return {'native-second-stack-cases': calls, 'compiled-input-changes': False}


if __name__ == '__main__':
    print(json.dumps(check(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent), indent=2))
