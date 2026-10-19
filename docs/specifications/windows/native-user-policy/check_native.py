"""Check independent native observations without importing product code."""
from pathlib import Path
import hashlib
import json
import re
import sys


PROFILES = {'arm64': (43620, 8, True), 'x64': (34404, 8, True),
            'x86': (332, 4, False), 'x86-laa': (332, 4, True)}
SOURCES = {'native-user-policy-v3': '3a52bb10baf1ff9d8b206166bce84690839f67456ca26a885b83b0fd599e80ee',
           'native-user-environment-v1': '049ff8e072a66406c44413224bfcb a5d17ebce27fd524e8418059b591429f62d'.replace(' ', ''),
           'native-sid-text-v1': '36a05ab559ebbd048c59d244cd811030e42473fbf728b2f1ead047e0aa4afaad'}


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
    assert len(names) == 92
    native = root / 'native'
    open_count = sid_count = 0
    canonical = None
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
            if prefix != 'native-sid-text-v1':
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
                    present = prefix == 'native-user-environment-v1' and path in (0, 1, 5)
                    assert int(row[4], 16) == (0 if present else 0xC0000034)
                    assert (int(row[5], 16) != 0) == present
                summary = re.fullmatch(r'SUMMARY cases=80 handles-before=(\d+) handles-after=(\d+)', lines[-1])
                assert summary and int(summary[1]) == int(summary[2]) > 0
                open_count += len(rows)
            else:
                assert lines[-1] == 'SUMMARY cases=128' and len(lines) == 129
                authorities = [0, 1, 5, 0xFFFFFFFF, 0x100000000, 0x010203040506, 0x123456789ABC, 0xFFFFFFFFFFFF]
                values = [0, 0xFFFFFFFF, 0x80000000, 0x12345678]
                captured = []
                for index, line in enumerate(lines[:-1]):
                    row = re.fullmatch(r'SID profile=(\S+) authority=(\d+) count=(\d+) ok=1 error=00000000 text=(S-1-\S+)', line)
                    assert row and row[1] == profile
                    authority, count = divmod(index, 16)
                    assert (int(row[2]), int(row[3])) == (authority, count)
                    sid = bytes((1, count)) + authorities[authority].to_bytes(6, 'big')
                    sid += b''.join(values[i % 4].to_bytes(4, 'little') for i in range(count))
                    captured.append({'sid': sid.hex(), 'text': row[4]})
                if canonical is None:
                    canonical = captured
                assert captured == canonical
                sid_count += len(captured)
    corpus = read_json(root / 'sid-text-cases.json')
    assert corpus == {'native-profiles': 4, 'observations': 512, 'cases': canonical}
    assert open_count == 640 and sid_count == 512
    assert canonical[64]['text'] == 'S-1-0x100000000'
    assert canonical[80]['text'] == 'S-1-0x10203040506'
    return {'key-opens': open_count, 'sid-format-observations': sid_count,
            'distinct-sid-format-cases': len(canonical), 'native-ABI-profiles': 4,
            'physical-Windows-kernels': 1, 'bound-native-files': len(names)}


if __name__ == '__main__':
    print(json.dumps(check(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent), indent=2))
