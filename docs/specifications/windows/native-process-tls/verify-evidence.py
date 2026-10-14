from pathlib import Path
import hashlib
import json
import re
import sys

if not __debug__:
    raise RuntimeError('optimized evidence validation rejected')

BASE = Path(__file__).parent
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
GROUPS = [
    ('control', range(44), 'native-process-tls-initial-identities.json'),
    ('extended-control', range(44, 146), 'native-process-tls-extended-identities.json'),
    ('alias-control', range(146, 180), 'native-process-tls-alias-identities.json'),
    ('length-order', range(190, 270), 'native-process-tls-length-order-identities.json'),
]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path):
    data = path.read_bytes()
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def verify_identity(path, row):
    actual = identity(path)
    require(all(actual[key] == row[key] for key in actual), 'bound file identity: ' + path.name)


total = 0
baseline = json.loads((BASE / GROUPS[0][2]).read_text())
for group, ordinals, metadata_name in GROUPS:
    metadata = json.loads((BASE / metadata_name).read_text())
    require(metadata['kernel'] == baseline['kernel'] and metadata['principal'] == baseline['principal'], 'same native kernel and principal')
    require(metadata['libraries'] == baseline['libraries'], 'same four installed library identities')
    require(metadata['native-machine'] == 0xAA64 and metadata['process-machine'] == 0, 'native ARM64 inspector')
    inspector_name = Path(metadata['inspector']['path'].replace('\\', '/')).name
    verify_identity(BASE / inspector_name, metadata['inspector'])
    outcomes = json.loads((BASE / ('native-process-tls-' + group + '-outcomes.json')).read_text(encoding='utf-8-sig'))
    require([row['profile'] for row in outcomes] == PROFILES, 'complete ordered execution profiles')
    rows_by_profile = {}
    for profile in PROFILES:
        stem = 'native-process-tls-' + group + '-' + profile
        producer = next(row for row in metadata['producers'] if row['profile'] == profile and (group != 'control' or row['group'] == group))
        require(producer['cases'] == len(ordinals), 'native producer case count')
        verify_identity(BASE / ('native-process-tls-' + group + '.cpp'), producer['source'])
        verify_identity(BASE / (stem + '.log'), producer['stdout'])
        outcome = next(row for row in outcomes if row['profile'] == profile)
        require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == producer['source']['sha256'], 'source and native execution exit')
        binary = producer['executable']
        require(outcome['exe-sha256'].lower() == binary['sha256'] and outcome['exe-bytes'] == binary['bytes'], 'actual inspected executable identity')
        require(binary['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'compiled producer architecture')
        require(bool(binary['characteristics'] & 0x20) == (profile != 'x86'), 'actual LAA')
        text = (BASE / (stem + '.log')).read_text()
        rows = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('result ')]
        exits = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('child-exit ')]
        require([int(row['case']) for row in rows] == list(ordinals), 'exact ordered native calls')
        require([int(row['case']) for row in exits] == list(ordinals) and all(row['code'] == '77' for row in exits), 'all owned children reached collector exit')
        require(all(row['escaped'] == '00000000' for row in rows), 'no escaped call exceptions')
        require(text.splitlines()[-1] == f'complete cases={len(ordinals)} child-failures=0 width={4 if profile.startswith("x86") else 8}' and 'failure=' not in text, 'complete native collector footer')
        rows_by_profile[profile] = {int(row['case']): row for row in rows}
        total += len(rows)
    for left, right in [('arm64', 'x64'), ('x86', 'x86-laa')]:
        require({n: (row['status'], row['input-protect']) for n, row in rows_by_profile[left].items()} ==
                {n: (row['status'], row['input-protect']) for n, row in rows_by_profile[right].items()}, 'matched architecture status/guard policy')

require(total == 1040, 'complete accepted native call count')
final = json.loads((BASE / 'native-process-tls-length-order-identities.json').read_text())
require(final['accepted-total-control-count'] == total and final['accepted-length-order-control-count'] == 320 and final['excluded-duplicate-control-count'] == 176, 'aggregate accepted and excluded counts')
contracts = json.loads((BASE / 'primary-contracts.json').read_text())
for row in contracts['files']:
    verify_identity(BASE / row['path'], row)
require(contracts['phnt-commit'] == '53fbbdc5b5d2b08761db1c7b26bfa8c820924356', 'pinned primary declaration')
print('Verified 1040 native calls across four profiles; 176 duplicate controls excluded.')
