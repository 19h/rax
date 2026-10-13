#!/usr/bin/env python3
"""Independent constructor partition, ownership, fresh entry and source binding."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def read_json(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig'))


def fields(line):
    return dict(token.split('=', 1) for token in line.split() if '=' in token)


def partition():
    return itertools.product(range(13), [0, 1, 3, 4, 7, 8, 9, 12, 16, 32, 0xffffffff],
                             range(14), range(2))


def expected(width, process, length, output):
    """Observed build29683 contract; distinct from the Rust dispatch path."""
    if width == 8 and length:
        if output in [2, 3]:
            return 0x80000002, 0
        if output == 13:
            return 0xc0000005, 0
    if length != width:
        return 0xc0000004, 0
    if process in [1, 3]:
        return 0xc0000008, 0
    if process in [2, 12]:
        return 0xc0000024, 0
    if process not in [0, 5, 11]:
        return 0xc0000022, 0
    births = int(width == 4)
    if output in [1, 2, 5, 6, 8, 13] or width == 8 and output in [9, 11]:
        return 0xc0000005, births
    if output == 7 or width == 8 and output == 10:
        return 0x80000001, births
    return 0, 1


def check_row(row, ordinal, inputs, width):
    process, length, output, content = inputs
    require(int(row['case']) == ordinal, 'independent input ordinal')
    require([int(row[k], 16 if k == 'length' else 10) for k in
             ['process', 'length', 'output', 'content']] == list(inputs),
            'independent input partition differs')
    require(int(row['width']) == width and int(row['exception'], 16) == 0,
            'native caller ABI or exception')
    status, births = expected(width, process, length, output)
    require(int(row['status'], 16) == status, 'native constructor status precedence')
    require(int(row['born-count']) == births, 'native constructor birth count')
    born = [item.split(':') for item in row['born'].split(',') if item]
    require(len(born) == births and all(len(item) == 5 and item[1:] ==
            ['00000000', '00000001', '1', '0'] for item in born), 'native typed handle metadata')
    require(all(int(item[0], 16) != 0 for item in born), 'native live handle identity')
    require(row['restored'] == '1' and row['handles-before'] == row['handles-after'],
            'native exact handle restoration')
    protection = {5: 2, 6: 1, 7: 0x104, 8: 0x102, 9: 1, 10: 0x104, 11: 2, 12: 1}.get(output, 4)
    if status == 0x80000001:
        protection &= ~0x100
    require(row['protection'] == '00000004,' + format(protection, '08X') + ',',
            'native guard protection ordering')
    data = bytearray([0xa5] * 8192)
    offset = {0: 128, 3: 129, 4: 132, 5: 4096, 6: 4096, 7: 4096,
              8: 4096, 9: 4092, 10: 4092, 11: 4092, 12: 4088}.get(output)
    if offset is not None:
        data[offset:offset + 8] = bytes([0xff if content else 0] * 8)
    if status == 0:
        require(offset is not None and len(born) == 1, 'native successful handle publication')
        data[offset:offset + width] = int(born[0][0], 16).to_bytes(width, 'little')
    require(bytes.fromhex(row['bytes0']) == data[120:152] and
            bytes.fromhex(row['bytes1']) == data[4080:4128], 'native exact constructor bytes')


def check_fresh(lines):
    require('complete cases=2 warmup=1' in lines, 'fresh complete partition')
    require([sum(line.startswith(prefix) for line in lines) for prefix in
             ['case=', 'image ', 'entry ', 'return ', 'bytes ', 'owner ']] == [3, 3, 6, 6, 6, 3],
            'fresh original-entry/true-LR cardinality')
    for ordinal in range(3):
        rows = [(line.split()[0], fields(line)) for line in lines
                if fields(line).get('case') == str(ordinal)]
        groups = {}
        for kind, row in rows:
            groups.setdefault(kind, []).append(row)
        require([len(groups.get(kind, [])) for kind in ['image', 'entry', 'return', 'bytes', 'owner',
                'case=' + str(ordinal)]] == [1, 2, 2, 2, 1, 1],
                'fresh original-entry/true-LR cardinality')
        image = groups['image'][0]
        entry_address = int(image['entry'], 16)
        require(image['type'] == '1000000' and int(image['allocation'], 16) <= entry_address and
                bytes.fromhex(image['bytes'])[:8] == bytes.fromhex('810300D4C0035FD6'),
                'fresh live MEM_IMAGE SVC')
        entries, returns = groups['entry'], groups['return']
        for index, (entry, result) in enumerate(zip(entries, returns)):
            args = [int(value, 16) for value in entry['args'].rstrip(',').split(',')]
            require(entry['call'] == result['call'] == str(index) and
                    args[0] == 0xffffffffffffffff and
                    [args[1], args[3]] == [[53, 16], [112, 8]][index],
                    'fresh startup constructor partition')
            require(entry['function'] == result['function'] == '28' and
                    entry['thread'] == result['thread'] and int(entry['thread']) > 0 and
                    int(entry['pc'], 16) == entry_address + 4 and
                    int(result['pc'], 16) == int(entry['lr'], 16) + 4 and
                    result['status'] == '00000000', 'fresh thread/function/true-LR binding')
        before, after = groups['bytes']
        owner = groups['owner'][0]
        output = int(entries[1]['args'].rstrip(',').split(',')[2], 16)
        require(before['phase'] == 'output-before' and after['phase'] == 'output-after' and
                int(before['address'], 16) == int(after['address'], 16) == output and
                before['size'] == after['size'] == '8' and before['data'] == '0000000000000000' and
                int.from_bytes(bytes.fromhex(after['data']), 'little') == int(owner['handle'], 16),
                'fresh actual handle publication')
        require(owner['grant'] == '00000001' and owner['type-match'] == owner['duplicate-closed'] == '1',
                'fresh actual typed handle ownership')
        case = groups['case=' + str(ordinal)][0]
        require(case['warmup'] == str(int(ordinal == 0)) and case['installed'] == case['done'] == '1' and
                case['exit'] == '77' and case['calls'] == '2' and
                (ordinal == 0 or case['exact-handle-set'] == '1' and
                 case['handles-before'] == case['handles-after']), 'fresh capture lifetime')


def check_primary(root):
    expected_urls = {
        'phnt-ntpsapi.h': 'https://raw.githubusercontent.com/winsiderss/phnt/master/ntpsapi.h',
        'phnt-LICENSE': 'https://raw.githubusercontent.com/winsiderss/phnt/master/LICENSE'}
    rows = read_json(root / 'references/sources.json')
    require(len(rows) == 2 and {r['file'] for r in rows} == set(expected_urls), 'primary reference inventory')
    for row in rows:
        raw = (root / 'references' / row['file']).read_bytes()
        require(row['url'] == expected_urls[row['file']] and len(raw) == row['bytes'] and
                hashlib.sha256(raw).hexdigest() == row['sha256'], 'primary reference binding')
    header = (root / 'references/phnt-ntpsapi.h').read_text()
    enum = header.split('typedef enum _PROCESSINFOCLASS', 1)[1].split('} PROCESSINFOCLASS', 1)[0]
    names = [line.strip().split(',', 1)[0] for line in enum.splitlines()
             if line.strip().startswith('Process') and ',' in line]
    require(names.index('ProcessSchedulerSharedData') == 112, 'primary class number')
    declaration = next(line for line in header.splitlines() if 'ProcessSchedulerSharedData,' in line)
    require('q: SCHEDULER_SHARED_DATA_SLOT_INFORMATION' in declaration, 'primary query-comment conflict retained')


def check_manifest(root):
    manifest = read_json(root / 'evidence-manifest.json')
    paths = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file() and
             p.name != 'evidence-manifest.json' and '__pycache__' not in p.parts}
    require(set(manifest) == paths, 'complete evidence file inventory')
    for name, binding in manifest.items():
        raw = (root / name).read_bytes()
        require(len(raw) == binding['bytes'] and hashlib.sha256(raw).hexdigest() == binding['sha256'],
                'exact evidence file hash')


def main(directory):
    root = Path(directory)
    identity = read_json(root / 'native-scheduler-shared-producer-identities.json')
    inspector = identity['inspector-identity']
    raw = (root / inspector['path'].replace('\\', '/').split('/')[-1]).read_bytes()
    require(len(raw) == inspector['bytes'] and hashlib.sha256(raw).hexdigest() == inspector['sha256'],
            'actual native inspector binding')
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    inventory = [(item['family'], item['profile']) for item in identity['producers']]
    require(len(inventory) == 5 and set(inventory) == {('create', p) for p in profiles} |
            {('fresh-v2', 'arm64')}, 'independent producer inventory')
    require(identity['kernel'][:3] == [10, 0, 29683] and 'S-1-5-18' in identity['principal'],
            'captured native kernel and principal')
    require([(r['bytes'], r['sha256'], r['machine']) for r in identity['native-libraries']] == [
        (4609312, '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f', 0xaa64),
        (1849976, 'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e', 0x14c)],
        'captured installed native DLL identities')
    total = births = 0
    for item in identity['producers']:
        family, profile = item['family'], item['profile']
        for name, key in [(item['source'], 'source-identity'), (item['log'], 'stdout-identity')]:
            raw = (root / name).read_bytes()
            require(len(raw) == item[key]['bytes'] and hashlib.sha256(raw).hexdigest() == item[key]['sha256'],
                    'actual compiled source binding' if key == 'source-identity' else 'actual stdout binding')
        pe = item['executable-identity']
        require(pe['machine'] == {'arm64': 0xaa64, 'x64': 0x8664, 'x86': 0x14c, 'x86-laa': 0x14c}[profile],
                'actual PE machine')
        require(bool(pe['characteristics'] & 0x20) == (profile != 'x86'), 'actual PE LAA')
        lines = (root / item['log']).read_text().splitlines()
        if family == 'fresh-v2':
            check_fresh(lines)
            outcome = read_json(root / 'native-scheduler-shared-fresh-v2-arm64-outcome.json')
        else:
            outcome = next(r for r in read_json(root / 'native-scheduler-shared-create-outcomes.json')
                           if r['profile'] == profile)
            rows = [fields(line) for line in lines if line.startswith('case=')]
            inputs = list(partition())
            require(len(rows) == len(inputs) == 4004, 'independent input partition cardinality')
            width = 4 if profile.startswith('x86') else 8
            for ordinal, (row, axes) in enumerate(zip(rows, inputs)):
                check_row(row, ordinal, axes, width)
                births += int(row['born-count'])
            require('complete cases=4004 exact-handle-set=1 owned-input-handles-closed=9' in lines,
                    'native complete lifetime partition')
            total += len(rows)
        require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == item['source-identity']['sha256'] and
                outcome['exe-sha256'].lower() == pe['sha256'] and outcome['exe-bytes'] == pe['bytes'],
                'actual native producer outcome')
    require(total == 16016 and births == 204, 'independent mature totals')
    check_primary(root)
    check_manifest(root)
    print(json.dumps({'actual-producers': 5, 'independent-native-rows': total, 'native-owned-births': births,
                      'fresh-complete-cases': 2, 'status-byte-guard-ownership-check': 'pass'}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus', default=str(Path(__file__).parent))
    main(parser.parse_args().corpus)
