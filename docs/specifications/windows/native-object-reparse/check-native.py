"""Verify independent native link targets and bounded directory reparsing."""
from pathlib import Path
import hashlib
import json
import re

D = Path(__file__).resolve().parent
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
SOURCES = {
    'native-object-reparse-v2.cpp': 'a8ea0766aa6d744dc2155d053a694fd9b5111cbe07d59574d2162992fa327204',
    'native-targets/native-base-links-probe.cpp': 'd5a041138f2fbef1962fcac3e5894a81af4ff2625dd60a080ead898a194e2a28',
}


def need(condition, message):
    if not condition:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


def text(value):
    need(len(value) % 4 == 0, 'UTF-16 word extent')
    return ''.join(chr(int(value[index:index + 4], 16)) for index in range(0, len(value), 4))


need(__debug__, 'optimized verifier rejected')
for row in load(D / 'native-inputs.json')['files']:
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], 'native input ' + row['path'])
for name, expected in SOURCES.items():
    need(hashlib.sha256((D / name).read_bytes()).hexdigest() == expected, 'native producer source')
reparse_outcomes = load(D / 'native-object-reparse-v2-outcomes.json')
target_outcomes = load(D / 'native-targets/native-base-links-outcomes.json')
for outcomes in [reparse_outcomes, target_outcomes]:
    need([r['profile'] for r in outcomes] == PROFILES and all(r['exit-code'] == 0 for r in outcomes), 'four successful native profiles')

for folder, prefix, source_path in [(D, 'native-object-reparse-v2-', 'native-object-reparse-v2.cpp'),
                                   (D / 'native-targets', 'native-base-links-', 'native-targets/native-base-links-probe.cpp')]:
    before = load(folder / (prefix + 'modules-before.json'))
    after = load(folder / (prefix + 'modules-after.json'))
    need(before == after and {r['Hash'].lower() for r in before} == {
        '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
        'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'}, 'stable selected NTDLL images')
    for profile, machine in zip(PROFILES, [0xAA64, 0x8664, 0x14C, 0x14C]):
        before = load(folder / (prefix + profile + '-before.json'))
        after = load(folder / (prefix + profile + '-after.json'))
        need(before == after and before['machine'] == machine and before['bytes'] > 0, 'native executable ABI and identity')
        need(before['large-address-aware'] == (profile != 'x86'), 'native LAA profile')
        source_after = load(folder / (prefix + profile + '-source-after.json'))
        need(source_after['Hash'].lower() == SOURCES[source_path], 'native source stayed unchanged')

counts = [*range(73), 128, 256, 1024]
flags = [0, 0x40, 0x100, 0x140, 0x1000, 0x1040, 0x1100, 0x1140]
for result in reparse_outcomes:
    lines = (D / result['log']).read_text().splitlines()
    rows = [fields(line) for line in lines if line.startswith('reparse ')]
    need(len(rows) == 608, 'native directory reparse count')
    successful = 0
    for index, (count, flag) in enumerate((c, f) for c in counts for f in flags):
        row = rows[index]
        expected = (0xC000050B if count and flag & 0x1000 else
                    0xC0000034 if count >= 64 else 0xC000000D if flag & 0x100 else 0)
        need(int(row['case']) == index and int(row['links']) == count and int(row['flags'], 16) == flag,
             'complete chain/flag matrix')
        need(int(row['status'], 16) == expected, 'native chain or flag result')
        if expected == 0:
            need(row.get('canonical-base') == '1', 'native shared directory identity')
            successful += 1
    end = fields(lines[-1])
    need(end['cases'] == '608' and int(end['opened']) == int(end['closed']) == successful == 130
         and end['before'] == end['after'], 'native reparse handle lifetime')

expected_targets = {
    '\\BaseNamedObjects\\Local': '\\BaseNamedObjects',
    '\\BaseNamedObjects\\Global': '\\BaseNamedObjects',
    '\\BaseNamedObjects\\Session': '\\Sessions\\BNOLINKS',
    '\\Sessions\\1\\BaseNamedObjects\\Local': '\\Sessions\\1\\BaseNamedObjects',
    '\\Sessions\\1\\BaseNamedObjects\\Global': '\\BaseNamedObjects',
    '\\Sessions\\1\\BaseNamedObjects\\Session': '\\Sessions\\BNOLINKS',
}
observed = []
for result in target_outcomes:
    lines = (D / 'native-targets' / result['log']).read_text().splitlines()
    need(fields(lines[0])['process-session'] == '0', 'session-one paths use the actual session-zero caller')
    rows = [fields(line) for line in lines if line.startswith('link ')]
    normalized = []
    need(len(rows) == 6, 'six explicit native link targets')
    for row, (path, target) in zip(rows, expected_targets.items()):
        need(row['open'] == row['query'] == '00000000' and text(row['path']) == path
             and text(row['target']) == target, 'native target mapping')
        need(int(row['length']) == len(target.encode('utf-16-le')) and
             int(row['required']) == int(row['length']) + 2, 'counted target extent')
        normalized.append({k: row[k] for k in ['open','query','length','required']} | {'path':path,'target':target})
    end = fields(lines[-1])
    need(end['opens'] == end['closed'] == '6' and end['handles-before'] == end['handles-after'], 'native target handle lifetime')
    observed.append({'profile': result['profile'], 'links': normalized})
need(observed == load(D / 'native-targets/observations.json'), 'target summary independently reconstructed')
print('Native object-manager proof:2432 reparse rows,24 link targets, all handles closed')
