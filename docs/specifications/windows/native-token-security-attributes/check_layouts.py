"""Independent PHNT-layout/native-capture checker; imports no product code."""
from pathlib import Path
import json
import hashlib
import runpy
import struct

HERE = Path(__file__).resolve().parent
CAPTURES = HERE / "native" if (HERE / "native").is_dir() else HERE
ORDER = runpy.run_path(str(HERE / 'derive-query-order.py'))['predict']
TSA = 'TSA://ProcUnique'


def rows(path):
    return [dict(part.split('=', 1) for part in line.split())
            for line in path.read_text().splitlines() if line.startswith('case=')]


def fixtures(kind):
    values = {
        1: [-1, -(1 << 63), (1 << 63) - 1],
        2: [0, 0x0123456789ABCDEF, (1 << 64) - 1],
        3: ['a', '\u03a9\u20ac'],
        4: [(0x0123456789ABCDEF, 'fixture')],
        5: [bytes.fromhex('010100000000000512000000')],
        6: [0, 1],
        16: [bytes.fromhex('00017F80FF')],
    }
    return ('Assist.Fixture', kind, 2 if kind in [3, 4] else 0, values[kind])


def pack(attributes, width, base, capacity, overflow=False):
    """Explicit measured layouts, independently encoded from captured values."""
    header, stride, us = (16, 40, 16) if width == 8 else (12, 24, 8)
    data = bytearray(capacity)
    cursor = header + stride * len(attributes)

    def put(at, value):
        if at + len(value) <= capacity:
            data[at:at + len(value)] = value

    def ptr(at, target):
        put(at, (base + target).to_bytes(width, 'little'))

    def string(at, text):
        nonlocal cursor
        cursor += (-(base + cursor)) % 2
        value = text.encode('utf-16le')
        put(at, struct.pack('<HH', len(value), len(value)))
        ptr(at + (8 if width == 8 else 4), cursor)
        put(cursor, value)
        cursor += len(value)

    complete = True
    for i, (name, kind, flags, values) in enumerate(attributes):
        at = header + stride * i
        string(at, name)
        put(at + us, struct.pack('<HHII', kind, 0, flags, len(values)))
        cursor += (-(base + cursor)) % 8
        values_at = cursor
        if kind in [1, 2, 6]:
            value = b''.join((n & ((1 << 64) - 1)).to_bytes(8, 'little') for n in values)
            cursor += len(value)
            if cursor <= capacity:
                ptr(at + stride - width, values_at)
                put(values_at, value)
            else:
                complete = False
        else:
            element = us if kind == 3 else (8 + us if kind == 4 else 2 * width)
            cursor += element * len(values)
            ptr(at + stride - width, values_at)
            for j, value in enumerate(values):
                entry = values_at + element * j
                if kind == 3:
                    string(entry, value)
                elif kind == 4:
                    put(entry, value[0].to_bytes(8, 'little'))
                    string(entry + 8, value[1])
                else:
                    ptr(entry, cursor)
                    put(entry + width, len(value).to_bytes(4, 'little'))
                    put(cursor, value)
                    cursor += len(value)
    if complete and not overflow:
        put(0, struct.pack('<HHI', 1, 0, len(attributes)))
        if attributes:
            ptr(8, header)
    return bytes(data), cursor


def tsa_values(records, width):
    row = next(r for r in records if r['status'] == '00000000')
    data, base = bytes.fromhex(row['bytes']), int(row['output'], 16)
    pointer_at = 16 + 32 if width == 8 else 12 + 20
    at = int.from_bytes(data[pointer_at:pointer_at + width], 'little') - base
    return [int.from_bytes(data[at + i * 8:at + (i + 1) * 8], 'little') for i in range(2)]


