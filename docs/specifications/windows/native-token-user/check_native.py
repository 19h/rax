"""Bind native capture identity and normalize only proven output-pointer bytes."""
from pathlib import Path
import argparse
import hashlib
import itertools
import json
import re

STEM = 'native-token-user-v4'
SOURCE = '4f95a76792ed8f7feff00d400a67d0152f7b10c945c1faf5f369d9d93fb7c183'
PROFILES = {'arm64': (43620, True, 8), 'x64': (34404, True, 8),
            'x86': (332, False, 4), 'x86-laa': (332, True, 4)}
KINDS = ['primary', 'thread-token', 'effective', 'null', 'process', 'thread',
         'invalid', 'event', 'real', 'no-access', 'impersonation']
LENGTHS = list(range(65)) + [128, 256, 4096, 0xFFFFFFFF]
SPECIAL = [0, 19, 20, 21, 28, 32, 64, 4096, 0xFFFFFFFF]
SID = '010100000000000512000000'


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def fields(line, prefix):
    assert line.startswith(prefix)
    parts = line[len(prefix):].split()
    row = dict(item.split('=', 1) for item in parts)
    assert len(row) == len(parts)
    return row


def normalized(text, at, output, width, written, returned=None):
    assert len(text) % 2 == 0
    pointer = ((output + 2 * width) & ((1 << (width * 8)) - 1)).to_bytes(width, 'little')
    parts = [text[i:i + 2] for i in range(0, len(text), 2)]
    for i, pair in enumerate(parts):
        if pair != '??':
            assert re.fullmatch('[0-9A-F]{2}', pair)
        address = at + i
        if written and output <= address < output + width and not (
                returned is not None and returned <= address < returned + 4):
            index = address - output
            assert pair == f'{pointer[index]:02X}', (text, index, pair, pointer.hex())
            parts[i] = '@' + str(index)
    return ''.join(parts)


