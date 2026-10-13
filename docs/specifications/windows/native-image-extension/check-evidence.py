#!/usr/bin/env python3
"""Independent partitions, native identity, ABI controls and fresh entry proof."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig'))


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def partition(family, profile):
    bits = 32 if profile.startswith('x86') else 64
    if family == 'query-probe':
        for target, kind, flags, length in itertools.product(
                range(10), [0, 1, 2, 3, 0x80000000, 0xffffffff],
                [0, 1, 2, 3, 0xffffffff], [0, 1, 4, 8, 12, 15, 16, 17, 20, 23, 24, 25, 32, 64]):
            yield {'target': target, 'type': kind, 'flags': flags, 'length': length}
    elif family == 'boundaries-v2':
        for length, output, returned, target, selector in itertools.product(
                [0, 23, 24, 25, 4097], range(12), range(10), range(5), range(3)):
            yield dict(family=0, process=0, target=target, length=length,
                       output=output, returned=returned, selector=selector)
        for process, length, output, returned, selector in itertools.product(
                range(1, 10), [23, 24], range(12), range(10), range(3)):
            yield dict(family=1, process=process, target=0, length=length,
                       output=output, returned=returned, selector=selector)
    elif family == 'limits':
        maximum = (1 << bits) - 1
        for length, output, returned, target in itertools.product(
                [0, 1, 23, 24, 25, 4096, 4097, 0x7fffffff, 0x80000000,
                 0xffffffff, maximum - 23, maximum - 7, maximum],
                [0, 1, 2, 3, 6, 12, 13], [0, 1, 5, 8], [0, 3, 4]):
            yield dict(family=2, process=0, target=target, length=length,
                       output=output, returned=returned, selector=0)
    else:
        raise ValueError('unknown native family')


def main(directory):
    root = Path(directory)
    identity = read_json(root / 'native-image-extension-final-producer-identities.json')
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    families = ['query-probe', 'boundaries-v2', 'limits']
    expected = {(family, profile) for family in families for profile in profiles} | {('fresh', 'arm64')}
    inventory = [(item['family'], item['profile']) for item in identity['producers']]
    require(len(inventory) == len(expected) and set(inventory) == expected, 'independent producer inventory')
    require(identity['kernel'][:3] == [10, 0, 29683], 'captured native kernel')
    require('S-1-5-18' in identity['principal'], 'captured native principal')
    dlls = [(item['bytes'], item['sha256'], item['machine']) for item in identity['native-libraries']]
    require(dlls == [
        (4609312, '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f', 0xaa64),
        (1849976, 'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e', 0x14c)
    ], 'captured installed native DLL identities')
    captured = {}
    total = 0
    for item in identity['producers']:
        family, profile = item['family'], item['profile']
        for filename, key in [(item['source'], 'source-identity'), (item['log'], 'stdout-identity')]:
            raw = (root / filename).read_bytes()
            require(len(raw) == item[key]['bytes'] and hashlib.sha256(raw).hexdigest() == item[key]['sha256'],
                    'actual compiled source binding' if key == 'source-identity' else 'actual stdout binding')
        pe = item['executable-identity']
        require(pe['machine'] == {'arm64': 0xaa64, 'x64': 0x8664, 'x86': 0x14c, 'x86-laa': 0x14c}[profile], 'actual PE machine')
        require(bool(pe['characteristics'] & 0x20) == (profile != 'x86'), 'actual PE LAA')
        require(pe['bytes'] > 0 and len(pe['sha256']) == 64, 'actual native executable identity')
        lines = (root / item['log']).read_text().splitlines()
        if family == 'fresh':
            validate_fresh(lines)
            outcome = read_json(root / 'native-image-extension-fresh-arm64-outcome.json')
            require(outcome['exit-code'] == 0 and outcome['exe-sha256'].lower() == pe['sha256'], 'actual native producer outcome')
            continue
        outcomes = read_json(root / ('native-image-extension-' + ('query' if family == 'query-probe' else family) + '-outcomes.json'))
        outcome = next((entry for entry in outcomes if entry['profile'] == profile), None)
        require(outcome is not None and outcome['exit-code'] == 0 and
                outcome['source-hash'].lower() == item['source-identity']['sha256'] and
                outcome['exe-sha256'].lower() == pe['sha256'] and outcome['exe-bytes'] == pe['bytes'],
                'actual native producer outcome')
        rows = [fields(line) for line in lines if line.startswith('case=')]
        independent = list(partition(family, profile))
        require(len(rows) == len(independent), 'independent input partition cardinality')
        for ordinal, (row, inputs) in enumerate(zip(rows, independent)):
            require(int(row['case']) == ordinal, 'independent input ordinal')
            for key, value in inputs.items():
                radix = 16 if (family == 'query-probe' and key in ['type', 'flags']) or (family != 'query-probe' and key == 'length') else 10
                require(int(row[key], radix) == value, 'independent input partition differs')
            require(int(row['exception'], 16) == 0, 'native caller exception')
            if family == 'query-probe':
                require(len(bytes.fromhex(row['bytes'])) == 64, 'native matrix output extent')
            else:
                require(len(row['protection'].rstrip(',').split(',')) == 4, 'native protection extent')
                for name, size in [('bytes0', 160), ('bytes1', 96), ('bytes2', 48)]:
                    require(len(bytes.fromhex(row[name])) == size, 'native boundary output extent')
        complete = [fields(line) for line in lines if line.startswith('complete ')]
        require(len(complete) == 1 and int(complete[0]['cases']) == len(independent), 'native complete partition')
        if family != 'query-probe':
            require(complete[0]['handles-before'] == complete[0]['handles-after'], 'native query handle restoration')
        captured[(family, profile)] = rows
        total += len(rows)
    require(total == 83088, 'independent mature row total')
    for family in families:
        for left, right in [('arm64', 'x64'), ('x86', 'x86-laa')]:
            for a, b in zip(captured[(family, left)], captured[(family, right)]):
                keys = ['status', 'returned', 'bytes'] if family == 'query-probe' else ['status', 'protection', 'bytes0', 'bytes1', 'bytes2']
                if family == 'query-probe' and left == 'arm64' and int(a['target']) in [0, 2, 3]:
                    keys = ['status', 'returned']  # Native SCP presence differs by caller personality.
                require(all(a[key] == b[key] for key in keys), 'same-width native semantic agreement')
    verify_primary(root)
    verify_manifest(root)
    print(json.dumps({'actual-producers': 13, 'families': 3, 'independent-native-rows': total,
                      'portable-replay-rows': total - 2520, 'present-extension-controls': 2520,
                      'fresh-complete-cases': 2, 'same-width-agreement': True}))


def verify_primary(root):
    sources = read_json(root / 'references/sources.json')
    expected = {
        'phnt-ntmmapi.h': 'https://raw.githubusercontent.com/winsiderss/phnt/master/ntmmapi.h',
        'phnt-LICENSE': 'https://raw.githubusercontent.com/winsiderss/phnt/master/LICENSE',
        'microsoft-ntqueryvirtualmemory.html': 'https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryvirtualmemory',
    }
    require(len(sources) == len(expected) and {row['file'] for row in sources} == set(expected),
            'primary reference inventory')
    for row in sources:
        raw = (root / 'references' / row['file']).read_bytes()
        require(row['url'] == expected[row['file']] and len(raw) == row['bytes'] and
                hashlib.sha256(raw).hexdigest() == row['sha256'], 'primary reference binding')
    header = (root / 'references/phnt-ntmmapi.h').read_text()
    require('#define MemoryImageExtensionInformation 0xE' in header and
            all(field in header for field in ['MemoryImageExtensionCfgScp',
                'MemoryImageExtensionCfgEmulatedScp', 'MemoryImageExtensionTypeMax',
                'ExtensionType', 'ExtensionImageBaseRva', 'ExtensionSize']),
            'primary private query declaration')


def verify_manifest(root):
    manifest = read_json(root / 'evidence-manifest.json')
    files = {str(path.relative_to(root)) for path in root.rglob('*') if path.is_file() and
             '__pycache__' not in path.parts and path.name != 'evidence-manifest.json'}
    require(set(manifest) == files, 'complete evidence file inventory')
    for name, binding in manifest.items():
        raw = (root / name).read_bytes()
        require(len(raw) == binding['bytes'] and hashlib.sha256(raw).hexdigest() == binding['sha256'], 'exact evidence file hash')


def validate_fresh(lines):
    require(any(line == 'complete cases=2 warmup=1' for line in lines), 'fresh complete partition')
    cases = [fields(line) for line in lines if line.startswith('case=')]
    images = [fields(line) for line in lines if line.startswith('image ')]
    entries = [fields(line) for line in lines if line.startswith('entry ') and ',E,' in fields(line)['args']]
    returns = [fields(line) for line in lines if line.startswith('return ') and fields(line)['call'] == '2']
    require(len(cases) == len(images) == len(entries) == len(returns) == 3, 'fresh original-entry/true-LR cardinality')
    for ordinal, (case, image, entry, result) in enumerate(zip(cases, images, entries, returns)):
        require(all(int(row['case']) == ordinal for row in [case, image, entry, result]), 'fresh case ordinal')
        require(case['warmup'] == str(int(ordinal == 0)) and case['installed'] == case['done'] == '1' and
                case['exit'] == '77' and case['calls'] == '3' and
                (ordinal == 0 or case['exact-handle-set'] == '1' and case['handles-before'] == case['handles-after']), 'fresh capture lifetime')
        all_entries = [fields(line) for line in lines if line.startswith('entry ') and
                       int(fields(line)['case']) == ordinal]
        all_returns = [fields(line) for line in lines if line.startswith('return ') and
                       int(fields(line)['case']) == ordinal]
        require([row['call'] for row in all_entries] == ['0', '1', '2'] and
                [row['call'] for row in all_returns] == ['0', '1', '2'] and
                [int(row['args'].split(',')[2], 16) for row in all_entries] == [6, 4, 14],
                'fresh startup query partition')
        for before, after in zip(all_entries, all_returns):
            require(before['function'] == after['function'] == '35' and
                    before['thread'] == after['thread'] == entry['thread'] and
                    int(before['pc'], 16) == int(image['entry'], 16) + 4 and
                    int(after['pc'], 16) == int(before['lr'], 16) + 4 and
                    after['status'] == '00000000', 'fresh thread/function/true-LR binding')
        code = bytes.fromhex(image['bytes'])
        instruction = int.from_bytes(code[:4], 'little')
        require(int(image['type'], 16) == 0x1000000 and instruction & 0xffe0001f == 0xd4000001 and
                (instruction >> 5) & 0xffff == 35 and code[4:8] == bytes.fromhex('C0035FD6'), 'fresh live MEM_IMAGE SVC')
        args = [int(value, 16) for value in entry['args'].rstrip(',').split(',')]
        require(len(args) == 6 and args[0] == (1 << 64) - 1 and args[1] == int(image['allocation'], 16) and
                args[2] == 14 and args[4] == 24, 'fresh actual query inputs')
        require(int(entry['function']) == 35 and int(entry['pc'], 16) == int(image['entry'], 16) + 4 and
                int(result['pc'], 16) == int(entry['lr'], 16) + 4 and result['status'] == '00000000', 'fresh true LR return')
        chunks = {fields(line)['phase']: fields(line) for line in lines if line.startswith('bytes ') and int(fields(line)['case']) == ordinal}
        require(set(chunks) == {'output-before', 'returned-before', 'output-after', 'returned-after'}, 'fresh query byte phases')
        for phase, size, address in [('output-before', 24, args[3]), ('output-after', 24, args[3]), ('returned-before', 8, args[5]), ('returned-after', 8, args[5])]:
            require(int(chunks[phase]['size']) == size and len(bytes.fromhex(chunks[phase]['data'])) == size and
                    int(chunks[phase]['address'], 16) == address, 'fresh query byte extent/address binding')
        require(bytes.fromhex(chunks['output-before']['data']) == bytes(24) and
                bytes.fromhex(chunks['returned-before']['data']) == bytes(8) and
                bytes.fromhex(chunks['output-after']['data']) == bytes.fromhex('000000000000000000304700000000000010000000000000') and
                int.from_bytes(bytes.fromhex(chunks['returned-after']['data']), 'little') == 24, 'fresh actual extension payload')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus', default=str(Path(__file__).parent))
    main(parser.parse_args().corpus)
