"""Rebuild the exact replay corpus from bounded native copy observations."""
from pathlib import Path
import hashlib
import json
import re

D = Path(__file__).resolve().parent
C = D / 'copy-native'
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
SOURCE = {
    'native-bno-copy-boundaries.cpp': '83189ac043b406bcac2782e05bf14e3556c16d80ed8898aff7620309238d657e',
    'native-bno-copy-child.cpp': '0c8c05094853ef5f7aeaa71565dbf1b03a22b40c573fdf95bb533f702d543ba8',
    'native-bno-copy-curve-child.cpp': 'fb6ac40384206b54bb0ea7f1050b1130d3e0bcf3a8bce8f28d95a6cc906d3149',
}


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


need(__debug__, 'optimized verification rejected')
for row in load(D / 'native-inputs.json')['files']:
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'native input identity: ' + row['path'])
for name, digest in SOURCE.items():
    need(hashlib.sha256((C / name).read_bytes()).hexdigest() == digest,
         'native copy producer source')
post = load(C / 'bno-copy-native-post-identities.json')['files']
need(len(post) == 15, 'native copy post-run identity count')
paths = {r['path'].replace('\\', '/'): r for r in post}
need(len(paths) == len(post), 'unique native copy post-run identities')
for path, row in paths.items():
    name = path.split('/')[-1]
    if name.endswith('.cpp'):
        need(row['sha256'] == SOURCE[name] and row['bytes'] == (C / name).stat().st_size,
             'native copied source stayed unchanged')
    else:
        need(name in ['probe.exe', 'controller.exe'], 'unexpected native artifact')
        if name == 'probe.exe':
            profile = path.split('/')[-2].removeprefix('bno-copy-')
            need(profile in PROFILES, 'native probe profile')
            stem = 'bno-copy-' + profile
        else:
            profile = 'arm64'
            stem = ('bno-copy-curve' if 'bno-copy-curve-controller' in path
                    else 'bno-copy-controller')
        before, after = [load(C / (stem + '-' + suffix + '.json'))
                         for suffix in ['before', 'after']]
        need(before == after and row['sha256'] == before['sha256']
             and row['bytes'] == before['bytes'], 'native copy executable stayed unchanged')
        need(before['machine'] == (332 if profile.startswith('x86') else
                                  43620 if profile == 'arm64' else 34404)
             and before['large-address-aware'] == (profile != 'x86'),
             'native copy executable ABI')

runs = []
for stem, count in [('bno-copy', 12), ('bno-copy-curve', 34)]:
    launch = load(C / (stem + '-launch.json'))
    need(launch['state'] == 'finished' and launch['exit-code'] == 0,
         'native copy driver completion')
    outcomes = load(C / (stem + '-outcomes.json'))
    need(len(outcomes) == count and all(row['exit-code'] == 0 for row in outcomes),
         'native copy child completion')
    expected_keys = ({(p, m) for p in PROFILES for m in ['ascii', 'unicode', 'long']}
                     if stem == 'bno-copy' else
                     {(p, n) for p in ['x86', 'x86-laa'] for n in
                      [8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 66, 67, 127, 128, 129, 134, 135]})
    need({(r['profile'], r.get('mode', r.get('length'))) for r in outcomes} == expected_keys,
         'native copy profile and length coverage')
    for outcome in outcomes:
        lines = (C / outcome['log']).read_text().splitlines()
        control, reference = fields(lines[0]), fields(lines[2])
        width = 4 if outcome['profile'].startswith('x86') else 8
        need(lines[1] == 'child-create ok=1 error=0' and control['enabled'] == '1'
             and reference['enabled'] == '1' and int(reference['pointer-bytes']) == width,
             'native child isolation and pointer width')
        units = control['prefix-utf16']
        need(len(units) % 4 == 0, 'native UTF-16 prefix width')
        prefix = [int(units[i:i + 4], 16) for i in range(0, len(units), 4)]
        required = 2 * width + 2 * (len(prefix) + 1)
        need(int(control['prefix-units']) == int(reference['prefix-units']) == len(prefix)
             and int(reference['required']) == required, 'native exact record length')
        cases, keys = [], []
        for line in lines:
            if not line.startswith('copy-case '):
                continue
            row = fields(line)
            remaining = int(row['remaining'])
            need(int(row['ordinal']) == len(cases) and row['escaped'] == '00000000'
                 and row['last-error'] == '7E11ABCD', 'native ordinal and exception boundary')
            keys.append((remaining, int(row['protection'], 16), int(row['length'], 16)))
            cells = [row['prefix'][i:i + 2] for i in range(0, len(row['prefix']), 2)]
            need(len(cells) == remaining and len(row['prefix']) == 2 * remaining,
                 'native complete pre-boundary output bytes')
            pointer = (int(row['output'], 16) + 2 * width).to_bytes(width, 'little')
            if remaining >= width and row['prefix'][:2 * width] == pointer.hex().upper():
                cells[:width] = ['@' + str(i) for i in range(width)]
            cases.append({key: row[key] for key in
                          ['remaining', 'protection', 'after', 'length', 'status', 'returned']}
                         | {'output': ''.join(cells)})
        expected = {(r, p, n) for r in range(required + 33)
                    for p in [1, 2, 0x104, 0x102] for n in [required, required + 16, 0xffffffff]}
        need(len(keys) == len(set(keys)) == len(expected) and set(keys) == expected,
             'native complete offset, protection and capacity matrix')
        need(lines[-2] == f'copy-complete cases={len(cases)} closed-handles=1 released-arenas=1'
             and lines[-1] == 'child-complete wait=00000000 queried=1 exit=0 closed=1',
             'native child and fixture lifetime')
        runs.append({'profile': outcome['profile'], 'prefix': prefix,
                     'native-log': outcome['log'], 'cases': cases})
expected = {'runs': runs, 'total-calls': sum(len(r['cases']) for r in runs)}
need(expected['total-calls'] == 95088 and load(D / 'copy-cases.json') == expected,
     'copy replay corpus exactly matches native observations')
need(hashlib.sha256(json.dumps(expected, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
     == '95337e58f32adb8ccf563af6ba7ec19f7461713edc7ce25df2321d468ad173d7', 'native copy semantic fingerprint')
print('Native BNO copy proof: 95088 calls, 46 child runs, 15 post-run identities')
