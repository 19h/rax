"""Validate complete native matrices independently of guest implementation."""
from pathlib import Path
import hashlib
import itertools
import json
import re

ROOT = Path(__file__).resolve().parent
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
DESCRIPTORS = ['empty-dacl', 'allow-one', 'deny-one-allow-three', 'absent-dacl',
               'null-dacl', 'missing-owner-group', 'captured-absolute-nonmember']
TOKENS = ['primary', 'thread-token', 'effective', 'process', 'thread', 'null',
          'invalid', 'real-primary', 'no-access', 'impersonation']
DESIRED = [0, 1, 2, 3, 0x02000000, 0x80000000, 0x00040000, 0x00020000, 0x01000000]
LENGTHS = [0, 1, 4, 8, 20, 56, 64]

def need(condition, message):
    if not condition:
        raise ValueError(message)

def load(name):
    return json.loads((ROOT / name).read_text(encoding='utf-8-sig'))

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def records(text):
    return [dict(value.split('=', 1) for value in line.split()[1:])
            for line in text.splitlines() if line.startswith('case ')]

def canonical(row):
    return {key: value for key, value in row.items()
            if key not in ['ordinal', 'escaped', 'last-error']}

def main():
    need(__debug__, 'optimized verification is not supported')
    names = set()
    for item in load('inputs.json')['files']:
        name = item['path']
        need(name not in names and not Path(name).is_absolute() and '..' not in Path(name).parts,
             'unique relative native evidence paths')
        names.add(name)
        p = ROOT / name
        need(p.stat().st_size == item['bytes'] and digest(p) == item['sha256'], name)
    fixture = load('cases.json')
    for family, prefix, count in [('v4', 'native-access-check-v4', 4410),
                                  ('boundaries', 'native-access-check-boundaries', 1500)]:
        base = ROOT / 'native' / family
        identities = json.loads((base / (prefix + '-identities.json')).read_text(encoding='utf-8-sig'))
        need(len(identities['producers']) == 4, 'four independently inspected producers')
        matrices = []
        descriptor_rows = []
        for index, (profile, producer) in enumerate(zip(PROFILES, identities['producers'])):
            need(producer['profile'] == profile and producer['machine'] == [0xAA64, 0x8664, 0x14C, 0x14C][index]
                 and producer['large-address-aware'] == (profile != 'x86'), 'producer ABI')
            source = base / (prefix + '.cpp')
            need(digest(source) == producer['source']['sha256'], 'independently inspected source')
            log = base / (prefix + '-' + profile + '.log')
            need(digest(log) == producer['stdout']['sha256'] and log.stat().st_size == producer['stdout']['bytes'], 'raw stdout identity')
            text = log.read_text(encoding='utf-8-sig')
            rows = records(text)
            need(len(rows) == count and [int(r['ordinal']) for r in rows] == list(range(count)), 'complete ordinal sequence')
            need(all(r['escaped'] == '00000000' and r['last-error'] == '7E11ABCD' for r in rows), 'no escaping exception or changed last error')
            need(producer['calls'] == count and producer['escaped-exceptions'] == 0
                 and producer['closed-handles'] == 3, 'native outcome and cleanup')
            if family == 'v4':
                expected = list(itertools.product(DESCRIPTORS, TOKENS, DESIRED, LENGTHS))
                actual = [(r['descriptor'], r['token'], int(r['desired'], 16), int(r['length'])) for r in rows]
                need(actual == expected, 'full logical Cartesian matrix')
                need(all(len(r['privileges']) == 128 for r in rows), '64 privilege bytes')
                need('complete cases=4410 closed-handles=3 released-descriptors=6' in text, 'logical cleanup trailer')
                descriptors = {}
                for line in text.splitlines():
                    if line.startswith('descriptor '):
                        r = dict(v.split('=', 1) for v in line.split()[1:])
                        if r['name'] != 'captured-absolute-nonmember':
                            descriptors[r['name']] = r['raw']
                descriptor_rows.append(descriptors)
            else:
                expected = list(itertools.product(['primary', 'thread-token', 'effective', 'invalid', 'no-access', 'event'],
                                                  [0, 8, 20, 56, 0xFFFFFFFF], range(50)))
                actual = [(r['token'], int(r['length'], 16), int(r['role'])) for r in rows]
                need(actual == expected, 'full pointer/protection Cartesian matrix')
                need(all(all(re.fullmatch(r'(?:[0-9A-F]{2}|\?\?){8}', r[f'p{i}-cells']) for i in range(6)) for r in rows), 'eight captured byte cells per pointer')
                need('complete cases=1500 closed-handles=3 released-descriptors=1 released-arenas=1' in text, 'boundary cleanup trailer')
            matrices.append([canonical(r) for r in rows])
        if family == 'v4':
            need(all(m == matrices[0] for m in matrices) and fixture['logical'] == matrices[0], 'four logical profiles and compiled fixture')
            need(all(r == descriptor_rows[0] for r in descriptor_rows) and load('descriptors.json') == descriptor_rows[0], 'six relative descriptors are byte-identical')
        else:
            need(matrices[0] == matrices[1] and matrices[2] == matrices[3], 'within-ABI pointer agreement')
            need(sum(a != b for a, b in zip(matrices[0], matrices[2])) == 108, '108 observed WoW64 conversion differences')
            need(fixture['native64-boundaries'] == matrices[0] and fixture['wow64-boundaries'] == matrices[2], 'both compiled boundary fixtures')
    subject = load('native-access-check-subject.json')
    need(subject['stable-captures'] == 2 and subject['closed-handles'] == 1
         and subject['token-access'] == 8 and fixture['subject'] == subject['subject'], 'stable captured primary subject')
    need(subject['source-sha256'] == digest(ROOT / 'capture-access-subject.py'), 'subject producer identity')
    private = load('native-access-privileges-v2.json')
    need(private['machine'] == 'ARM64' and private['pointer-bytes'] == 8
         and private['closed-handles'] == 5 and private['released-descriptors'] == 16, 'private native-token scope and cleanup')
    need(private['source-sha256'] == digest(ROOT / 'native-access-privileges-v2.py'), 'private token producer identity')
    expected = list(itertools.product(['none', 'security', 'take-ownership', 'both'],
        ['owned-empty', 'world-empty', 'foreign-empty', 'world-null'],
        [0, 1, 0x20000, 0x40000, 0x80000, 0x1000000, 0x1080000, 0x2000000, 0x3000000],
        [0, 8, 19, 20, 31, 32, 44, 56, 64]))
    rows = private['rows']
    need([(r['token'], r['descriptor'], r['desired'], r['capacity']) for r in rows] == expected, 'complete private-token matrix')
    need(all(r['ordinal'] == i and len(r['privileges']) == 128 and r['last-error'] == 0x7E11ABCD for i, r in enumerate(rows)), 'private outputs and last error')
    alignment = load('native-access-descriptor-alignment.json')
    need(alignment['source-sha256'] == digest(ROOT / 'native-access-descriptor-alignment.py')
         and alignment['released-descriptors'] == 1 and len(alignment['rows']) == 16, 'alignment producer and cleanup')
    need(all(r['offset'] == i and r['return'] == (0 if i % 4 == 0 else 0x80000002)
             and r['last-error'] == 0x7E11ABCD for i, r in enumerate(alignment['rows'])), 'four-byte native alignment')
    translation = load('native-access-status-map.json')
    need(translation['source-sha256'] == digest(ROOT / 'native-access-status-map.py'), 'translation producer identity')
    need([(r['status'], r['error'], r['last-error']) for r in translation['rows']] ==
         [(0xC000005C, 1309, 0x7E11ABCD), (0xC0000079, 1338, 0x7E11ABCD), (0xC00000E6, 1360, 0x7E11ABCD)], 'native RTL translations')
    capture = load('captured-inputs.json')
    need(digest(ROOT / 'capture/native-access-check-capture.log') == capture['trace-sha256'], 'ordinary pre-call input trace')
    need((capture['desired-access'], capture['token-pseudo-handle'], capture['privilege-set-length']) == (1, -4, 56), 'actual continuation inputs')
    need(capture['acl']['bytes'] == 8 + 52 + 176, 'recorded ACL capacity arithmetic')
    print('Verified 23640 four-profile native calls, 1296 ARM64 private-token calls, alignment/RTL controls and compiled fixtures; guest validation is separate')

if __name__ == '__main__':
    main()
