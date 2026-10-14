from pathlib import Path
import hashlib
import json
import re

if not __debug__:
    raise RuntimeError('optimized final validation rejected')
BASE = Path(__file__).parent
REPOSITORY = BASE.parents[3]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def read(name):
    data = (BASE / name).read_bytes()
    return data.decode('utf-16' if data.startswith(b'\xff\xfe') else 'utf-8-sig').replace('\r\n', '\n')


def load(name):
    return json.loads(read(name))


def counts(name):
    return [list(map(int, row)) for row in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', read(name))]


def identity(path, expected):
    data = path.read_bytes()
    require(len(data) == expected['bytes'] and hashlib.sha256(data).hexdigest() == expected['sha256'], 'source identity: ' + str(path))


full = {'macos': counts('macos-full-full.log'), 'linux': counts('linux-full-full.log'), 'windows': counts('native-process-tls-full-full.log')}
require(full == {'macos': [[7776, 0, 2]], 'linux': [[7770, 0, 2]], 'windows': [[7192, 5, 2]]}, 'full library counts')
for host in ['macos', 'linux']:
    require(counts('macos-focused.log' if host == 'macos' else 'linux-full-focused.log') == [[6, 0, 0]], host + ' focused selection')
    require(all(row['exit-code'] == 0 for row in load(host + '-full-gates.json')), host + ' broad gates')
    require(counts(host + '-full-capi.log') == [[168, 0, 0], [0, 0, 0]], host + ' C API')
    require(counts(host + '-full-integration.log') == [[544, 0, 0], [0, 0, 0]], host + ' host selection')
require(counts('native-process-tls-corrected-focused.log') == [[6, 0, 0]], 'Windows focused selection')
require(counts('native-process-tls-full-capi.log') == [[168, 0, 0], [0, 0, 0]], 'Windows C API')
require(counts('native-process-tls-full-integration.log') == [[0, 0, 0], [4, 0, 0]], 'Windows native memory selection')
require(all(row['exit-code'] == (101 if row['gate'] == 'full' else 0) for row in load('native-process-tls-full-gates.json')), 'Windows broad gate exits')


def failures(text):
    return [line.strip() for line in text.split('failures:\n\n')[-1].splitlines() if line.startswith('    ')]


windows_failures = failures(read('native-process-tls-full-full.log'))
baseline = REPOSITORY / 'docs/specifications/windows/native-performance-counter/native-performance-counter-refined-full.log'
require(len(windows_failures) == 5 and windows_failures == failures(baseline.read_bytes().decode('utf-8-sig').replace('\r\n', '\n')), 'exact preceding five failure identities')
freeze = load('runtime-source-freeze.json')
require(len(freeze) == 6, 'exact changed Rust source scope')
for name, row in freeze.items():
    identity(REPOSITORY / name, row)
archives = {}
for host, filename in [('macos', 'macos-owning-archive-identities.json'), ('linux', 'linux-owning-archive-identities.json'), ('windows', 'native-process-tls-final-owning-archive-hashes.json')]:
    record = load(filename)
    require(len(record['sources']) == 213 and record['core-features'] == [], host + ' scoped source/features')
    for row in record['sources']:
        identity(REPOSITORY / row['path'], row)
    require(len(record['archives']) == 2 and all(row['structural-member-walk'] == 'pass' and row['members'] > 0 for row in record['archives']), host + ' archive walks')
    archives[host] = record['archives']
require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven owning CTests')
for host in ['macos', 'linux']:
    require(all(row['exit-code'] == 0 for row in load(host + '-owning-gates.json')), host + ' embedding success')
for name in ['linux-owning-cpp.log', 'assist-native-process-tls-owned-final-cpp.log']:
    require('162 checks passed' in read(name) and '2/2 passed' in read(name) and 'PASS' in read(name), 'C++ process/ABI/archive consumers')
source = load('native-process-tls-final-owning-source-verification.json')
require(source['rust-source-units'] == 213 and len(source['cpp-consumers']) == 9, 'current nine C++ consumers')
for row in source['cpp-consumers']:
    identity(REPOSITORY.parent.parent / row['path'], row)
negative = load('evidence-stress-results.json')
require(len(negative) == 8 and all(row['rejected'] for row in negative), 'eight controlled evidence failures')
observer = load('native-process-tls-final-observer-identity.json')
identity(BASE / 'native-process-tls-after-trace.rs', observer)
identity(BASE / 'native-process-tls-extended-trace.rs', observer['extended-observer'])
require(observer['sha256'] == 'd41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e', 'original observer bytes')
require(observer['extended-observer']['sha256'] == '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0', 'unchanged extended observer bytes')
require('diagnostic turn budget exhausted' in read('native-process-tls-final-after-production-trace-output.log'), 'original observer budget result')
terminal = re.search(r'terminal turn=(\d+) (.*)', read('native-process-tls-extended-trace-output.log'))
require(terminal is not None and 'NtApphelpCacheControl (0x4c)' in terminal[2] and int(terminal[1]) < 3000000, 'ordinary extended frontier')
ordinary = read('native-process-tls-final-after-ordinary-processes.log')
require(ordinary.count('run reason=5 exit=0') == 4 and ordinary.count('NtApphelpCacheControl (0x4c)') == 3 and 'NtTraceEvent (0x5e)' in ordinary, 'ordinary internal outcomes')
require(load('native-process-tls-length-order-identities.json')['accepted-total-control-count'] == 1040, 'native corpus count')
summary = {'baseline-rax': '26104da23cffb6876b739c330edde610dd47f5c8', 'full-lib': full,
           'focused-passed-per-host': 6, 'CAPI-passed-per-host': 168, 'native-calls': 1040,
           'portable-original-replay-calls': 176, 'caller-byte-comparisons': 160, 'vector-byte-comparisons': 172,
           'windows-baseline-failures': windows_failures, 'owning-scoped-rust-sources': 213,
           'matching-current-cpp-consumers': 9, 'core-features': [], 'archives': archives,
           'controlled-falsifications': 8, 'extended-terminal-turn': int(terminal[1]),
           'ordinary-frontier': terminal[2], 'ordinary-guest-exits': 0,
           'whole-current-root-plugin-GUI-packages': 'excluded', 'full-same-host-goal': 'active and incomplete',
           'source-note': 'full/focused Rust gates precede a complexity-comment correction; runtime statements and tests unchanged; final owning archives rebuilt from current exact source bytes'}
(BASE / 'validation-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps({'full-lib': full, 'scoped-sources': 213, 'extended-terminal-turn': int(terminal[1]), 'guest-exits': 0}))
