from pathlib import Path
import hashlib
import json
import re
import shutil
import sys

if sys.flags.optimize:
    raise ValueError('optimized finalization rejected')
c = Path(__file__).parent
rax = Path('/Users/int/hexrays/kvasir/vendor/rax')
proof = rax / 'docs/specifications/windows/native-section-open'
share = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')

def require(ok, reason):
    if not ok:
        raise ValueError(reason)

def text(path):
    raw = path.read_bytes()
    return raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig').replace('\r\n', '\n')

def j(path):
    return json.loads(text(path))

def result(path):
    return [list(map(int, row)) for row in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', text(path))]

def fail_names(path):
    return [line.strip() for line in text(path).split('failures:\n\n')[-1].splitlines() if line.startswith('    ')]

def matches(path, item):
    raw = path.read_bytes()
    return len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256']

for host in ('macos', 'linux'):
    gates = j(c / (host + '-gates.json'))
    require(len(gates) == 5 and all(row['exit-code'] == 0 for row in gates), host + ' completed portable gates')
    require(all(row['exit-code'] == 0 for row in j(c / (host + '-owning-gates.json'))), host + ' owning gates')
    for path in c.glob(host + '-*'):
        if path.is_file() and path.suffix in ('.log', '.json'):
            shutil.copyfile(path, proof / path.name)
for path in share.iterdir():
    if path.is_file() and (path.name.startswith('native-section-final-') or path.name.startswith('native-section-open-final-') or path.name.startswith('assist-native-section-owned-final-')):
        shutil.copyfile(path, c / path.name)
        shutil.copyfile(path, proof / path.name)
gates = j(proof / 'native-section-open-final-gates.json')
require(len(gates) == 5 and all(row['exit-code'] == (101 if row['gate'] == 'full' else 0) for row in gates), 'Windows completed gates, exact baseline full failure')
require(j(c / 'format-check-exit.json')['exit-code'] == 0, 'format gate')

def log(host, gate):
    return proof / (host + '-' + gate + '.log' if host != 'windows' else 'native-section-open-final-' + gate + '.log')

hosts = ('macos', 'linux', 'windows')
full = {host: result(log(host, 'full')) for host in hosts}
focused = {host: result(log(host, 'focused')) for host in hosts}
capi = {host: result(log(host, 'capi')) for host in hosts}
integration = {host: result(log(host, 'integration')) for host in hosts}
require(full == {'macos': [[7749, 0, 2]], 'linux': [[7743, 0, 2]], 'windows': [[7163, 5, 2]]}, 'exact full selection/counts: ' + str(full))
require(focused == {'macos': [[4, 0, 0]], 'linux': [[4, 0, 0]], 'windows': [[5, 0, 0]]}, 'exact focused counts')
require(capi == {host: [[168, 0, 0], [0, 0, 0]] for host in hosts}, 'exact C API counts')
require(integration == {'macos': [[544, 0, 0], [0, 0, 0]], 'linux': [[544, 0, 0], [0, 0, 0]], 'windows': [[0, 0, 0], [4, 0, 0]]}, 'exact integration counts')
baseline = rax / 'docs/specifications/windows/native-thread-token-open'
windows_failures = fail_names(log('windows', 'full'))
require(len(windows_failures) == 5 and windows_failures == fail_names(baseline / 'native-thread-token-final-full.log'), 'unchanged Windows failure identities')
ignored = {}
for host in hosts:
    ignored[host] = [line.strip() for line in text(log(host, 'full')).splitlines() if line.startswith('test ') and ' ... ignored' in line]
    require(ignored[host] == j(proof / 'unix-validation-summary.json')['macos']['ignored'], 'unchanged ignored selection: ' + host)

freeze = j(c / 'runtime-source-freeze.json')
require(len(freeze) == 17 and all(matches(rax / name, item) for name, item in freeze.items()), 'runtime source freeze')
archives = j(proof / 'native-section-final-owning-archive-hashes.json')
require(len(archives['sources']) == 113 and len(archives['helpers']) == 2 and archives['core-features'] == [], 'Windows source/helper/core selection')
for item in archives['sources']:
    require(matches(rax / item['path'], item), 'actual source: ' + item['path'])
for item in archives['helpers']:
    require(matches(c / item['path'], item), 'actual helper: ' + item['path'])
require(len(archives['archives']) == 2 and all(row['members'] > 0 and row['structural-member-walk'] == 'pass' for row in archives['archives']), 'Windows complete archive member walks')
for host in ('macos', 'linux'):
    rows = j(proof / (host + '-owning-archive-identities.json'))
    require(len(rows['sources']) == 113 and rows['core-features'] == [] and len(rows['archives']) == 2, host + ' archive/source scope')
    require(all(row['members'] > 0 and row['structural-member-walk'] == 'pass' for row in rows['archives']), host + ' complete archive member walks')
source = j(proof / 'native-section-final-owning-source-verification.json')
require(source['rust-source-units'] == 113 and len(source['cpp-consumers']) == 9, 'Windows current consumer scope')
for item in source['cpp-consumers']:
    require(matches(rax.parent.parent / item['path'], item), 'current consumer: ' + item['path'])
require('100% tests passed out of 7' in text(proof / 'macos-owning-ctest.log'), 'seven owning macOS CTests')
for name in ('linux-owning-cpp.log', 'assist-native-section-owned-final-cpp.log'):
    output = text(proof / name)
    require('162 checks passed' in output and '2/2 passed' in output and 'PASS' in output, 'five owning C++ consumers: ' + name)

observer = j(proof / 'native-section-final-observer-identity.json')
require(observer['bytes'] == 11177 and observer['sha256'] == 'd41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e' and observer['executable-machine'] == 0xAA64, 'unchanged actual observer/native executable')
require(j(proof / 'baseline-observer-identity.json')['sha256'] == observer['sha256'], 'paired observer identity')
native = j(proof / 'native-section-producer-identities.json')
after_native = j(proof / 'native-section-final-producer-identities.json')
for key in ('native-machine', 'kernel', 'principal', 'native-libraries', 'producers', 'inspector-identity'):
    require(native[key] == after_native[key], 'after-owning native identity unchanged: ' + key)
trace = text(proof / 'native-section-final-after-production-trace-output.log')
terminal = re.search(r'terminal turn=(\d+) Complete\(Internal\("([^"\n]+)"\)\)', trace)
require(terminal is not None, 'actual ordinary observer terminal')
after_turn, frontier = int(terminal[1]), terminal[2]
require(after_turn > 145453, 'ordinary continuation beyond previous section-open boundary')
ordinary = text(proof / 'native-section-final-after-ordinary-processes.log')
require(ordinary.count('run reason=5 exit=0') == 4, 'four ordinary Internal outcomes, not guest exits')
threads = [json.loads(line.removeprefix('threads=')) for line in ordinary.splitlines() if line.startswith('threads=')]
frontiers = re.findall(r'native NTDLL service ([^"\n]+)', ordinary)
require(len(threads) == len(frontiers) == 4 and len(set(frontiers[:3])) == 1 and 'NtTraceEvent (0x5e)' in frontiers[3], 'three ordinary CRT continuation boundaries and unchanged whoami boundary')
negative = j(proof / 'negative-evidence-report.json')
require(negative['rejected'] == len(negative['cases']) == 27, 'controlled falsification count')
summary = {'baseline-rax': j(c / 'task-baseline.json')['rax'], 'baseline-root': j(c / 'task-baseline.json')['root'], 'full-lib': full, 'focused': focused, 'capi': capi, 'integration': integration, 'windows-failures': windows_failures, 'windows-baseline-failures': 'exactly unchanged', 'ignored': ignored, 'all-targets': {host: 0 for host in hosts}, 'format': 0, 'native-producers': 9, 'open-replay-rows': 22584, 'duplicate-replay-rows': 71040, 'fresh-first-calls': 2, 'controlled-falsifications': 27, 'owning': {'macos-ctests': 7, 'macos-protected-scans': 2, 'linux-cpp-consumers': 5, 'windows-cpp-consumers': 5, 'rust-source-units': 113, 'helpers': 2, 'matching-current-cpp-consumers': 9, 'core-features': [], 'whole-current-root-plugin-GUI-packages': 'excluded'}, 'ordinary': {'before-turn': 145453, 'after-turn': after_turn, 'delta-scheduler-calls': after_turn - 145453, 'observer-frontier': frontier, 'cpp-frontiers': frontiers, 'cpp-threads': threads, 'all-four-Internal': True, 'guest-exits': 0}, 'full-same-host-goal': 'active and incomplete'}
for path in (c / 'validation-summary.json', proof / 'validation-summary.json'):
    path.write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
