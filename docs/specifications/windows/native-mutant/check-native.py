"""Verify retained native producers, complete matrices, lifetimes and projections."""
from pathlib import Path
import argparse
import hashlib
import itertools
import json
import re

B = Path(__file__).resolve().parent
parser = argparse.ArgumentParser()
parser.add_argument('--source-root', type=Path, default=B.parents[3])
P = parser.parse_args().source_root
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


def verify_run(folder, prefix, source_hash, source_name=None):
    root = B / 'native' / folder
    source_name = source_name or prefix + '.cpp'
    source = root / source_name
    if not source.exists():
        source = B / 'native/producers' / source_name
    assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash
    launch = load(root / (prefix + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    modules = load(root / (prefix + '-modules-before.json'))
    assert modules == load(root / (prefix + '-modules-after.json'))
    assert {row['Hash'].lower() for row in modules} == {
        '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
        'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'}
    outcomes = load(root / (prefix + '-outcomes.json'))
    assert [row['profile'] for row in outcomes] == PROFILES
    result = []
    for outcome in outcomes:
        profile = outcome['profile']
        assert outcome['exit-code'] == 0
        stem = prefix + '-' + profile
        before = load(root / (stem + '-before.json'))
        assert before == load(root / (stem + '-after.json'))
        assert before['machine'] == (43620 if profile == 'arm64' else 34404 if profile == 'x64' else 332)
        assert before['large-address-aware'] == (profile != 'x86')
        assert load(root / (stem + '-source-after.json'))['Hash'].lower() == source_hash
        lines = (root / outcome['log']).read_text().splitlines()
        identity = fields(lines[0])
        assert int(identity['pointer-bytes']) == (4 if profile.startswith('x86') else 8)
        result.append((profile, lines))
    return result


def projection(profile, kind, rows, columns):
    expected = '\n'.join('\t'.join(row[key] for key in columns) for row in rows) + '\n'
    assert (B / 'replay' / (kind + '-' + profile + '.tsv')).read_text() == expected


assert __debug__
for row in load(B / 'primary-provenance.json')['files']:
    raw = ((P / row['repository-path']) if row['path'].startswith('../') else (B / row['path'])).read_bytes()
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
manifest = load(B / 'native-inputs.json')
assert len({row['path'] for row in manifest['files']}) == len(manifest['files'])
for row in manifest['files']:
    path = (B / row['path']).resolve()
    assert path.is_relative_to(B) and path.suffix != '.exe'
    raw = path.read_bytes()
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']

grants = {0: 0, 1: 1, 2: 0, 3: 1, 0x1F0001: 0x1F0001, 0x1F0003: 0x1F0001,
          0x02000000: 0x1F0001, 0x80000000: 0x20001, 0x40000000: 0x20000,
          0x20000000: 0x120000, 0x10000000: 0x1F0001, 0x01000000: 0x01000000,
          0x00200000: 0, 0xFFFFFFFF: 0x011F0001}
for profile, lines in verify_run('native-v2', 'native-mutant-v2',
                                 '51189faf7621e9422d85c3c57873f3f2cc194c6ea7735956d2949d4e584b630c',
                                 'native-mutant-v2-probe.cpp'):
    rows = [fields(line) for line in lines if line.startswith('create ')]
    objects = {row['ordinal']: row for row in
               (fields(line) for line in lines if line.startswith('object '))}
    assert [int(row['ordinal']) for row in rows] == list(range(223))
    assert all(row['escaped'] == '00000000' for row in rows)
    access_rows = [row for row in rows if row['label'] in ['null', 'unnamed']]
    assert len(access_rows) == 196
    assert {int(row['access'], 16) for row in access_rows} == set(grants)
    for row in access_rows:
        assert row['status'] == '00000000'
        obj = objects[row['ordinal']]
        assert obj['status'] == '00000000' and obj['returned'] == '00000038'
        assert int(obj['granted'], 16) == grants[int(row['access'], 16)]
    done = fields(lines[-1])
    assert done['cases'] == '223' and done['created'] == done['closed'] == '219'
    assert done['directory-closed'] == '1'

for profile, lines in verify_run('native-faults', 'native-mutant-faults-v2',
                                 'f5e15fad3a8e7ac04ec8e73b2e8f672b1192892923a00b1dfbf8ad434100fbf1'):
    query = [fields(line) for line in lines if line.startswith('query ')]
    release = [fields(line) for line in lines if line.startswith('release ')]
    assert [int(row['case']) for row in query] == list(range(6300))
    assert [int(row['case']) for row in release] == list(range(50))
    actual = [(int(r['handle']), int(r['class'], 16), int(r['length'], 16),
               int(r['out']), int(r['ret'])) for r in query]
    assert actual == list(itertools.product(range(5), [0, 1, 0xFFFFFFFF],
        [0, 1, 7, 8, 16, 17, 0xFFFFFFFF], range(10), [0, 1, 5, 6, 8, 10]))
    assert all(r['escaped'] == '00000000' for r in query + release)
    done = fields(lines[-1])
    assert done['queries'] == '6300' and done['releases'] == '50' and done['before'] == done['after']
    projection(profile, 'query', query, ['case', 'handle', 'class', 'length', 'out', 'ret', 'status', 'bytes', 'returned'])
    projection(profile, 'release', release, ['case', 'handle', 'previous', 'status', 'bytes', 'before', 'after'])
    for row in release:
        assert row['before'] == '00000000/00000008/000000000100A5A5'
        success = row['status'] == '00000000'
        assert row['after'] == ('00000000/00000008/010000000000A5A5' if success else row['before'])

for profile, lines in verify_run('native-create-faults-v2', 'native-mutant-create-faults-v2',
                                 'f6d4561f39ee12e1281eb92729cf59027ca1a1c8f16d0af19c23a033db14ec91'):
    rows = [fields(line) for line in lines if line.startswith('create ')]
    assert [int(row['case']) for row in rows] == list(range(3200))
    assert [(int(r['attrs']), int(r['owner'], 16), int(r['access'], 16), int(r['output'])) for r in rows] == list(
        itertools.product(range(20), [0, 1, 256, 257], [0, 0x1F0001], range(20)))
    assert all(row['escaped'] == '00000000' for row in rows)
    publications = {int(r['case']): int(r['handle'], 16) for r in
        (fields(line) for line in lines if line.startswith('published '))}
    successes = [r for r in rows if int(r['status'], 16) < 0x80000000]
    assert set(publications) == {int(r['case']) for r in successes}
    width = 4 if profile.startswith('x86') else 8
    for row in successes:
        mode = int(row['output'])
        raw = bytes.fromhex(row['attr-bytes' if mode >= 16 else 'out-bytes'].split(':')[1])
        assert len(raw) == 192
        if mode >= 16:
            offset = (mode - 16) * width
        elif mode in [0, 1]:
            offset = mode
        elif mode in [2, 10, 11, 12]:
            offset = 124
        elif mode in [3, 13, 14]:
            offset = 127
        else:
            assert mode in [4, 5, 6, 7]
            offset = 128
        assert int.from_bytes(raw[offset:offset + width], 'little') == publications[int(row['case'])]
    done = fields(lines[-1])
    assert done['cases'] == '3200' and int(done['created']) == int(done['closed']) == len(successes)
    assert done['before'] == done['after']
    projection(profile, 'create', rows, ['case', 'attrs', 'output', 'owner', 'access', 'status'])

for profile, lines in verify_run('native-names-v4', 'native-mutant-names-v4',
                                 'c049e3d1628637fb06d2b93943bd7d8ffcee4597983b6fac7ac11f73f99a1789'):
    rows = [fields(line) for line in lines if line.startswith('named ')]
    pointers = [fields(line) for line in lines if line.startswith('attribute-pointer ')]
    assert [int(row['ordinal']) for row in rows] == list(range(144))
    assert [int(row['ordinal']) for row in pointers] == list(range(144))
    assert all(row['escaped'] == '00000000' for row in pointers)
    assert [r['status'] for r in rows[141:]] == ['00000000', 'C0000034', '00000000']
    done = fields(lines[-1])
    assert done['cases'] == '144' and done['acquired'] == done['closed'] == '77'
    assert done['before'] == done['after']

for revision, source_hash, handles in [
    (5, '2435973ecddfe3baee8c6f62cc1923e5046c591b6d6f04ec9d2ec890c971c1ac', 89),
    (6, '4e74d710ffd50b42633fce5ff7297bc417b6f4698da50b7b90fafc473b57f9a9', 101),
]:
    for profile, lines in verify_run('native-names-v' + str(revision),
            'native-mutant-names-v' + str(revision), source_hash):
        rows = [fields(line) for line in lines if line.startswith('named ')]
        pointers = [fields(line) for line in lines if line.startswith('attribute-pointer ')]
        assert [int(row['ordinal']) for row in rows] == list(range(156))
        assert len(pointers) == 156 and all(row['escaped'] == '00000000' for row in pointers)
        assert all(row['status'] == '00000000' for row in rows[144:])
        objects = {int(row['ordinal']): row for row in
                   (fields(line) for line in lines if line.startswith('object-name '))}
        leaf = bytes.fromhex(fields(lines[0])['leaf']).decode('utf-16-le')
        for index in [31, 33, 35, 37, 39, 41]:
            assert bytes.fromhex(objects[index]['units']).decode('utf-16-le') == '\\BaseNamedObjects\\' + leaf
        for index in [32, 34, 36, 38, 40, 42]:
            assert bytes.fromhex(objects[index]['units']).decode('utf-16-le') == '\\BaseNamedObjects\\' + leaf.swapcase()
        if revision == 6:
            raw = [fields(line) for line in lines if line.startswith('win32-raw ')]
            assert [(int(row['role']), row['operation']) for row in raw] == [
                (index, operation) for index in range(6) for operation in ['create', 'open']]
            for index, row in enumerate(raw):
                assert int(row['same-native']) == int(row['role'] != '0')
                expected_error = ('00000000' if row['role'] == '0' else '000000B7') if row['operation'] == 'create' else 'A5A5A5A5'
                assert row['error'] == expected_error
                name = bytes.fromhex(row['input']).decode('utf-16-le', 'surrogatepass')
                expected = '\\BaseNamedObjects\\' + name.removeprefix('Local\\').split('\0')[0]
                assert bytes.fromhex(objects[156 + index]['units']).decode('utf-16-le', 'surrogatepass') == expected
        done = fields(lines[-1])
        assert done['cases'] == '156' and int(done['acquired']) == int(done['closed']) == handles
        assert done['before'] == done['after']
print('Verified 25200 query, 12800 create-fault, 200 release and 892 grant/lifecycle rows; naming revisions 4/5/6 and 48 Win32 identity comparisons; all replay projections match native bytes')
