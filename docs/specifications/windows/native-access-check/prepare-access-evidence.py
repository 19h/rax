"""Derive replay inputs from independently recorded native observations."""
from pathlib import Path
import hashlib, itertools, json, re, shutil

B = Path(__file__).resolve().parent
D = B / 'evidence-draft'
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
MACHINES = [0xAA64, 0x8664, 0x14C, 0x14C]
DESCRIPTORS = ['empty-dacl', 'allow-one', 'deny-one-allow-three', 'absent-dacl',
               'null-dacl', 'missing-owner-group', 'captured-absolute-nonmember']
TOKENS = ['primary', 'thread-token', 'effective', 'process', 'thread', 'null',
          'invalid', 'real-primary', 'no-access', 'impersonation']
DESIRED = [0, 1, 2, 3, 0x02000000, 0x80000000, 0x00040000, 0x00020000, 0x01000000]
LENGTHS = [0, 1, 4, 8, 20, 56, 64]
BOUNDARY_TOKENS = ['primary', 'thread-token', 'effective', 'invalid', 'no-access', 'event']
BOUNDARY_LENGTHS = [0, 8, 20, 56, 0xFFFFFFFF]

def need(condition, message):
    if not condition:
        raise ValueError(message)

def read_records(path):
    raw = path.read_bytes()
    text = raw.decode('utf-8-sig')
    rows = [dict(item.split('=', 1) for item in line.split()[1:])
            for line in text.splitlines() if line.startswith('case ')]
    need([int(x['ordinal']) for x in rows] == list(range(len(rows))), 'ordinal sequence')
    need(all(x['escaped'] == '00000000' and x['last-error'] == '7E11ABCD' for x in rows),
         'escaping exception or changed last error')
    return raw, text, rows

def canonical(row):
    return {key: value for key, value in row.items() if key not in ['ordinal', 'escaped', 'last-error']}

def main():
    D.mkdir(exist_ok=True)
    files = []
    def copy(path, relative):
        target = D / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        data = path.read_bytes()
        if target.exists():
            need(target.read_bytes() == data, 'refuse evidence replacement ' + relative)
        else:
            target.write_bytes(data)
        files.append({'path': relative, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    matrices = {}
    for family, prefix, count in [('v4', 'native-access-check-v4', 4410),
                                   ('boundaries', 'native-access-check-boundaries', 1500)]:
        source = B / 'probes' / family
        identity = json.loads((source / (prefix + '-identities.json')).read_text())
        need(len(identity['producers']) == 4, 'four independently inspected producers')
        normalized = []
        for profile, machine, producer in zip(PROFILES, MACHINES, identity['producers']):
            log = source / (prefix + '-' + profile + '.log')
            raw, text, rows = read_records(log)
            need(len(rows) == count, 'complete matrix')
            need(producer['profile'] == profile and producer['machine'] == machine
                 and producer['large-address-aware'] == (profile != 'x86'), 'actual producer ABI')
            need(producer['stdout']['sha256'] == hashlib.sha256(raw).hexdigest()
                 and producer['stdout']['bytes'] == len(raw), 'independent raw identity')
            need(producer['calls'] == count and producer['escaped-exceptions'] == 0
                 and producer['closed-handles'] == 3, 'independent calls and cleanup')
            if family == 'v4':
                expected = itertools.product(DESCRIPTORS, TOKENS, DESIRED, LENGTHS)
                actual = ((x['descriptor'], x['token'], int(x['desired'], 16), int(x['length'])) for x in rows)
                need(list(actual) == list(expected), 'complete logical Cartesian matrix')
                need('complete cases=4410 closed-handles=3 released-descriptors=6' in text, 'logical cleanup')
                need(all(len(x['privileges']) == 128 for x in rows), '64 privilege bytes')
            else:
                expected = itertools.product(BOUNDARY_TOKENS, BOUNDARY_LENGTHS, range(50))
                actual = ((x['token'], int(x['length'], 16), int(x['role'])) for x in rows)
                need(list(actual) == list(expected), 'complete pointer Cartesian matrix')
                need('complete cases=1500 closed-handles=3 released-descriptors=1 released-arenas=1' in text, 'boundary cleanup')
                need(all(all(re.fullmatch(r'(?:[0-9A-F]{2}|\?\?){8}', x[f'p{i}-cells']) for i in range(6)) for x in rows), 'eight captured cells')
            normalized.append([canonical(x) for x in rows])
        if family == 'v4':
            need(all(rows == normalized[0] for rows in normalized), 'logical ABI agreement')
            matrices['logical'] = normalized[0]
        else:
            need(normalized[0] == normalized[1] and normalized[2] == normalized[3], 'within-ABI boundary agreement')
            need(sum(a != b for a, b in zip(normalized[0], normalized[2])) == 108, 'observed ABI conversion differences')
            matrices['native64-boundaries'] = normalized[0]
            matrices['wow64-boundaries'] = normalized[2]
        for path in sorted(source.iterdir()):
            need(path.is_file(), 'flat retained native evidence')
            copy(path, 'native/' + family + '/' + path.name)
    for name in ['native-access-check-subject.json', 'capture-access-subject.py', 'subject-capture-controller.json',
                 'captured-inputs.json', 'captured-shape-native-results.json', 'prepare-access-evidence.py']:
        copy(B / name, name)
    for path in sorted((B / 'capture').iterdir()):
        if path.is_file():
            copy(path, 'capture/' + path.name)
    subject = json.loads((B / 'native-access-check-subject.json').read_text())
    need(subject['stable-captures'] == 2 and subject['closed-handles'] == 1
         and subject['token-access'] == 8 and subject['subject']['user']['sid'] == '010100000000000512000000',
         'fresh controlled subject identity')
    source = (B / 'capture-access-subject.py').read_bytes()
    need(subject['source-sha256'] == hashlib.sha256(source).hexdigest(), 'subject producer identity')
    fixture = {'scope': 'four producer ABIs on one Windows10.0.29683 ARM64 kernel',
               'logical-native-calls': 17640, 'boundary-native-calls': 6000,
               'subject': subject['subject'], **matrices}
    (D / 'cases.json').write_text(json.dumps(fixture, indent=2) + '\n')
    (D / 'inputs.json').write_text(json.dumps({'files': files}, indent=2) + '\n')
    print('Derived', len(matrices['logical']), 'logical and', len(matrices['native64-boundaries']) + len(matrices['wow64-boundaries']), 'boundary rows from 23640 native calls')

if __name__ == '__main__':
    main()
