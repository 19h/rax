"""Bind the four-profile native token-opening controls and their measured axes."""
from pathlib import Path
import hashlib
import json

D = Path(__file__).resolve().parent
N = D / 'native'


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


need(__debug__, 'optimized checking rejected')
for manifest in ['native-inputs.json', 'primary/sources.json']:
    rows = load(D / manifest)['files']
    need(len(rows) == len({r['path'] for r in rows}), 'unique manifest paths')
    for row in rows:
        raw = (D / row['path']).read_bytes()
        need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
             'input identity ' + row['path'])
producers = load(N / 'producer-identities.json')['producers']
profiles = ['arm64', 'x64', 'x86', 'x86-laa']
kinds = ['initial', 'boundary-v3', 'access', 'lifetime']
need(len(producers) == 16 and {(p['kind'], p['profile']) for p in producers} ==
     {(kind, profile) for kind in kinds for profile in profiles}, 'complete producer axes')
count = 0
for p in producers:
    kind, profile = p['kind'], p['profile']
    need(p['exit-code'] == 0, 'native producer exit')
    width = 4 if profile.startswith('x86') else 8
    need(p['exe']['machine'] == {'arm64': 0xaa64, 'x64': 0x8664,
                                 'x86': 0x14c, 'x86-laa': 0x14c}[profile], 'native PE machine')
    need(p['exe']['large-address-aware'] == (profile != 'x86')
         and p['exe']['bytes'] > 0 and len(p['exe']['sha256']) == 64, 'native post-run PE identity')
    for row in [p['source'], p['log']]:
        raw = (N / row['path']).read_bytes()
        need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
             'producer source/log identity')
    lines = (N / p['log']['path']).read_text().splitlines()
    identity = fields(next(line for line in lines if line.startswith('identity ')))
    need(int(identity['width']) == width and identity['profile'] == profile, 'native caller identity')
    need(lines[-1].startswith('complete '), 'native resource cleanup completion')
    rows = [fields(line) for line in lines if line.startswith('case ')]
    if kind == 'lifetime':
        operations = [fields(line) for line in lines if line.startswith(('open ', 'close '))]
        expected = 25 if width == 4 else 28
        need(len(operations) == expected and [int(r['ordinal']) for r in operations] == list(range(expected)),
             'native lifetime operation sequence')
        need(all(r['last-error'] == '4A2E19F7' for r in operations), 'native lifetime last error')
        sequence = [int(fields(line)['count']) for line in lines if line.startswith('handle label=first-after-open ')]
        need(len(sequence) == 6 and sequence == list(range(sequence[0], sequence[0] + 6)),
             'shared primary-token handle count increments')
        need(all(r['status'] == 'C0000008' for r in operations if r['label'] == 'process-high-alias'),
             'native high-handle rejection')
        count += expected
        continue
    expected = {'initial': 1024, 'boundary-v3': 2160, 'access': 13215}[kind]
    need(len(rows) == expected and [int(r['ordinal']) for r in rows] == list(range(expected)),
         'complete native operation sequence')
    for row in rows:
        need(row['exception'] == '00000000' and row['lost'] == '0'
             and row['last-error'] == '4A2E19F7', 'no escaped exceptions, handle loss or last-error change')
        success = row['status'] == '00000000'
        need(int(row['births']) == int(success) and int(row['output-is-birth']) == int(success),
             'exact returned handle birth')
        grants = [int(v, 16) for v in row['grants'].split(',') if v]
        need(len(grants) == int(success), 'one grant for one successful opening')
        need(len(bytes.fromhex(row['prefix'])) == 24 and len(bytes.fromhex(row['boundary'])) == 32,
             'complete caller byte windows')
    if kind == 'boundary-v3':
        need({(int(r['handle']), int(r['desired'], 16), int(r['mode'])) for r in rows} ==
             {(h, a, m) for h in [0, 1, 5, 6, 7, 10]
              for a in [0, 8, 0x01000000, 0xffffffff] for m in range(90)}, 'complete boundary axes')
        need(all(int(r['offset']) == 4080 + int(r['mode']) % 9 * 3 for r in rows), 'boundary offset construction')
    if kind == 'access':
        masks = {int(r['desired'], 16) for r in rows}
        need(len(masks) == 2643 and {int(r['handle']) for r in rows} == {0, 5, 6, 7, 9}, 'access axes')
        need({(int(r['handle']), int(r['desired'], 16)) for r in rows} ==
             {(h, a) for h in [0, 5, 6, 7, 9] for a in masks}, 'complete access cross product')
        for row in rows:
            desired = int(row['desired'], 16)
            mapped = desired & 0x0fffffff
            for generic, specific in [(0x80000000, 0x2001a), (0x40000000, 0x201e0),
                                       (0x20000000, 0x20005), (0x10000000, 0xf01ff)]:
                if desired & generic:
                    mapped |= specific
            grant = None
            if row['handle'] == '5' or mapped & ~0x031fffff:
                status = 0xc0000022
            elif mapped & 0x01000000:
                status = 0xc0000061
            elif mapped == 0 or mapped & ~0x020f01ff:
                status = 0xc0000022
            else:
                status = 0
                grant = 0xf01ff if mapped & 0x02000000 else mapped
            need(int(row['status'], 16) == status, 'measured access precedence')
            need(row['grants'] == ('' if grant is None else f'{grant:08X},'), 'measured native access grant')
    count += expected
host = load(N / 'host-version.json')
need(host['platform-version'] == [10, 0, 29683] and host['platform-machine'] == 'ARM64', 'native host identity')
need(count == 65702, 'total recorded service and lifetime operations')
print('16 producer identities and 65702 native operations verified; lifetime controls remain separate from row replay')
