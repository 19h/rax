"""Verify the fixed-key native observation and its complete producer identity."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re

PROFILES = {'arm64': (0xAA64, True, 8), 'x64': (0x8664, True, 8),
            'x86': (0x14C, False, 4), 'x86-laa': (0x14C, True, 4)}
SOURCE_SHA = '35dcaad6900a013062d5ca8fe5a174c0f6b91e63fcc70bf7b6b6dac8e4bfe8c8'
SOURCE = 'native-redirection-keys-v1'


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in (b'\xff\xfe', b'\xfe\xff')
                      else raw.decode('utf-8-sig'))


def check(root):
    assert hashlib.sha256((root / (SOURCE + '.cpp')).read_bytes()).hexdigest() == SOURCE_SHA
    assert SOURCE_SHA in (root / (SOURCE + '.ps1')).read_text()
    launch = load(root / (SOURCE + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    outcomes = load(root / (SOURCE + '-outcomes.json'))
    assert [row['profile'] for row in outcomes] == list(PROFILES)
    assert all(row['exit-code'] == 0 and row['log'] == SOURCE + '-' + row['profile'] + '.log'
               for row in outcomes)
    before = load(root / (SOURCE + '-modules-before.json'))
    after = load(root / (SOURCE + '-modules-after.json'))
    assert before == after and len(before) == 6
    assert len({row['Path'].lower() for row in before}) == 6
    ntdll = next(row for row in before if row['Path'].lower().endswith('system32\\ntdll.dll'))
    assert ntdll['Hash'].lower() == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
    total = 0
    expected = set(itertools.product(range(2), range(8), [1, 0x20019, 0x20119, 0x20219]))
    for profile, (machine, laa, pointer_bytes) in PROFILES.items():
        stem = SOURCE + '-' + profile
        pe = load(root / (stem + '-before.json'))
        assert pe == load(root / (stem + '-after.json'))
        assert pe['profile'] == profile and pe['machine'] == machine
        assert pe['large-address-aware'] is laa and pe['bytes'] > 0
        assert re.fullmatch('[0-9a-f]{64}', pe['sha256'])
        assert load(root / (stem + '-source-after.json'))['Hash'].lower() == SOURCE_SHA
        lines = (root / (stem + '.log')).read_text().splitlines()
        assert lines[0] == f'PROFILE {profile} POINTER_BYTES {pointer_bytes}'
        rows = []
        for line in lines[1:-1]:
            match = re.fullmatch(r'OPEN sample=(\d+) path=(\d+) desired=([0-9A-F]{8}) '
                                 r'status=([0-9A-F]{8}) out=([0-9A-F]{16})', line)
            assert match, line
            sample, path, desired, status, output = match.groups()
            rows.append((int(sample), int(path), int(desired, 16)))
            assert status == 'C0000034' and int(output, 16) == 0
        assert len(rows) == 64 and len(set(rows)) == 64 and set(rows) == expected
        summary = re.fullmatch(r'SUMMARY cases=64 handles-before=(\d+) handles-after=(\d+)', lines[-1])
        assert summary and summary[1] == summary[2]
        total += len(rows)
    assert total == 256
    return {'profiles': 4, 'native-kernels': 1, 'cases': total, 'selected-root-cases': 224,
            'outside-selected-root-observations': 32,
            'scope': 'Root and descendants are selected; native path=4 is an outside-prefix control, which remains an unsupported guest frontier.'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.root), indent=2))


if __name__ == '__main__':
    main()