def check(root):
    native = root / 'native'
    plan = load(root / 'capture-plan-v4.json')
    assert plan['source-sha256'] == SOURCE and plan['expected-cases'] == 22688
    for row in plan['primary-sources']:
        raw = (root / row['file']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    assert hashlib.sha256((native / (STEM + '.cpp')).read_bytes()).hexdigest() == SOURCE
    assert SOURCE in (native / (STEM + '.ps1')).read_text()
    launch = load(native / (STEM + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    outcomes = load(native / (STEM + '-outcomes.json'))
    assert [r['profile'] for r in outcomes] == list(PROFILES)
    assert all(r['exit-code'] == 0 and r['log'] == STEM + '-' + r['profile'] + '.log' for r in outcomes)
    before = load(native / (STEM + '-modules-before.json'))
    assert before == load(native / (STEM + '-modules-after.json')) and len(before) == 6
    assert len({row['Path'].lower() for row in before}) == 6
    assert next(row['Hash'].lower() for row in before if row['Path'].lower().endswith('system32\\ntdll.dll')) == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
    expected = [(kind, 0, length) for kind in KINDS for length in LENGTHS]
    expected += [(kind, role, length) for kind in KINDS for role in range(43) for length in SPECIAL]
    boundary_order = list(itertools.product(range(2), range(2), [1, 2, 0x104, 0x102], range(41)))
    assert len(expected) == 5016 and len(boundary_order) == 656
    groups = []
    mapping = {}
    for profile, (machine, laa, width) in PROFILES.items():
        stem = STEM + '-' + profile
        pe = load(native / (stem + '-before.json'))
        assert pe == load(native / (stem + '-after.json'))
        assert pe['profile'] == profile and pe['machine'] == machine and pe['large-address-aware'] is laa
        assert pe['bytes'] > 0 and re.fullmatch('[a-f0-9]{64}', pe['sha256'])
        assert load(native / (stem + '-source-after.json'))['Hash'].lower() == SOURCE
        lines = (native / (stem + '.log')).read_text().splitlines()
        required, header = 2 * width + 12, 2 * width
        assert lines[0] == f'reference class=1 status=C0000023 value=A5A5A5A5 bytes={required}'
        assert lines[1] == f'user-reference required={required} header={header} sid-offset={header} attributes=00000000 sid-bytes=12 sid={SID}'
        assert lines[2] == f'identity profile={profile} pointer-bytes={width} build=10.0.29683 page=4096 cases=5016'
        cases, boundaries = [], []
        for line in lines[3:]:
            if line.startswith('case '):
                row = fields(line, 'case ')
                ordinal = len(cases)
                assert int(row['ordinal']) == ordinal
                assert (row['kind'], int(row['role']), int(row['length'], 16)) == expected[ordinal]
                assert row['escaped'] == '00000000'
                assert row['last-error'] == ('00000008' if width == 4 and row['status'] == 'C0000017' else '7E11ABCD')
                assert len(row['payload']) == 2 * int(row['payload-bytes'])
                output, returned = int(row['output'], 16), int(row['returned'], 16)
                pointer = ((output + header) & ((1 << (width * 8)) - 1)).to_bytes(width, 'little').hex().upper()
                # A complete pointer is observable before a later ReturnLength
                # fault. Successful WoW64 aliases may overwrite its bytes last.
                written = row['status'] == '00000000' or row['payload'].startswith(pointer)
                last = returned if width == 4 and returned != 0 and row['status'] == '00000000' else None
                item = {key: row[key] for key in ['kind', 'role', 'length', 'status', 'last-error', 'output-protect', 'returned-protect']}
                item['output-cells'] = normalized(row['output-bytes-cells'], output, output, width, written, last)
                item['return-cells'] = normalized(row['return-bytes-cells'], returned, output, width, written, last)
                item['payload'] = normalized(row['payload'], output, output, width, written, last)
                cases.append(item)
            elif line.startswith('boundary '):
                row = fields(line, 'boundary ')
                assert int(row['ordinal']) == len(boundaries)
                assert (int(row['subject']), int(row['side']), int(row['protection'], 16), int(row['remaining'])) == boundary_order[len(boundaries)]
                assert row['escaped'] == '00000000' and len(row['cells']) == 128
                output = int(row['output'], 16)
                item = {key: row[key] for key in ['subject', 'side', 'remaining', 'protection', 'status', 'first-after', 'second-after', 'returned']}
                item['cells'] = normalized(row['cells'], output, output, width, row['status'] == '00000000')
                boundaries.append(item)
            elif line.startswith('boundary-complete '):
                assert line == 'boundary-complete cases=656'
            elif line.startswith('complete '):
                match = re.fullmatch(r'complete cases=5016 closed-handles=4 released-arenas=1 handles-before=(\d+) handles-after=(\d+)', line)
                assert match and match[1] == match[2]
            else:
                raise AssertionError(line)
        assert len(cases) == 5016 and len(boundaries) == 656
        assert lines[-2] == 'boundary-complete cases=656' and lines[-1].startswith('complete ')
        group = {'cases': cases, 'boundaries': boundaries}
        if profile in ['x64', 'x86-laa']:
            previous = groups[0 if profile == 'x64' else 1]
            assert group == previous, 'paired ABI profile differs after pointer-only normalization: ' + profile
            mapping[profile] = 0 if profile == 'x64' else 1
        else:
            mapping[profile] = len(groups)
            groups.append(group)
    result = {'native-main-cases': 20064, 'native-boundary-cases': 2624,
              'native-total-cases': 22688, 'abi-profiles': 4, 'native-kernels': 1,
              'reference-user-sid': SID, 'mapping': mapping, 'groups': groups,
              'scope': 'Captured SYSTEM subject. Opaque result pointers are represented by byte-index markers; statuses, byte counts, padding, aliases and fault-visible data remain native observations.'}
    corpus = root / 'cases.json'
    if corpus.exists():
        assert load(corpus) == result
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    result = check(args.evidence)
    if args.write:
        path = args.evidence / 'cases.json'
        assert not path.exists(), 'Do not overwrite an existing corpus.'
        path.write_text(json.dumps(result, separators=(',', ':')) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'groups'}, indent=2))
