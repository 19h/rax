from pathlib import Path
import hashlib
import json
import re
import sys

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


def identity(path, item):
    raw = path.read_bytes()
    require(len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256'], 'source bytes: ' + str(path))


require(load('format-check-exit.json')['exit-code'] == 0, 'owned formatter gate')
full = {host: counts(host + '-refined-full.log') for host in ['macos', 'linux']}
full['windows'] = counts('native-performance-counter-refined-full.log')
require(full == {'macos': [[7770, 0, 2]], 'linux': [[7764, 0, 2]], 'windows': [[7186, 5, 2]]}, 'final full library counts')
for host in ['macos', 'linux']:
    require(all(row['exit-code'] == 0 for row in load(host + '-refined-gates.json')), host + ' refined gates')
    require(len(load(host + '-full-gates.json')) == 5 and all(row['exit-code'] == 0 for row in load(host + '-full-gates.json')), host + ' first broad gates')
    require(counts(host + '-full-capi.log') == [[168, 0, 0], [0, 0, 0]], host + ' C API')
    require(counts(host + '-full-integration.log') == [[544, 0, 0], [0, 0, 0]], host + ' integration')
    require(counts(host + '-refined-clock.log') == [[4, 0, 0]], host + ' full clock selection')
require(counts('native-performance-counter-refined-clock.log') == [[4, 0, 0]], 'native Windows clock selection')
require(counts('native-performance-counter-full-capi.log') == [[168, 0, 0], [0, 0, 0]], 'Windows C API')
require(counts('native-performance-counter-full-integration.log') == [[0, 0, 0], [4, 0, 0]], 'Windows host integration selection')
require(counts('native-performance-counter-full-focused.log') == [[8, 0, 0]], 'eight Windows focused tests')
require(all(counts(host + '-full-focused.log') == [[7, 0, 0]] for host in ['macos', 'linux']), 'seven Unix focused tests')


def failures(text):
    return [line.strip() for line in text.split('failures:\n\n')[-1].splitlines() if line.startswith('    ')]


windows_failures = failures(read('native-performance-counter-refined-full.log'))
baseline = REPOSITORY / 'docs/specifications/windows/native-section-map/upstream-integration/native-section-map-integrated-full.log'
require(len(windows_failures) == 5 and windows_failures == failures(baseline.read_bytes().decode('utf-8-sig').replace('\r\n', '\n')), 'exact five baseline failures')
freeze = load('runtime-source-freeze.json')
require(len(freeze) == 16, 'exact changed source count')
for name, item in freeze.items():
    identity(REPOSITORY / name, item)
archives = {}
for host, filename in [('macos', 'macos-owning-archive-identities.json'), ('linux', 'linux-owning-archive-identities.json'), ('windows', 'native-performance-counter-final-owning-archive-hashes.json')]:
    record = load(filename)
    require(len(record['sources']) == 210 and record['core-features'] == [], host + ' owning source/feature scope')
    for item in record['sources']:
        identity(REPOSITORY / item['path'], item)
    require(len(record['archives']) == 2 and all(row['members'] > 0 and row['structural-member-walk'] == 'pass' for row in record['archives']), host + ' complete archive walk')
    archives[host] = record['archives']
require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven owning CTests')
for filename in ['linux-owning-cpp.log', 'assist-native-performance-counter-owned-final-cpp.log']:
    output = read(filename)
    require('162 checks passed' in output and '2/2 passed' in output and 'PASS' in output, 'five owning C++ consumers')
for host in ['macos', 'linux']:
    require(all(row['exit-code'] == 0 for row in load(host + '-owning-gates.json')), host + ' owning terminal success')
source_report = load('native-performance-counter-final-owning-source-verification.json')
require(source_report['rust-source-units'] == 210 and len(source_report['cpp-consumers']) == 9, 'nine matching consumers')
for item in source_report['cpp-consumers']:
    identity(REPOSITORY.parent.parent / item['path'], item)
negative = load('evidence-stress-results.json')
require(len(negative) == 10 and all(row['rejected'] for row in negative), 'ten evidence corruptions rejected')
observer = load('native-performance-counter-final-observer-identity.json')
identity(BASE / 'native-performance-counter-after-trace.rs', observer)
require(observer['bytes'] == 11177 and observer['sha256'] == 'd41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e', 'original observer unchanged')
extended = observer['extended-observer']
identity(BASE / 'native-performance-counter-extended-trace.rs', extended)
original = (BASE / 'native-performance-counter-after-trace.rs').read_bytes()
modified = (BASE / 'native-performance-counter-extended-trace.rs').read_bytes()
require(modified.replace(b'0..3000000', b'0..1000000').replace(b'turn == 2999999', b'turn == 999999') == original, 'only trace budget literals changed')
require('diagnostic turn budget exhausted' in read('native-performance-counter-final-after-production-trace-output.log'), 'retained original observer budget outcome')
terminal = re.search(r'terminal turn=(\d+) (.*)', read('native-performance-counter-extended-trace-output.log'))
require(terminal is not None and 'NtSetInformationProcess class 35' in terminal[2] and int(terminal[1]) < 3000000, 'actual extended observer frontier')
ordinary = read('native-performance-counter-final-after-ordinary-processes.log')
require(ordinary.count('run reason=5 exit=0') == 4 and ordinary.count('NtSetInformationProcess class 35') == 3 and 'NtTraceEvent (0x5e)' in ordinary, 'four ordinary outcomes remain Internal')
summary = {
    'baseline-rax': '2de987e04ee60f3fe7dfb442c129f31c6ea27bdd', 'full-lib': full,
    'focused': {'macos': 7, 'linux': 7, 'windows': 8}, 'native-calls': 1000,
    'monotonic-observations': 32, 'native-physical-kernel': 'ARM64; x64/x86 compatibility processes',
    'CAPI-passed-per-host': 168, 'windows-baseline-failures': windows_failures,
    'native-frequency-Hz': 24000000, 'builtin-frequency-Hz': 1000000000,
    'owning-scoped-rust-sources': 210, 'matching-current-cpp-consumers': 9,
    'core-features': [], 'archives': archives, 'controlled-falsifications': 10,
    'original-observer': {'bytes': 11177, 'budget': 1000000, 'outcome': 'budget exhausted'},
    'extended-observer': {'budget': 3000000, 'terminal-turn': int(terminal[1]), 'outcome': terminal[2], 'only-budget-literals-changed': True},
    'ordinary-guest-exits': 0, 'whole-current-root-plugin-GUI-packages': 'excluded',
    'retained-failures': ['missing test import: first compile', 'first Windows full CPU-attribution timing fixture failure; strict assertion retained with startup excluded and250ms worker'],
    'full-same-host-goal': 'active and incomplete',
}
(BASE / 'validation-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps({'full-lib': full, 'scoped-sources': 210, 'extended-terminal-turn': int(terminal[1]), 'guest-exits': 0}))