def main():
    checked = 0
    failures = []
    profiles = []
    source_hashes = {
        'v1': '87f8c3af4bbc60c8f68bb06e86376bbbc8c218c19ecb6196bf8b308991147a43',
        'v2': '9817133450d8fa848159116aeab334a3327395d1139fd2cb520f7c74c426480c',
        'v3': 'f6ee6a26d8f23ec2deaa03ee71ab6fa3a525f5dd601ab567bb7b4d1254bedecd',
        'v4b': '961e27623cee97542c1b24a89816b4d0cb140f9018f75f37d69e601b1849ada7',
        'v5': '27875c004d769cddcdafa7ad137af433f8e1e7674bb0c23e9b87458a05a7eca6',
        'v9': '6a132c1d2fe27dfdabc2f031f2cf5505a31b267ea4dc1a32495589e786c4c2db',
        'v7b': '711bdee3db5e5da0203120a4c65fcd6da970db7bbcf7cd55b2db69d47c38dcb8',
        'v6': '70df6a77a6b2ea156532ca8bca5a9f89e10cbb276804d7e29672ced335f174d4',
    }
    def load(name):
        raw = (CAPTURES / name).read_bytes()
        return json.loads(raw.decode('utf-16' if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else 'utf-8-sig'))
    modules = None
    for version, count in [('v1', 78), ('v2', 672), ('v3', 2562), ('v4b', 56), ('v5', 992), ('v6', 1152), ('v7b', 8), ('v9', 144)]:
        prefix = f'native-token-security-attributes-{version}'
        assert hashlib.sha256((CAPTURES / (prefix + '.cpp')).read_bytes()).hexdigest() == source_hashes[version]
        assert load(prefix + '-launch.json')['state'] == 'finished'
        assert load(prefix + '-launch.json')['exit-code'] == 0
        outcomes = load(prefix + '-outcomes.json')
        assert [r['profile'] for r in outcomes] == ['arm64', 'x64', 'x86', 'x86-laa']
        assert all(r['exit-code'] == 0 for r in outcomes)
        before = load(prefix + '-modules-before.json')
        assert before == load(prefix + '-modules-after.json')
        if modules is None:
            modules = before
        assert before == modules
        for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
            pe = load(prefix + '-' + profile + '-before.json')
            assert pe == load(prefix + '-' + profile + '-after.json')
            assert pe['profile'] == profile
            assert pe['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile]
            assert pe['large-address-aware'] == (profile != 'x86')
            assert load(prefix + '-' + profile + '-source-after.json')['Hash'].lower() == source_hashes[version]
            path = CAPTURES / f'native-token-security-attributes-{version}-{profile}.log'
            records = rows(path)
            text = path.read_text()
            assert f'complete cases={count}' in text
            if version in ['v4b', 'v7b']:
                setup = [dict(part.split('=', 1) for part in line.split()[1:]) for line in text.splitlines() if line.startswith('fixture ')]
                assert [int(row['type']) for row in setup] == ([0] if version == 'v7b' else [1, 2, 3, 4, 5, 6, 16])
                for setup_row in setup:
                    assert int(setup_row['width']) == (4 if profile.startswith('x86') else 8)
                    assert int(setup_row['attribute-bytes']) == (24 if profile.startswith('x86') else 40)
                    assert int(setup_row['information-bytes']) == (12 if profile.startswith('x86') else 16)
                    assert int(setup_row['fqbn-bytes']) == (16 if profile.startswith('x86') else 24)
                    assert int(setup_row['octet-bytes']) == (8 if profile.startswith('x86') else 16)
                assert all(row['set'] == '00000000' and row['exception'] == '00000000' for row in setup)
                assert f"successful-types={1 if version == 'v7b' else 7} primary-privileges-unchanged=1" in text
            assert len(records) == count
            assert [int(r['case']) for r in records] == list(range(count))
            width = 4 if profile.startswith('x86') else 8
            opaque = None if version in ['v4b', 'v7b', 'v9'] else tsa_values(records, width)
            for row in records:
                assert int(row['width']) == width
                assert row.get('exception', '00000000') == '00000000'
                assert row['last-error'] == '0000C0DE'
                base, length = int(row['output'], 16), int(row['length'], 16)
                actual = bytes.fromhex(row['bytes'])
                if version in ['v3', 'v5', 'v6', 'v9']:
                    status, regions, detail = ORDER(row)
                    for field, key, pages in [('input-protect', 'input', 4), ('out-protect', 'out-base', 2), ('ret-protect', 'ret-base', 2)]:
                        address = int(row[key], 16)
                        assert [regions[address + 4096 * i] for i in range(pages)] == [int(v, 16) for v in row[field].split(',')]
                    nm, rm = int(row['names-mode']), int(row['returned-mode'])
                    attrs = [(TSA, 2, 0x41, opaque)] * (2 if nm == 20 else 1)
                    expected = bytearray(b'\xA5' * len(actual))
                    out_base = int(row['out-base'], 16)
                    if rm in [7, 8] and base == out_base:
                        offset = 4 if rm == 8 else 0
                        expected[offset:offset + 4] = struct.pack('<I', 0x11223344)
                    returned = 0x11223344
                    valid_out = out_base <= base < out_base + 8192
                    if rm in [7, 8] and valid_out and base != out_base:
                        returned = 0xA5A5A5A5
                    if 'returned' in detail:
                        returned = detail['returned']
                    if version == 'v6' and detail.get('packing') and width == 4 and status not in [0, 0x80000005]:
                        # Independent memset store hypothesis: first unaligned
                        # vector, aligned body, then the final unaligned pair.
                        offset = (-base) % 16 or 16
                        stores = [0] + list(range(offset, length - 32, 16)) + [length - 32, length - 16]
                        om = int(row['output-mode'])
                        boundary = int(row['out-base'], 16) + 4096
                        for at in stores:
                            if om in [9, 11] or (om in [8, 10] and base + at + 16 > boundary):
                                break
                            start, end = min(at, len(actual)), min(at + 16, len(actual))
                            expected[start:end] = bytes(end - start)
                    if detail.get('packing') and status in [0, 0x80000005]:
                        value, converted = pack(attrs, width, base, length, detail.get('overflow', False))
                        expected[:min(length, len(actual))] = value[:len(actual)]
                        if width == 4:
                            returned = converted
                    if rm in [7, 8] and 'returned' in detail:
                        offset = 4 if rm == 8 else 0
                        expected[offset:offset + 4] = struct.pack('<I', returned)
                    if rm in [7, 8] and 'returned' not in detail and valid_out:
                        offset = 4 if rm == 8 else 0
                        returned = int.from_bytes(expected[offset:offset + 4], 'little')
                    assert row['readable'] == ('0' if rm in [1, 2] or (rm in [7, 8] and not valid_out) else '1')
                    if row['readable'] == '0':
                        returned = 0x11223344
                    field = 'returned'
                else:
                    attrs = [] if version == 'v7b' else [fixtures(int(row['type']))] if version == 'v4b' else [(TSA, 2, 0x41, opaque)] * (2 if row['mode'] == '9' else 1)
                    native, needed = pack(attrs, 8, 0, 65536)
                    mode = int(row.get('mode', 0))
                    if width == 8 and length == 0:
                        status, returned = 0xC000000D, 0x11223344
                    elif version not in ['v4b', 'v7b'] and mode in [1, 12, 13]:
                        status, returned = 0xC000000D, 0
                    elif version not in ['v4b', 'v7b'] and mode not in [0, 7, 8, 9]:
                        status, returned = 0xC0000225, 0
                    elif length < needed:
                        status, returned = 0xC0000023, needed
                    else:
                        status = 0
                        _, returned = pack(attrs, width, base, length)
                        if width == 8:
                            returned = needed
                    expected = bytearray(b'\xA5' * len(actual))
                    if status == 0:
                        value, _ = pack(attrs, width, base, length)
                        end = 16 if version == 'v7b' and width == 8 else min(length, len(actual))
                        expected[:end] = value[:end]
                    field = 'required'
                checks = [status == int(row['status'], 16), returned == int(row[field], 16), bytes(expected) == actual]
                if not all(checks):
                    differences = [i for i, (a, b) in enumerate(zip(expected, actual)) if a != b]
                    failures.append({'version': version, 'profile': profile, 'case': row['case'], 'checks': checks, 'status': f'{status:08X}', 'returned': f'{returned:08X}', 'expected-returned': row[field], 'byte-differences': differences[:20]})
                checked += 1
            profiles.append({'version': version, 'profile': profile, 'cases': count})
    assert checked == 22656
    corpus_path = HERE / 'cases.json'
    if corpus_path.exists():
        corpus = json.loads(corpus_path.read_text())
        assert corpus['total-cases'] == 22048 and len(corpus['groups']) == 24
        assert [(g['version'], g['profile']) for g in corpus['groups']] == [(v, p) for v in ['v1', 'v2', 'v3', 'v4b', 'v5', 'v6'] for p in ['arm64', 'x64', 'x86', 'x86-laa']]
        for group in corpus['groups']:
            records = rows(CAPTURES / f"native-token-security-attributes-{group['version']}-{group['profile']}.log")
            assert group['cases'] == records
            if group['version'] != 'v4b':
                width = 4 if group['profile'].startswith('x86') else 8
                assert group['opaque'] == tsa_values(records, width)
    empty = json.loads((HERE / 'empty-cases.json').read_text())
    assert empty['total-cases'] == 32
    assert [g['profile'] for g in empty['groups']] == ['arm64', 'x64', 'x86', 'x86-laa']
    for group in empty['groups']:
        assert group['cases'] == rows(CAPTURES / f"native-token-security-attributes-v7b-{group['profile']}.log")
    upper = json.loads((HERE / 'upper-cases.json').read_text())
    assert upper['total-cases'] == 576
    assert [g['profile'] for g in upper['groups']] == ['arm64', 'x64', 'x86', 'x86-laa']
    for group in upper['groups']:
        assert group['cases'] == rows(CAPTURES / f"native-token-security-attributes-v9-{group['profile']}.log")
    zero_source = CAPTURES / 'native-token-security-attributes-v8.cpp'
    assert hashlib.sha256(zero_source.read_bytes()).hexdigest() == '33ba9267dd95a5c04a800106880426458af4ae31b26834793d0b48025c1ba0a5'
    assert load('native-token-security-attributes-v8-launch.json')['exit-code'] == 0
    zero_outcomes = load('native-token-security-attributes-v8-outcomes.json')
    assert [row['profile'] for row in zero_outcomes] == ['arm64', 'x64', 'x86', 'x86-laa']
    assert all(row['exit-code'] == 0 for row in zero_outcomes)
    for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
        prefix = f'native-token-security-attributes-v8-{profile}'
        assert load(prefix + '-before.json') == load(prefix + '-after.json')
        assert load(prefix + '-source-after.json')['Hash'].lower() == '33ba9267dd95a5c04a800106880426458af4ae31b26834793d0b48025c1ba0a5'
        text = (CAPTURES / (prefix + '.log')).read_text()
        setup = [dict(part.split('=', 1) for part in line.split()[1:]) for line in text.splitlines() if line.startswith('fixture ')]
        assert [int(row['type']) for row in setup] == [1, 2, 3, 4, 5, 6, 16]
        assert all(row['set'] == 'C000000D' and row['exception'] == '00000000' for row in setup)
        assert 'complete cases=0 successful-types=0 primary-privileges-unchanged=1' in text
    result = {'checked': checked, 'zero-value-creation-controls': 28, 'profiles': profiles, 'failures': failures}
    (HERE / 'native-layout-check.json').write_text(json.dumps(result, indent=2) + '\n')
    print('checked', checked, 'failures', len(failures))
    for failure in failures[:20]:
        print(failure)
    return int(bool(failures))


if __name__ == '__main__':
    raise SystemExit(main())
