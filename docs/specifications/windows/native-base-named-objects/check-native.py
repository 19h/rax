"""Check raw native directory observations without executing a Windows binary."""
from collections import Counter
from pathlib import Path
import hashlib
import json
import re

D = Path(__file__).resolve().parent
PROFILES = [('arm64', 0xAA64, 8, True), ('x64', 0x8664, 8, True),
            ('x86', 0x14C, 4, False), ('x86-laa', 0x14C, 4, True)]
ACCESS = [0, 1, 2, 3, 4, 8, 15, 16, 0x20000, 0xF000F,
          0x10000000, 0x80000000, 0x40000000, 0x20000000, 0x1000000, 0x2000000]
FLAGS = [0, 0x40, 0x100, 0x140]
SOURCE_HASH = {
    'session-directories-v1': '219d9b6957d201293c33f8639e2dcc1787a447a7e73c8923fef5fae56484f4cc',
    'session-one-directories-v1': '1c81ffd3e5bf027950440aae3ce6e52ec7d5ec1b6078a4b31b49002e136d31cf',
    'directory-coherence-v2': 'ff25729e0675ef9e576e7fd651b6bc47428b2d631b2487328213eb13f8002bb5',
}


def need(condition, label):
    if not condition:
        raise ValueError(label)


def load(path):
    data = path.read_bytes()
    return json.loads(data.decode('utf-16') if data[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else data.decode('utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def records(text, prefix):
    return [dict(word.split('=', 1) for word in line.split() if '=' in word)
            for line in text.splitlines() if line.startswith(prefix)]


def number(row, key, base=10):
    return int(row[key], base)


def native_group(folder, stem):
    root = D / 'native' / folder
    launch = load(root / (stem + '-launch.json'))
    need(launch['state'] == 'finished' and launch['exit_code'] == 0, 'native driver completion')
    outcomes = load(root / (stem + '-outcomes.json'))
    need([r['profile'] for r in outcomes] == [p[0] for p in PROFILES], 'four native profiles')
    need(digest(root / ('native-' + stem + '.cpp')) == SOURCE_HASH[stem], 'compiled native source')
    for result, (profile, machine, width, laa) in zip(outcomes, PROFILES):
        need(result['exit-code'] == 0 and result['source'] == SOURCE_HASH[stem], 'native source after run')
        before = load(root / (stem + '-' + profile + '-before.json'))
        after = load(root / (stem + '-' + profile + '-after.json'))
        need(before == after and before['profile'] == profile, 'post-run PE identity')
        need(before['machine'] == machine and before['large-address-aware'] == laa and before['bytes'] > 0,
             'native PE machine and LAA')
        yield root, profile, width, result


need(__debug__, 'optimized validation rejected')
seen = set()
for row in load(D / 'native-inputs.json')['files']:
    need(row['path'] not in seen, 'unique native proof paths')
    seen.add(row['path'])
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'native input identity ' + row['path'])

calls_total = 0
observed_duplicate = False
for folder, stem, selected in [('current-session', 'session-directories-v1', 0),
                                ('session-one', 'session-one-directories-v1', 1)]:
    for root, profile, width, result in native_group(folder, stem):
        text = (D / result['log']).read_text(encoding='utf-8-sig')
        need(f'profile={profile} pointer-bytes={width} build=10.0.29683 token-session=0 process-session=0 private-namespace=0'
             in text, 'native kernel and caller identity')
        need(('selection-session=1 source=observed-NT-Sessions-directory' in text) == (selected == 1),
             'selected session differs explicitly from native principal')
        paths = records(text, 'path ')
        expected = ['\\BaseNamedObjects', '\\Sessions', f'\\Sessions\\{selected}',
                    f'\\Sessions\\{selected}\\BaseNamedObjects',
                    f'\\Sessions\\{selected}\\BaseNamedObjects\\Restricted',
                    '\\BaseNamedObjects\\Restricted']
        need([number(r, 'ordinal') for r in paths] == list(range(6)), 'six ordered native paths')
        need([bytes.fromhex(r['name']).decode('utf-16-le') for r in paths] == expected,
             'measured NT path grammar')
        rows = records(text, 'open ')
        keys = [(number(r, 'path'), number(r, 'flags', 16), number(r, 'access', 16)) for r in rows]
        need(len(keys) == len(set(keys)) == 384, 'complete access matrix')
        need(set(keys) == {(p, f, a) for p in range(6) for f in FLAGS for a in ACCESS},
             'native access and flag partitions')
        by_key = dict(zip(keys, rows))
        successes = 0
        for row in rows:
            status = number(row, 'status', 16)
            need(status in [0, 0xC000000D, 0xC0000022, 0xC0000034, 0xC000003A, 0xC0000061],
                 'observed native status domain')
            if status == 0:
                successes += 1
                data = bytes.fromhex(row['basic'])
                need(number(row, 'basic-status', 16) == 0 and number(row, 'basic-bytes') == len(data) == 56,
                     'native basic-information extent')
                need(int.from_bytes(data[4:8], 'little') & ~0xF000F == 0, 'native directory grant mask')
        for flags in [0, 0x40]:
            need(number(by_key[(0, flags, 15)], 'status', 16) == 0, 'native ordinary directory open')
            need(int.from_bytes(bytes.fromhex(by_key[(0, flags, 15)]['basic'])[4:8], 'little') == 15,
                 'native exact query/traverse/create grants')
            need(number(by_key[(0, flags, 0)], 'status', 16) == 0xC0000022, 'zero requested directory access')
            need(number(by_key[(0, flags, 0x1000000)], 'status', 16) == 0xC0000061,
                 'native system-security privilege boundary')
        for flags in [0x100, 0x140]:
            need(all(number(by_key[(0, flags, access)], 'status', 16) == 0xC000000D for access in ACCESS),
                 'native directory OPENLINK rejection')
        if selected == 0:
            need(all(number(r, 'status', 16) == 0xC0000034 for r in rows if r['path'] == '3'),
                 'session-zero local directory absence')
            need(all(number(r, 'status', 16) == 0xC000003A for r in rows if r['path'] == '4'),
                 'missing session-zero parent')
        else:
            need(all(number(by_key[(p, 0x40, 15)], 'status', 16) == 0 for p in range(6)),
                 'native nonzero session ancestry and restricted children')
        entries = records(text, 'entry ')
        for path in range(6):
            current = [r for r in entries if number(r, 'path') == path]
            need([number(r, 'ordinal') for r in current] == list(range(len(current))),
                 'complete ordered enumeration rows')
            for row in current:
                need(len(bytes.fromhex(row['name'])) % 2 == len(bytes.fromhex(row['type'])) % 2 == 0,
                     'native UTF-16 row widths')
            identities = Counter((r['name'], r['type']) for r in current)
            observed_duplicate |= any(count > 1 for count in identities.values())
        final = records(text, 'complete ')
        need(len(final) == 1 and number(final[0], 'paths') == 6 and number(final[0], 'opens') == successes
             and number(final[0], 'entries') == len(entries), 'native completion counts')
        need(final[0]['handles-before'] == final[0]['handles-after'], 'no leaked native handles')
        calls_total += len(rows)
need(observed_duplicate, 'retain live-enumeration duplicate evidence')

coherence_rows = 0
coherence_failures = 0
for root, profile, width, result in native_group('coherence', 'directory-coherence-v2'):
    text = (root / result['log']).read_text(encoding='utf-8-sig')
    need(f'profile={profile} pointer-bytes={width} build=10.0.29683' in text, 'coherence caller identity')
    passes = []
    for index in range(6):
        rows = [r for r in records(text, 'entry ') if number(r, 'pass') == index]
        need([number(r, 'ordinal') for r in rows] == list(range(len(rows))), 'coherence enumeration extent')
        values = sorted((r['name'], r['type']) for r in rows)
        unique = len({name for name, _ in values}) == len(values)
        report = [r for r in records(text, 'pass-complete ') if number(r, 'pass') == index]
        need(len(report) == 1 and number(report[0], 'count') == len(values)
             and number(report[0], 'unique') == int(unique), 'coherence uniqueness claim')
        need(number(report[0], 'same-previous') == int(index > 0 and passes[-1][0] == values),
             'coherence consecutive identity claim')
        passes.append((values, unique))
        coherence_rows += len(values)
    pairs = records(text, 'pair ')
    need(len(pairs) == 3, 'three bounded snapshot attempts')
    accepted = False
    for attempt, report in enumerate(pairs):
        a, b = passes[attempt * 2:attempt * 2 + 2]
        coherent = a[1] and b[1] and a[0] == b[0]
        need(number(report, 'attempt') == attempt and number(report, 'coherent') == int(coherent),
             'coherence pair claim')
        accepted |= coherent
        coherence_failures += int(not coherent)
    final = records(text, 'complete ')
    need(len(final) == 1 and number(final[0], 'passes') == 6
         and final[0]['bounded-acquisition'] == ('accepted' if accepted else 'unstable'),
         'bounded snapshot decision')
    need(final[0]['handles-before'] == final[0]['handles-after'], 'coherence handle lifetime')
need(coherence_failures == 2 and coherence_rows == 17436, 'retained unstable pairs and entry rows')

for prefix, source_hash, executable_hash in [
    ('directory-frontier', 'd5f5dba7cd758ec430d98542891dcb4c809cf2e4fbcc52f8b75895eba891d206',
     '0c58ec60b7f1f9d26f153880cdc0c266f4144e4e2f956c72254795b57ef298e4'),
    ('directory-fast', '1a5d047c324cdcc365d697273bacca64536d0c4366ee58dc0e66764119efa907',
     '2c905f3ccbf1d8472fc4eb071d52b4e878dbf7a77460a4ee6622acc301bda99c')]:
    root = D / 'diagnostics'
    value = load(root / (prefix + '-launch.json'))
    need(value['state'] == 'finished' and value['exit_code'] == 0, 'diagnostic completion')
    need(value['source-before'] == value['source-after'] and value['source-after']['sha256'] == source_hash
         and digest(root / (prefix + '-trace.rs')) == source_hash, 'diagnostic source identity')
    need(value['executable-before'] == value['executable-after']
         and value['executable-after']['sha256'] == executable_hash, 'diagnostic executable identity')
    need(len(value['outcomes']) == 2, 'two diagnostic programs')
    for index, row in enumerate(value['outcomes']):
        need(row['exit-code'] == 0 and row['program'] == row['program-after'], 'diagnostic program identity')
        path = root / row['log']
        need(digest(path) == row['log-identity']['sha256'], 'diagnostic log identity')
        text = path.read_text(encoding='utf-8-sig')
        turn = [2149956, 2152305][index]
        need(f'terminal turn={turn} Complete(Internal(' in text and 'NT directory outside selected installed namespace' in text,
             'recorded directory boundary, not successful guest exit')
        if prefix == 'directory-fast':
            need('prefix-turns=2100000 result=BudgetExhausted' in text
                 and f'directory turn={turn} access=0xf root=Ok(0) flags=Ok(40)' in text
                 and 'directory name="\\\\BaseNamedObjects" length=34' in text,
                 'bounded diagnostic prefix and requested NT name')
        else:
            need('decoded=Some("\\\\BaseNamedObjects")' in text, 'full diagnostic NT name')
# Preserve the regression that required an explicit incomplete membership state.
old = D / 'development/runtime-epoch1'
need(digest(old / 'native-namespace-churn.rs') ==
     '507fda0612ad1a9a39c888bc55c0ba58b37dd07524eccf0eb83e219228ba8eb9',
     'Original churn source')
for kind in ['archive', 'exe']:
    before = load(old / ('namespace-churn-old-' + kind + '-before.json'))
    after = load(old / ('namespace-churn-old-' + kind + '-after.json'))
    need(before == after, 'Original churn artifact stayed unchanged')
text = (old / 'namespace-churn-old.log').read_text(encoding='utf-8-sig')
rows = records(text, 'capture ')
need(len(rows) == 16 and [int(row['index']) for row in rows] == list(range(16))
     and all(row['result'] == 'failed' for row in rows), 'Original churn failures')
need(text.count('selected NT directory changed during bounded capture') == 16,
     'Original churn same capture failure')
need('complete captures=16 ok=0 failed=16 created=1008 closed=1008 error=None '
     'scope=capture-only-no-guest-execution' in text, 'Original churn handle lifetime')

print(f'Native proof verified: {calls_total} access cases, 24 enumerations/{coherence_rows} rows, two failed snapshot pairs, four diagnostic runs')
