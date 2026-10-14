from pathlib import Path
import hashlib
import json
import re
import sys

if not __debug__:
    raise RuntimeError('optimized integrated validation rejected')

BASE = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
REPOSITORY = BASE.parent.parents[3]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def read(name):
    data = (BASE / name).read_bytes()
    encoding = 'utf-16' if data.startswith(b'\xff\xfe') else 'utf-8-sig'
    return data.decode(encoding).replace('\r\n', '\n')


def load(name):
    return json.loads(read(name))


def test_results(name):
    return [list(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', read(name))]


def verify_bytes(path, recorded):
    data = path.read_bytes()
    require(len(data) == recorded['bytes'] and hashlib.sha256(data).hexdigest() == recorded['sha256'], 'source identity: ' + str(path))


full = {host: test_results(host + '-integrated-full.log') for host in ['macos', 'linux']}
full['windows'] = test_results('native-section-map-integrated-full.log')
require(full == {'macos': [[7764, 0, 2]], 'linux': [[7758, 0, 2]], 'windows': [[7179, 5, 2]]}, 'combined full library counts')
focused = {host: test_results(host + '-integrated-focused.log') for host in ['macos', 'linux']}
focused['windows'] = test_results('native-section-map-integrated-focused.log')
require(focused == {'macos': [[4, 0, 0]], 'linux': [[4, 0, 0]], 'windows': [[5, 0, 0]]}, 'combined focused counts')
capi = {host: test_results(host + '-integrated-capi.log') for host in ['macos', 'linux']}
capi['windows'] = test_results('native-section-map-integrated-capi.log')
require(capi == {host: [[168, 0, 0], [0, 0, 0]] for host in ['macos', 'linux', 'windows']}, 'combined C API selection')
integration = {host: test_results(host + '-integrated-integration.log') for host in ['macos', 'linux']}
integration['windows'] = test_results('native-section-map-integrated-integration.log')
require(integration == {'macos': [[544, 0, 0], [0, 0, 0]], 'linux': [[544, 0, 0], [0, 0, 0]], 'windows': [[0, 0, 0], [4, 0, 0]]}, 'combined host integration selection')
for host in ['macos', 'linux']:
    gates = load(host + '-integrated-gates.json')
    require(len(gates) == 5 and all(row['exit-code'] == 0 for row in gates), host + ' completed combined gates')
    require(all(row['exit-code'] == 0 for row in load(host + '-owning-gates.json')), host + ' completed owning gates')
windows_gates = load('native-section-map-integrated-gates.json')
require(len(windows_gates) == 5 and all(row['exit-code'] == (101 if row['gate'] == 'full' else 0) for row in windows_gates), 'combined Windows gates retain baseline failure')


def failure_names(text):
    return [line.strip() for line in text.split('failures:\n\n')[-1].splitlines() if line.startswith('    ')]


windows_failures = failure_names(read('native-section-map-integrated-full.log'))
original_failures = failure_names((BASE.parent / 'native-section-map-latest-full.log').read_bytes().decode('utf-8-sig').replace('\r\n', '\n'))
require(len(windows_failures) == 5 and windows_failures == original_failures, 'five baseline failure identities unchanged')
archive_records = {}
for host, name in [('macos', 'macos-owning-archive-identities.json'), ('linux', 'linux-owning-archive-identities.json'), ('windows', 'native-section-map-final-owning-archive-hashes.json')]:
    evidence = load(name)
    require(evidence['core-features'] == [] and len(evidence['sources']) == 201, host + ' combined source/feature selection')
    for item in evidence['sources']:
        verify_bytes(REPOSITORY / item['path'], item)
    require(len(evidence['archives']) == 2 and all(row['members'] > 0 and row['structural-member-walk'] == 'pass' for row in evidence['archives']), host + ' complete archive member walks')
    archive_records[host] = evidence['archives']
source_report = load('native-section-map-final-owning-source-verification.json')
require(source_report['rust-source-units'] == 201 and len(source_report['cpp-consumers']) == 9, 'current scoped native sources and consumers')
for item in source_report['cpp-consumers']:
    verify_bytes(REPOSITORY.parent.parent / item['path'], item)
require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven owning macOS CTests')
for name in ['linux-owning-cpp.log', 'assist-native-section-map-map-owned-final-cpp.log']:
    output = read(name)
    require('162 checks passed' in output and '2/2 passed' in output and 'PASS' in output, 'five owning consumers: ' + name)
for name in ['macos-owning-protected-assist-cli.log', 'macos-owning-protected-assist-mcp.log']:
    require('0 failures' in read(name), 'protected executable scan')
upstream = load('section-map-upstream-overlay-manifest.json')
require(upstream['upstream'] == '0705e87135c6f6265e9ec618704041c3febcdf7e' and len(upstream['files']) == 196, 'selected upstream source change')
for item in upstream['files']:
    verify_bytes(REPOSITORY / item['path'], item)
transfer = load('section-map-upstream-overlay-verification.json')
require(transfer['files'] == 196 and transfer['upstream'] == upstream['upstream'] and transfer['actual-native-source-bytes'] == 'pass', 'actual native upstream transfer')
for name, item in load('runtime-source-freeze.json').items():
    verify_bytes(REPOSITORY / name, item)
observer = load('native-section-map-final-observer-identity.json')
require(observer['bytes'] == 11177 and observer['sha256'] == 'd41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e' and observer['executable-machine'] == 0xAA64, 'unchanged executed observer')
verify_bytes(BASE.parent / 'native-section-map-after-trace.rs', observer)
terminal = re.search(r'terminal turn=(\d+) Complete\(Internal\("([^"\n]+)"\)\)', read('native-section-map-final-after-production-trace-output.log'))
require(terminal is not None and int(terminal[1]) == 168227 and 'NtQueryPerformanceCounter (0x31)' in terminal[2], 'actual combined ordinary continuation')
ordinary = read('native-section-map-final-after-ordinary-processes.log')
frontiers = re.findall(r'native NTDLL service ([^"\n]+)', ordinary)
require(ordinary.count('run reason=5 exit=0') == 4 and len(frontiers) == 4 and len(set(frontiers[:3])) == 1 and 'NtTraceEvent (0x5e)' in frontiers[3], 'four ordinary Internal outcomes; no guest exits')
summary = {
    'upstream': upstream['upstream'], 'mapping-rebased-commit': upstream['rebased-mapping'],
    'full-lib': full, 'focused': focused, 'capi': capi, 'integration': integration,
    'windows-failures': windows_failures, 'windows-baseline-failures': 'exactly unchanged',
    'scoped-rust-source-units': 201, 'mapping-windows-source-units': 124,
    'upstream-rust-source-units': 77, 'upstream-transferred-files': 196,
    'matching-current-cpp-consumers': 9, 'core-features': [], 'archives': archive_records,
    'ordinary': {'before-turn': 161784, 'after-turn': 168227, 'delta-scheduler-calls': 6443, 'frontier': terminal[2], 'cpp-frontiers': frontiers, 'guest-exits': 0},
    'whole-current-root-plugin-GUI-packages': 'excluded', 'full-same-host-goal': 'active and incomplete',
}
(BASE / 'validation-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps({'full-lib': full, 'scoped-sources': 201, 'ordinary-turns': 168227, 'guest-exits': 0}))
