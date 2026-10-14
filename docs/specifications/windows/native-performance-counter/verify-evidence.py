from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import sys

if not __debug__:
    raise RuntimeError('optimized evidence validation rejected')

BASE = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path, recorded):
    data = path.read_bytes()
    require(len(data) == recorded['bytes'] and hashlib.sha256(data).hexdigest() == recorded['sha256'].lower(), 'actual recorded bytes: ' + path.name)


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]+)', line))


report = json.loads((BASE / 'native-performance-counter-identities.json').read_text())
require(report['native-machine'] == 0xAA64 and report['process-machine'] == 0, 'native ARM64 inspector')
require(report['kernel'] == [10, 0, 29683, 2, ''] and 'S-1-5-18' in report['principal'], 'measured kernel and SYSTEM subject')
require(len(report['libraries']) == 4, 'four selected native libraries')
for actual, recorded in zip(report['libraries'], json.loads((BASE / 'selected-library-baseline.json').read_text())):
    require(all(actual[key] == recorded[key] for key in ['path', 'bytes', 'sha256', 'machine', 'characteristics']), 'selected library identity preserved')
freeze = json.loads((BASE / 'source-freeze.json').read_text())
outcomes = json.loads((BASE / 'native-performance-counter-outcomes.json').read_text(encoding='utf-8-sig'))
require([row['profile'] for row in report['producers']] == ['arm64', 'x64', 'x86', 'x86-laa'], 'four complete producers')
identity(BASE / PureWindowsPath(report['inspector']['path']).name, report['inspector'])
counts = 0
monotonic = 0
status_guards = []
for producer in report['producers']:
    profile = producer['profile']
    source = BASE / PureWindowsPath(producer['source']['path']).name
    output = BASE / PureWindowsPath(producer['stdout']['path']).name
    identity(source, producer['source'])
    identity(source, freeze[source.name])
    identity(output, producer['stdout'])
    binary = producer['executable']
    require(binary['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'actual executable machine')
    require(bool(binary['characteristics'] & 0x20) == (profile != 'x86'), 'actual LAA')
    outcome = next(row for row in outcomes if row['profile'] == profile)
    require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == producer['source']['sha256'], 'executed source/exit')
    require(outcome['exe-sha256'].lower() == binary['sha256'] and outcome['exe-bytes'] == binary['bytes'], 'executed binary identity')
    text = output.read_text()
    rows = [fields(line) for line in text.splitlines() if line.startswith('case=')]
    require(len(rows) == 250 and [int(row['case']) for row in rows] == list(range(250)), 'ordered complete native matrix')
    require(all(row['escaped'] == '00000000' for row in rows), 'no collector exception')
    require('failure=' not in text and f'complete cases=250 monotonic=8 pointer-bytes={4 if profile.startswith("x86") else 8}' in text, 'complete native footer')
    calibration = fields(text.splitlines()[0])
    require(calibration == producer['calibration'] and calibration['status'] == '00000000', 'actual calibration')
    require(int(calibration['frequency']) > 0 and calibration['frequency'] == calibration['public-frequency'], 'native/public Hz agreement')
    status_guards.append([(row['status'], *(row['p' + str(page)] for page in range(8))) for row in rows])
    for row in rows:
        require(int(row['before']) <= int(row['after']), 'ordered clock bracket')
        changed = [key for key in row if key.startswith('b') and key[1:].isdigit()]
        require(len(changed) == int(row['changed']) and all(int(key[1:]) < 32768 for key in changed), 'bounded exact changed-byte record')
        require(all(row['status'] in ['00000000', '80000001', '80000002', 'C0000005'] for row in rows), 'measured status partition')
    sequences = [fields(line) for line in text.splitlines() if line.startswith('monotonic ')]
    require(len(sequences) == 8 and [int(row['case']) for row in sequences] == list(range(8)), 'complete monotonic sequence')
    require(all(int(row['delta']) == int(row['current']) - int(row['previous']) >= 0 for row in sequences), 'actual monotonic deltas')
    counts += len(rows)
    monotonic += len(sequences)
require(all(rows == status_guards[0] for rows in status_guards), 'four-profile status/guard equivalence on the same kernel')
primary = json.loads((BASE / 'primary-contracts.json').read_text())
repository = Path(__file__).resolve().parents[4]
identity(repository / primary['header-relative-path'], primary)
require(all(url.startswith('https://learn.microsoft.com/') for url in primary['primary-documents']), 'primary API provenance')
print(json.dumps({'native-calls': counts, 'monotonic-observations': monotonic, 'producer-profiles': 4, 'physical-kernel': 'ARM64; x64/x86 compatibility processes'}, sort_keys=True))
