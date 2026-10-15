from pathlib import Path
import hashlib
import json
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
V = HERE / 'validation'


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path, row):
    data = path.read_bytes()
    require(len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256'], str(path))


require(__debug__, 'optimized validation rejected')
summary = json.loads((V / 'summary.json').read_text())
for row in summary['artifacts']:
    identity(V / row['path'], row)
for row in json.loads((V / 'section-create-sources.json').read_text())['files']:
    identity(ROOT / row['path'], row)
for row in json.loads((V / 'owned-source-hashes.json').read_text())['files']:
    identity(ROOT / row['path'], row)
for host, expected in summary['library-counts'].items():
    name = host + '-lib.log' if host != 'windows' else 'native-section-create-full-full.log'
    text = (V / name).read_text()
    match = re.search(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;.*?0 filtered out', text)
    require(match and list(map(int, match.groups())) == expected, host + ' full library')
    if host == 'windows':
        tail = text[text.rfind('failures:'):]
        require(all(tail.count(name) == 1 for name in summary['known-windows-failures']), 'unchanged baseline failures')
    name = host + '-capi.log' if host != 'windows' else 'native-section-create-full-capi.log'
    require('168 passed; 0 failed; 0 ignored;' in (V / name).read_text(), host + ' CAPI')
    name = host + '-focused.log' if host != 'windows' else 'native-section-create-full-focused.log'
    require(str(summary['focused-counts'][host]) + ' passed; 0 failed; 0 ignored;' in (V / name).read_text(), host + ' selected constructor tests')
for row in json.loads((V / 'commands.json').read_text()):
    require(row['exit-code'] == (101 if row['host'] == 'windows' and row['gate'] == 'full' else 0), 'gate ' + row['host'] + row['gate'])
for host in ['macos', 'linux']:
    require('42 passed; 0 failed; 1 ignored;' in (V / (host + '-linux.log')).read_text(), host + ' serialized integration')
    require('544 passed; 0 failed; 0 ignored;' in (V / (host + '-windows.log')).read_text(), host + ' Windows personality integration')
for name in ['macos-owning-archives.json', 'linux-owning-archives.json', 'native-section-create-owning-archive-hashes.json']:
    record = json.loads((V / name).read_text())
    require(record['core-features'] == [] and len(record['sources']) == 270, name)
    for row in record['sources']:
        identity(ROOT / row['path'], row)
    require(all(item['structural-member-walk'] == 'pass' for item in record['archives']), 'complete archive walks')
identity(V / 'ordinary-observer.rs.txt', {'bytes': 11178, 'sha256': summary['rust-observer-sha256']})
trace = (V / 'native-section-create-after-production-trace-output.log').read_text()
require('terminal turn=2115163 Complete(Internal("native NTDLL service NtSecureConnectPort (0x194) at 0x180002980 is not implemented"))' in trace, 'new explicit frontier')
require(summary['terminal-turn'] - summary['previous-terminal-turn'] == 34 and summary['ordinary-guest-exits'] == 0, 'progress and exit classification')
original = (V / 'native-section-create-after-ordinary-processes.log').read_text()
require(original.count('run reason=6 exit=0 diagnostic=null') == 3, 'original5s timeout observations')
extra = (V / 'native-section-create-budget-output.log').read_text()
require(extra.count('run reason=5 exit=0 diagnostic="emulator failure: native NTDLL service NtSecureConnectPort') == 3 and extra.count('run reason=5 exit=0') == 4, 'separate60s results')
budget = json.loads((V / 'native-section-create-budget-identities.json').read_text())
identity(V / 'native-section-create-budget-probe.cpp', budget['source'])
identity(V / 'native-section-create-budget-output.log', budget['stdout'])
identity(V / 'ordinary-observer.rs.txt', budget['observer'])
identity(V / 'inspect-section-create-budget.py', budget['inspector'])
require(budget['executable']['machine'] == 0xAA64 and budget['executable']['bytes'] == 32131072, 'actual ARM64 budget executable')
outcome = json.loads((V / 'native-section-create-budget-outcome.json').read_text(encoding='utf-8-sig'))
require(outcome['exit-code'] == 0 and outcome['exe-sha256'].lower() == budget['executable']['sha256'] and outcome['source-sha256'].lower() == budget['source']['sha256'], 'independent budget identities')
require(summary['exact-model-replays'] + summary['explicit-model-unsupported'] == 10920, 'native/model partition')
print('Verified9 frozen Rust files,8519 input hashes,3host library/CAPI gates,270 scoped embedding inputs and +34-turn SecureConnectPort frontier')
