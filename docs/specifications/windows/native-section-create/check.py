from pathlib import Path
import argparse
import hashlib
import json
import re


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path, row):
    data = path.read_bytes()
    require(len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256'], str(path))


def check(root):
    require(__debug__, 'optimized validation rejected')
    manifest = json.loads((root / 'native-artifacts.json').read_text())
    names = [row['path'] for row in manifest['artifacts']]
    require(len(names) == len(set(names)), 'unique artifact paths')
    for row in manifest['artifacts']:
        require(Path(row['path']).name == row['path'], 'artifact path grammar')
        identity(root / row['path'], row)
    total = 0
    closed = 0
    for group, cases, handles in [('priority', 1386, 91), ('valid-cross', 1344, 288)]:
        stem = 'native-section-create-' + group
        record = json.loads((root / (stem + '-identities.json')).read_text())
        require(record['kernel'][:3] == [10, 0, 29683], 'native kernel identity')
        require(len(record['producers']) == 4, 'producer count')
        require({row['profile'] for row in record['producers']} == {'arm64', 'x64', 'x86', 'x86-laa'}, 'exact profiles')
        identity(root / ('inspect-section-create-' + group + '.py'), record['inspector'])
        for row in record['producers']:
            profile = row['profile']
            require(row['controls'] == cases and row['closed-handles'] == handles, 'inspected count')
            identity(root / (stem + '.cpp'), row['source'])
            require(row['executable']['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'PE machine')
            require(bool(row['executable']['characteristics'] & 0x20) == (profile != 'x86'), 'PE large-address-awareness')
            log = root / (stem + '-' + profile + '.log')
            identity(log, row['stdout'])
            lines = log.read_text().splitlines()
            headers = [line for line in lines if line.startswith('case=')]
            require(len(headers) == cases, 'control count')
            parsed = [dict(re.findall(r'([\w-]+)=([^ ]+)', line)) for line in headers]
            require([int(item['case']) for item in parsed] == list(range(cases)), 'unique ordered case ordinals')
            require({int(item['width']) for item in parsed} == {4 if profile.startswith('x86') else 8}, 'actual pointer width')
            require(sum(int(item['status'], 16) < 0x80000000 for item in parsed) == handles, 'successful handles')
            require(sum(line == 'close status=00000000' for line in lines) == handles, 'released handles')
            require(lines[-1] == f'complete cases={cases} closed-handles={handles}', 'producer completion')
            require(not any(line.startswith('failure=') for line in lines), 'producer failure')
            for name, size in [('output', 32), ('maximum', 16)]:
                captures = [line for line in lines if line.startswith('bytes name=' + name + ' ')]
                require(len(captures) == cases, 'complete caller captures')
                require(all(len(bytes.fromhex(line.split('hex=')[1])) == size for line in captures), 'caller capture width')
            if group == 'priority':
                original = parsed[:231]
                require({int(item['access'], 16) for item in original if item['group'] == 'access-bit'} == {1 << bit for bit in range(32)}, 'complete access one-hots')
                require({int(item['object-flags'], 16) for item in original if item['group'] == 'object-flag-bit'} == {1 << bit for bit in range(32)}, 'complete object one-hots')
                for stage in range(1, 6):
                    forced = [('protection', '00000000'), ('attributes', '00000000'), ('maximum', '0'), ('maximum-pointer', '2'), ('output', '1')][stage - 1]
                    for index, item in enumerate(parsed[stage * 231:(stage + 1) * 231]):
                        expected = dict(original[index])
                        expected[forced[0]] = forced[1]
                        for key in ['group', 'width', 'maximum', 'access', 'protection', 'attributes', 'output', 'maximum-pointer', 'object', 'file', 'object-flags']:
                            require(item[key] == expected[key], 'independent forced-priority inputs')
            total += cases
            closed += handles
    original = json.loads((root / 'native-section-create-first-call-identities.json').read_text())
    require(original['executable']['machine'] == 0xAA64 and original['original-native-child-exit'] == 41, 'original native child')
    require((original['max-size-bytes'], original['protection'], original['allocation-attributes'], original['desired-access']) == (65536, 4, 0x08000000, 0xF001F), 'original seven-argument shape')
    identity(root / 'native-section-create-first-call.cpp', original['source'])
    identity(root / 'native-section-create-first-call-arm64.log', original['stdout'])
    identity(root / 'inspect-section-create-first-call.py', original['inspector'])
    text = (root / 'native-section-create-first-call-arm64.log').read_text()
    require(re.search(r'^entry ordinal=0 target=1 .*args=[0-9A-F]+,F001F,0,[0-9A-F]+,4,8000000,0,$', text, re.M), 'original unchanged arguments')
    require('return ordinal=0 class=0 status=00000000' in text and text.splitlines()[-1] == 'complete captured=1 calls=1 exit=41', 'original unchanged status and natural exit')
    require((total, closed) == (10920, 1516), 'aggregate native counts')
    print('Verified9 actual producers,10920 direct creations,1516 closed handles,original7args and natural native child exit41')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus', type=Path, default=Path(__file__).resolve().parent)
    check(parser.parse_args().corpus)
