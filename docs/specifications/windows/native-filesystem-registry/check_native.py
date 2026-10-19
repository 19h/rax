"""Check independent native observations without importing product code."""
from pathlib import Path
import hashlib
import json
import re
import sys


PROFILES = {'arm64': (43620, 8, True), 'x64': (34404, 8, True),
            'x86': (332, 4, False), 'x86-laa': (332, 4, True)}
SOURCES = {'native-filesystem-registry-v1': 'c2941920f387982505e702549e6e7318f66a55ffd7dd26edba2dc3496ede3777'}


def read_json(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig'))


def check(root):
    root = Path(root)
    manifest = read_json(root / 'native-manifest.json')
    names = set()
    for row in manifest['files']:
        path = row['path']
        assert path not in names and not Path(path).is_absolute() and '..' not in Path(path).parts
        names.add(path)
        raw = (root / path).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], path
    assert len(names) == 35
    native = root / 'native'
    open_count = 0
    for prefix, source_hash in SOURCES.items():
        assert hashlib.sha256((native / (prefix + '.cpp')).read_bytes()).hexdigest() == source_hash
        launch = read_json(native / (prefix + '-launch.json'))
        assert launch['state'] == 'finished' and launch['exit-code'] == 0
        outcomes = read_json(native / (prefix + '-outcomes.json'))
        assert len(outcomes) == 4 and {r['profile'] for r in outcomes} == set(PROFILES)
        before = read_json(native / (prefix + '-modules-before.json'))
        after = read_json(native / (prefix + '-modules-after.json'))
        assert before == after and len(before) == 6
        assert len({r['Path'].lower() for r in before}) == 6
        assert all(re.fullmatch('[0-9A-F]{64}', r['Hash']) for r in before)
        for outcome in outcomes:
            profile = outcome['profile']
            machine, width, laa = PROFILES[profile]
            assert outcome['exit-code'] == 0 and outcome['log'] == prefix + '-' + profile + '.log'
            stem = prefix + '-' + profile
            pe = read_json(native / (stem + '-before.json'))
            assert pe == read_json(native / (stem + '-after.json'))
            assert pe['profile'] == profile and pe['machine'] == machine
            assert pe['large-address-aware'] is laa and pe['bytes'] > 0
            assert re.fullmatch('[0-9a-f]{64}', pe['sha256'])
            assert read_json(native / (stem + '-source-after.json'))['Hash'].lower() == source_hash
            build = (native / (stem + '-build.log')).read_text(errors='replace')
            assert not re.search(r'error C[0-9]+|fatal error|error LNK', build)
            lines = (native / outcome['log']).read_text().splitlines()
            assert lines[0] == f'PROFILE {profile} POINTER_BYTES {width}'
            assert lines[1] == 'USER sid=S-1-5-18 attributes=00000000 bytes=12'
            rows = [re.fullmatch(r'OPEN sample=(\d+) path=(\d+) desired=([0-9A-F]{8}) status=([0-9A-F]{8}) out=([0-9A-F]{16})', line) for line in lines[2:-1]]
            assert len(rows) == 80 and all(rows)
            seen = set()
            for row in rows:
                sample, path, desired = map(int, (row[1], row[2], str(int(row[3], 16))))
                assert sample in range(2) and path in range(8) and desired in (1, 3, 0x20019, 0x20119, 0x20219)
                key = sample, path, desired
                assert key not in seen
                seen.add(key)
                present = path in (0, 1, 5)
                assert int(row[4], 16) == (0 if present else 0xC0000034)
                assert (int(row[5], 16) != 0) == present
            summary = re.fullmatch(r'SUMMARY cases=80 handles-before=(\d+) handles-after=(\d+)', lines[-1])
            assert summary and int(summary[1]) == int(summary[2]) > 0
            open_count += len(rows)
    assert open_count == 320
    return {'key-opens': open_count, 'native-ABI-profiles': 4,
            'physical-Windows-kernels': 1, 'bound-native-files': len(names)}


if __name__ == '__main__':
    print(json.dumps(check(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent), indent=2))
