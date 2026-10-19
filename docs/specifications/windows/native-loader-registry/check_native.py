"""Independent fixed-root native capture coverage, statuses, and identity."""
from pathlib import Path
import argparse
import hashlib
import itertools
import json
import re

PROFILES = {'arm64': (0xAA64, True, 8), 'x64': (0x8664, True, 8),
            'x86': (0x14C, False, 4), 'x86-laa': (0x14C, True, 4)}
FAMILIES = [
    ('native', 'native-safeboot-option-v1', '09be645592eb967b620fbb8a7b3240499f9bad037f02c8cba1f0fa220980ce99', False),
    ('srp-native', 'native-srp-gp-dll-v1', '4b6649a6a155984239ca60343b5d03e99b60de7dc9bbb4b9f40930ab62ff2653', False),
    ('safer-native', 'native-safer-code-identifiers-v1', '14a8a3c6edb84cf4fd5c3bff928a839ffcb413c501848942f9408f8d97e6efda', True),
]


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def check(root):
    report = {'abi-profiles': 4, 'native-kernels': 1, 'roots': [], 'native-cases': 0,
              'scope': 'Exact captured subject: SafeBoot and SRP DLL absent; Safer present. Native success handles are opaque identities. Guest immutable snapshots retain their inherited read-only grant policy, including denial of write-intent access.'}
    expected = set(itertools.product(range(2), range(8), [1, 3, 0x20019, 0x20119, 0x20219]))
    for folder, source, digest, present in FAMILIES:
        base = root / folder
        assert hashlib.sha256((base / (source + '.cpp')).read_bytes()).hexdigest() == digest
        assert digest in (base / (source + '.ps1')).read_text()
        launch = load(base / (source + '-launch.json'))
        assert launch['state'] == 'finished' and launch['exit-code'] == 0
        outcomes = load(base / (source + '-outcomes.json'))
        assert [r['profile'] for r in outcomes] == list(PROFILES)
        assert all(r['exit-code'] == 0 and r['log'] == source + '-' + r['profile'] + '.log' for r in outcomes)
        before = load(base / (source + '-modules-before.json'))
        assert before == load(base / (source + '-modules-after.json')) and len(before) == 6
        assert len({r['Path'].lower() for r in before}) == 6
        ntdll = next(r for r in before if r['Path'].lower().endswith('system32\\ntdll.dll'))
        assert ntdll['Hash'].lower() == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
        counts = {'success': 0, 'missing': 0}
        for profile, (machine, laa, width) in PROFILES.items():
            stem = source + '-' + profile
            pe = load(base / (stem + '-before.json'))
            assert pe == load(base / (stem + '-after.json'))
            assert pe['profile'] == profile and pe['machine'] == machine
            assert pe['large-address-aware'] is laa and pe['bytes'] > 0
            assert re.fullmatch('[a-f0-9]{64}', pe['sha256'])
            assert load(base / (stem + '-source-after.json'))['Hash'].lower() == digest
            lines = (base / (stem + '.log')).read_text().splitlines()
            assert lines[0] == f'PROFILE {profile} POINTER_BYTES {width}'
            seen = []
            for line in lines[1:-1]:
                match = re.fullmatch(r'OPEN sample=(\d+) path=(\d+) desired=([0-9A-F]{8}) status=([0-9A-F]{8}) out=([0-9A-F]{16})', line)
                assert match, line
                sample, index, desired, status, handle = match.groups()
                sample, index, desired, status, handle = int(sample), int(index), int(desired, 16), int(status, 16), int(handle, 16)
                seen.append((sample, index, desired))
                success = present and index in [0, 1, 5]
                assert status == (0 if success else 0xC0000034), (stem, line)
                assert (handle != 0) == success, (stem, line)
                counts['success' if success else 'missing'] += 1
            assert len(seen) == 80 and len(set(seen)) == 80 and set(seen) == expected
            summary = re.fullmatch(r'SUMMARY cases=80 handles-before=(\d+) handles-after=(\d+)', lines[-1])
            assert summary and summary[1] == summary[2]
        assert counts == ({'success': 120, 'missing': 200} if present else {'success': 0, 'missing': 320})
        report['roots'].append({'producer': source, 'present': present, 'cases': 320, **counts})
        report['native-cases'] += 320
    assert report['native-cases'] == 960
    report.update({'selected-root-observations': 840, 'outside-prefix-controls': 120})
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.evidence), indent=2))
