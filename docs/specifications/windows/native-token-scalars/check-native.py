"""Verify complete native scalar matrices and isolated private-flag controls."""
from pathlib import Path
import hashlib
import json
import re

D = Path(__file__).resolve().parent
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
CLASSES = [15, 18, 20, 21, 23, 24, 26, 27, 32, 40, 42, 45, 46, 47, 48]
FIELDS = ['kind', 'role', 'length', 'status', 'output-bytes-cells',
          'return-bytes-cells', 'output-protect', 'returned-protect']


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    raw = (D / path).read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def rows(path):
    text = (D / path).read_text(encoding='utf-8-sig')
    return text.splitlines(), [dict(re.findall(r'([\w-]+)=([^\s]*)', x))
                               for x in text.splitlines() if x.startswith('case ')]


def scalar_case(values, kind, length):
    return next(row for row in values if (row['kind'], row['role'], row['length'])
                == (kind, '0', f'{length:08X}'))


need(__debug__, 'optimized checker rejected')
seen = set()
for row in load('native-inputs.json')['files'] + load('primary-inputs.json')['files']:
    path = Path(row['path'])
    need(not path.is_absolute() and '..' not in path.parts and str(path) not in seen,
         'unique relative evidence paths')
    seen.add(str(path))
    raw = (D / path).read_bytes()
    need((len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256']),
         'evidence identity ' + str(path))

corpus = load('cases.json')
need(corpus['schema'] == 1 and corpus['profiles'] == PROFILES and corpus['classes'] == CLASSES,
     'scalar corpus identity')
need(corpus['per-query'] == 1830 and corpus['matrix-queries'] == 109800
     and len(corpus['groups']) == 6, 'complete scalar corpus')
for group in corpus['groups']:
    digest = hashlib.sha256(json.dumps(group['cases'], sort_keys=True,
                                      separators=(',', ':')).encode()).hexdigest()
    need(digest == group['sha256'] and len(group['cases']) == 1830, 'canonical rows')

launch = load('native/token-scalar-survey-v2-launch.json')
need(launch['state'] == 'finished' and launch['exit-code'] == 0, 'final native completion')
outcomes = load('native/token-scalar-survey-v2-outcomes.json')
need([(r['profile'], r['class']) for r in outcomes] == [(p, c) for p in PROFILES for c in CLASSES]
     and all(r['exit-code'] == 0 for r in outcomes), 'all native invocations')
post = load('native/token-scalar-survey-v2-post-run.json')
need([r['profile'] for r in post] == PROFILES, 'complete post-run native profiles')
source = hashlib.sha256((D / 'native/native-token-scalar-survey-v2.cpp').read_bytes()).hexdigest()
need(source == 'e2d3686afb8b6d69bb501fb8250dea1981d196e9d6e9e0eae6291f01535120e6', 'final native source')
for profile, final in zip(PROFILES, post):
    identity = load(f'native/token-scalar-survey-v2-{profile}-identity.json')
    need(final['source']['sha256'] == source and final['executable']['sha256'] == identity['sha256']
         and final['executable']['bytes'] == identity['bytes'] > 0, 'post-run source and PE')
    need(identity['machine'] == {'arm64': 0xaa64, 'x64': 0x8664, 'x86': 0x14c, 'x86-laa': 0x14c}[profile]
         and identity['large-address-aware'] == (profile != 'x86'), 'machine and LAA')
    for cls in CLASSES:
        path = f'native/token-scalar-survey-v2-{profile}-class{cls}.log'
        lines, values = rows(path)
        ref = dict(re.findall(r'([\w-]+)=([^\s]*)', lines[0]))
        need(int(ref['class']) == cls, 'reference selector')
        need(lines[1] == f'identity profile={profile} pointer-bytes={4 if profile.startswith("x86") else 8} build=10.0.29683 page=4096 cases=1830'
             and lines[-1] == 'complete cases=1830 closed-handles=3 released-arenas=1', 'native identity and cleanup')
        need([int(r['ordinal']) for r in values] == list(range(1830))
             and all(r['escaped'] == '00000000' and r['last-error'] == '7E11ABCD' for r in values),
             'ordered calls and last-error preservation')
        index = corpus['mapping'][profile][str(cls)]
        need([{k: r[k] for k in FIELDS} for r in values] == corpus['groups'][index]['cases'],
             'exact native row replay data')
        reference = corpus['references'][profile][str(cls)]
        unavailable = cls in [45, 46] or (cls == 40 and profile.startswith('x86'))
        status = 0xC0000003 if unavailable else 0
        width = 0 if unavailable else 1 if cls == 21 else 4
        value = None if unavailable else 1 if cls in [18, 20, 21, 27] else 0
        need(reference['status'] == status and reference['returned'] == width
             and reference['value'] == value and int(ref['status'], 16) == status
             and int(ref['bytes']) == width, 'native selector status, width and value')
        if width == 1:
            need(int(ref['value'], 16) == 0xA5A5A501, 'byte reference preserves adjacent bytes')
        if unavailable:
            need(reference['raw-value'] == int(ref['value'], 16) == 0xA5A5A5A5,
                 'unavailable reference preserves output')
        early = profile.startswith('x86') and cls in [40, 45]
        if early:
            need(all(r['status'] == 'C0000003' for r in values), 'WoW64 pre-probe rejection')
        elif unavailable:
            need(scalar_case(values, 'invalid', 4)['status'] == 'C0000003'
                 and any(r['status'] == 'C0000005' for r in values)
                 and any(r['status'] == '80000002' for r in values), 'post-probe unavailable selector')
        elif cls == 20:
            for kind in ['primary', 'invalid']:
                for length in [0, 1, 3, 5, 8, 15, 16, 4096]:
                    row = scalar_case(values, kind, length)
                    need(row['status'] == 'C0000004' and row['return-bytes-cells'] == '04000000',
                         'elevation exact length precedes handle')
            need(scalar_case(values, 'invalid', 4)['return-bytes-cells'] == '04000000',
                 'elevation publishes length before invalid handle')
        else:
            row = scalar_case(values, 'primary', width)
            expected = '01A5A5A5' if width == 1 else f'{value:02X}000000'
            need(row['status'] == '00000000' and row['output-bytes-cells'] == expected,
                 'successful native scalar publication')

initial = load('epoch1/token-scalar-survey-outcomes.json')
expected_failures = {(p, c) for p in PROFILES for c in [21, 45, 46]} | {('x86', 40), ('x86-laa', 40)}
need(len(initial) == 52 and {(r['profile'], r['class']) for r in initial if r['exit-code']} == expected_failures,
     'initial reference failures retained separately')
need(sum(r['exit-code'] == 0 for r in initial) == 38, 'initial complete matrices')
private_launch = load('private/token-namespace-private-launch.json')
need(private_launch['state'] == 'finished' and private_launch['exit-code'] == 0, 'private launch completion')
private_post = load('private/token-namespace-private-post-run.json')
need([r['profile'] for r in private_post] == PROFILES, 'private control profiles')
private_source = hashlib.sha256((D / 'private/native-token-namespace-private.cpp').read_bytes()).hexdigest()
need(private_source == '7abede036f0b6b68af4aed4dbcbda136ec261043baf3cb2b3d18da41ad79453a', 'private native source')
for profile, final in zip(PROFILES, private_post):
    identity = load(f'private/token-namespace-private-{profile}-identity.json')
    need(final['source']['sha256'] == private_source and final['executable']['sha256'] == identity['sha256'],
         'private source and executable')
    need(identity['machine'] == {'arm64': 0xaa64, 'x64': 0x8664, 'x86': 0x14c, 'x86-laa': 0x14c}[profile] and identity['large-address-aware'] == (profile != 'x86') and identity['bytes'] == final['executable']['bytes'] > 0, 'private PE machine and LAA')
    lines, values = rows(f'private/token-namespace-private-{profile}.log')
    need(lines[-1] == 'complete cases=6 successful-private-namespace-changes=6 reverted=1', 'private thread restored')
    need([r['requested'] for r in values] == ['00000000', '00000001', '00000007', '12345678', '80000000', 'FFFFFFFF'],
         'private values')
    for row in values:
        need(all(row[k] == '00000000' for k in ['set-status', 'before-status', 'before', 'after-status', 'duplicate-status'])
             and all(row[k] == '4' for k in ['before-length', 'after-length', 'duplicate-length']), 'private calls')
        need(row['after'] == row['duplicate'] == ('00000000' if int(row['requested'], 16) == 0 else '00000001')
             and row['last-error'] == '7E11ABCD', 'private Boolean normalization')
print('Verified 109800 native matrix queries, 60 native reference queries and 24 separate private Boolean controls')
