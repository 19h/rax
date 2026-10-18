from pathlib import Path
import hashlib
import json
import re
import shutil

B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
MODES = ['disabled-empty', 'enabled-ascii', 'enabled-unicode']
SOURCES = {
    'bno-namespace-precreate': ('native-bno-namespace-precreate.cpp',
                              '0ee97d214e5082be5e1e284c037f756db3bd68a321858e498230d0eab50c2a03'),
    'bno-routing': ('native-bno-routing.cpp',
                    '0a6b2c4fc90cc006e0f8b9ffc63b5a9c0e019dc197dab7092d4dad84f662a7e1'),
}


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


def units(text):
    return bytes.fromhex(text).decode('utf-16-be')


assert __debug__
summary = []
for stem, (source_name, source_hash) in SOURCES.items():
    dest = B / stem
    dest.mkdir(exist_ok=True)
    for path in S.glob(stem + '*'):
        if path.is_file():
            shutil.copy2(path, dest / path.name)
    shutil.copy2(S / source_name, dest / source_name)
    assert hashlib.sha256((dest / source_name).read_bytes()).hexdigest() == source_hash
    launch = load(dest / (stem + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    controller = load(dest / (stem + '-controller-before.json'))
    assert controller == load(dest / (stem + '-controller-after.json'))
    assert controller['Hash'].lower() == '4d2197c5b680de69d14feb17c048e937c20324a315c2a22b60a3de60ac9a0aa1'
    outcomes = load(dest / (stem + '-outcomes.json'))
    assert len(outcomes) == 12 and {(r['profile'], r['mode']) for r in outcomes} == {
        (p, m) for p in PROFILES for m in MODES}
    for profile in PROFILES:
        before = load(dest / (stem + '-' + profile + '-before.json'))
        assert before == load(dest / (stem + '-' + profile + '-after.json'))
        assert before['machine'] == (43620 if profile == 'arm64' else 34404 if profile == 'x64' else 332)
        assert before['large-address-aware'] == (profile != 'x86')
        assert load(dest / (stem + '-' + profile + '-source-after.json'))['Hash'].lower() == source_hash
    for row in outcomes:
        assert row['exit-code'] == 0
        lines = (dest / row['log']).read_text().splitlines()
        control, metadata = fields(lines[0]), fields(lines[2])
        assert lines[1] == 'child-create ok=1 error=0'
        assert metadata['enabled'] == control['enabled'] and metadata['session'] == '0'
        assert int(metadata['pointer-bytes']) == (4 if row['profile'].startswith('x86') else 8)
        assert metadata['prefix'] == control['prefix-utf16']
        prefix = units(metadata['prefix'])
        assert bool(prefix) == row['mode'].startswith('enabled-')
        parent = '\\BaseNamedObjects' + ('\\' + prefix if prefix else '')
        pre = fields(next(line for line in lines if line.startswith('namespace-before-event ')))
        assert pre['status'] == '00000000' and units(pre['path']) == parent
        event = units(fields(next(line for line in lines if line.startswith('event ')))['name'])
        assert re.fullmatch(re.escape(parent) + r'\\AssistBnoNamespace_\d+_\d+', event)
        entries = [fields(line) for line in lines if line.startswith('directory ')]
        actual = [r for r in entries if r['actual-parent'] == '1']
        assert len(actual) == 1 and units(actual[0]['path']) == parent and actual[0]['status'] == '00000000'
        done = fields(lines[-2])
        assert done['opened-directories'] == ('2' if prefix else '1') and done['preopened'] == '1'
        assert int(done['closed-handles']) == (5 if prefix else 4) + (5 if stem == 'bno-routing' else 0)
        assert lines[-1] == 'child-complete wait=00000000 queried=1 exit=0 closed=1'
        links = []
        if stem == 'bno-routing':
            links = [fields(line) for line in lines if line.startswith('private-link ')]
            assert len(links) == 3
            expected = [parent, '\\BaseNamedObjects', '\\BaseNamedObjects\\Session' if prefix else '\\Sessions\\BNOLINKS']
            for link, name, target in zip(links, ['Local', 'Global', 'Session'], expected):
                assert link['open'] == link['query'] == '00000000'
                assert units(link['path']) == parent + '\\' + name and units(link['target']) == target
                assert int(link['length']) == len(bytes.fromhex(link['target']))
                assert int(link['required']) == int(link['length']) + 2
            routed = [fields(line) for line in lines if line.startswith('routed-event ')]
            assert len(routed) == 2 and all(r['query'] == '00000000' and r['error'] == '0' for r in routed)
            leaf = units(routed[0]['input'])
            assert re.fullmatch(r'AssistBnoRoute_\d+_\d+', leaf)
            assert units(routed[0]['actual']) == parent + '\\' + leaf
            assert units(routed[1]['input']) == 'Global\\' + leaf
            assert units(routed[1]['actual']) == '\\BaseNamedObjects\\' + leaf
        summary.append({'producer': stem, 'profile': row['profile'], 'mode': row['mode'],
                        'session': 0, 'parent': parent, 'opened-before-own-event': True,
                        'closed-child-handles': int(done['closed-handles']),
                        'link-targets': [units(r['target']) for r in links],
                        'global-event-outside-private-parent': bool(prefix) if links else None})
empty = load(S / 'bno-namespace-empty-precreate.json')
assert empty['state'] == 'finished' and empty['exit-code'] == 0
assert empty['controller-before'] == empty['controller-after']
assert [r['profile'] for r in empty['runs']] == PROFILES
for row in empty['runs']:
    assert row['exit-code'] == 26 and row['exe-before'] == row['exe-after']
    assert row['exe-before']['sha256'] == load(B / 'bno-namespace-precreate' /
        ('bno-namespace-precreate-' + row['profile'] + '-before.json'))['sha256']
    lines = (S / row['log']).read_text().splitlines()
    assert len(lines) == 2 and lines[-1] == 'child-create ok=0 error=87'
    assert 'enabled=1 prefix-units=0' in lines[0]
(B / 'routing-reviewed.json').write_text(json.dumps({'runs': summary,
    'empty-prefix-control': 'four CreateProcess attribute rejections; token-set behavior unknown',
    'qualification': 'One ARM64 kernel, session0 principal. Preopen is before the probe event, not before all native CRT initialization.'}, indent=2) + '\n')
print('Verified24 namespace runs,36 link targets,24 routed events and four empty-prefix creation rejections')
