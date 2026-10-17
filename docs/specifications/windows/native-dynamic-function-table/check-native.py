"""Verify native producer identities, complete axes and independent fault rules."""
from pathlib import Path
from collections import Counter
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


def parse(raw):
    result = []
    for line in raw.decode('utf-8-sig').splitlines():
        fields = dict(token.split('=', 1) for token in line.split() if '=' in token)
        if fields:
            result.append({'record': line.split()[0].split('=')[0], **fields})
    return result


need(__debug__, 'optimized validation rejected')
seen = set()
for manifest in ['native-inputs.json', 'primary/sources.json', 'diagnostic-inputs.json']:
    for row in load(D / manifest)['files']:
        need(row['path'] not in seen, 'duplicate native path')
        seen.add(row['path'])
        raw = (D / row['path']).read_bytes()
        need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
             'input identity ' + row['path'])

counts = {'initial': 9, 'lifecycle': 1, 'key': 1, 'boundary': 1, 'ranges': 1,
          'ranges-isolated': 51, 'overlap': 56, 'overflow': 15, 'lifetime': 1,
          'zero-overlap': 9, 'sequence': 26, 'status-map': 1, 'handle-boundary': 6}
identities = load(N / 'producer-identities.json')
need(identities['windows-version'][:3] == [10, 0, 29683], 'native Windows build')
producers = identities['identities']
need(len(producers) == 52, 'native producer count')
expected_keys = {(kind, profile) for kind in counts for profile in ['arm64', 'x64', 'x86', 'x86-laa']}
need({(p['kind'], p['profile']) for p in producers} == expected_keys, 'native producer axes')
executions = []
for producer in producers:
    kind, profile = producer['kind'], producer['profile']
    need(len(producer['logs']) == counts[kind], 'child execution count')
    source = (N / producer['source']).read_bytes()
    need(len(source) == producer['source-bytes'] and
         hashlib.sha256(source).hexdigest() == producer['source-sha256'], 'source identity')
    need(producer['machine'] == {'arm64': 0xaa64, 'x64': 0x8664, 'x86': 0x14c, 'x86-laa': 0x14c}[profile],
         'native PE machine')
    need(producer['large-address-aware'] == (profile != 'x86'), 'native PE LAA')
    need(producer['exe-bytes'] > 0 and len(producer['exe-sha256']) == 64, 'post-run PE identity')
    for item in producer['logs']:
        raw = (N / item['path']).read_bytes()
        need(item['exit-code'] == 0 and len(raw) == item['bytes'] and
             hashlib.sha256(raw).hexdigest() == item['sha256'], 'native exit/stdout')
        records = parse(raw)
        executions.append({'kind': kind, 'profile': profile, 'log': item['path'],
                           'records': records, 'contract': kind != 'ranges'})
        for row in records:
            if 'exception' in row:
                need(row['exception'] == '00000000', 'uncaught native exception')
            if 'exceptions' in row:
                need(row['exceptions'] == '00000000,00000000,00000000', 'native range exceptions')
            if 'last-error' in row:
                need(row['last-error'] == '4A2E19F7', 'native last-error preservation')
        if kind == 'boundary':
            cases = [r for r in records if r['record'] in ['case', 'length']]
            need(len(cases) == 966 and [int(r['ordinal']) for r in cases] == list(range(966)),
                 'boundary complete sequence')
            for row in cases:
                if row['record'] == 'length':
                    if profile.startswith('x86'):
                        status = '00000000'
                    elif int(row['invalid']) >= 2:
                        status = 'C000000D'
                    elif int(row['length']) != 16:
                        status = 'C0000004'
                    else:
                        status = 'C0000005' if int(row['invalid']) & 1 else '00000000'
                    need(row['status'] == status, 'handle/length/read ordering')
                    continue
                need(row['unchanged'] == '1', 'native caller write')
                axis, offset = int(row['axis']), int(row['offset'])
                before = [int(row['p0'], 16), int(row['p1'], 16)]
                after = before.copy()
                status = '00000000'
                if not profile.startswith('x86'):
                    alignment = 8 if axis == 0 else 4
                    if offset % alignment:
                        status = '80000002'
                    else:
                        accesses = [(offset, 16)] if axis == 0 else [(offset + 48, 8), (offset + 40, 8)]
                        pages = [page for start, size in accesses
                                 for page in range(start // 4096, (start + size - 1) // 4096 + 1)]
                        for page in pages:
                            if before[page] & 0xff == 1:
                                status = 'C0000005'
                                break
                            if before[page] & 0x100:
                                status = '80000001'
                                after[page] &= ~0x100
                                break
                need(row['status'] == status, 'native read fault precedence')
                need([int(row['after0'], 16), int(row['after1'], 16)] == after,
                     'native read guard lifetime')
        if kind == 'key':
            need([int(r['byte']) for r in records if r['record'] == 'key'] == list(range(-1, 144)),
                 'complete key byte axes')
        if kind == 'lifecycle':
            need([int(r['ordinal']) for r in records if r['record'] == 'case'] == list(range(21)),
                 'complete lifecycle sequence')
        if kind == 'lifetime':
            lifetime = [r for r in records if r['record'] == 'case']
            need([int(r['ordinal']) for r in lifetime] == list(range(14)),
                 'complete free/thread lifetime sequence')
            for index, row in enumerate(lifetime):
                expected = '00000000'
                if not profile.startswith('x86'):
                    expected = {1: 'C0000018', 13: 'C00000FF'}.get(index, expected)
                need(row['status'] == expected and row['input-unchanged'] == '1',
                     'native free/thread ownership lifetime')
        if kind == 'sequence':
            operations = [r for r in records if r['record'] == 'operation']
            need([int(r['ordinal']) for r in operations] == list(range(len(operations))),
                 'ordered sequence ordinals')
            need(records[-1]['record'] == 'complete', 'sequence completion')
        if kind == 'status-map':
            mappings = [r for r in records if r['record'] == 'profile']
            for function in ['RtlNtStatusToDosError', 'RtlNtStatusToDosErrorNoTeb']:
                selected = {r['status']: int(r['error']) for r in mappings
                            if r['function'] == function}
                need(len(selected) == 11 and selected['C00000FF'] == 559
                     and selected['C0000095'] == 534, 'native DOS status conversions')

record_counts = {
    'initial': {'profile': 9},
    'lifecycle': {'identity': 1, 'case': 21, 'handle': 5, 'complete': 1},
    'key': {'identity': 1, 'key': 145, 'complete': 1},
    'boundary': {'identity': 1, 'case': 830, 'length': 136, 'complete': 1},
    'ranges': {'identity': 1, 'case': 51, 'complete': 1},
    'ranges-isolated': {'identity': 51, 'case': 51, 'complete': 51},
    'overlap': {'profile': 56},
    'overflow': {'identity': 15, 'case': 15},
    'lifetime': {'identity': 1, 'case': 14, 'complete': 1},
    'zero-overlap': {'profile': 9},
    'sequence': {'identity': 26, 'operation': 754, 'complete': 26},
    'status-map': {'profile': 22},
    'handle-boundary': {'profile': 6},
}
for kind, profile in expected_keys:
    actual = Counter(r['record'] for e in executions
                     if e['kind'] == kind and e['profile'] == profile for r in e['records'])
    need(actual == record_counts[kind], 'complete record counts ' + kind + '/' + profile)

corpus = load(N / 'corpus.json')
need(corpus['schema'] == 1 and corpus['executions'] == executions, 'raw-to-replay correspondence')
need(len(executions) == 712, 'total native child execution count')
print('52 post-run producer identities, 712 child executions and complete native fault axes verified')
