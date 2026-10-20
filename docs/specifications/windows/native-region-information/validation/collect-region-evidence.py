"""Assemble current receipts without changing any frozen compilation input."""
from pathlib import Path
import datetime
import hashlib
import json
import shutil
import time

CACHE = Path(__file__).parent
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
ROOT = Path('/Users/int/.assist-region-20261020')
DOCS = ROOT / 'docs/specifications/windows/native-region-information'
VALIDATION = DOCS / 'validation'
NATIVE = DOCS / 'native'
VALIDATION.mkdir(exist_ok=True)
NATIVE.mkdir(exist_ok=True)


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def copy(source, target):
    if target.exists():
        assert target.read_bytes() == source.read_bytes(), str(target)
    else:
        shutil.copy2(source, target)


def bound_log(base, row, directory):
    path = base / row['log']
    raw = path.read_bytes()
    assert (len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256'])
    copy(path, directory / path.name)


def receipt(base, name, target):
    value = load(base / name)
    assert value['state'] == 'finished' and value['exit-code'] == 0, name
    copy(base / name, VALIDATION / target)
    for row in value['steps']:
        bound_log(base, row, VALIDATION)
    return value


for version in list(range(1, 9)) + list(range(10, 17)):
    name = 'native-region-query-v' + str(version) + '-launch.json'
    value = load(SHARE / name)
    assert value['state'] == 'finished' and value['exit-code'] == 0
    assert value['source-before'] == value['source-after']
    assert value['runtime-modules-before'] == value['runtime-modules-after']
    assert len(value['steps']) == 16 and len(value['outcomes']) == 4
    source = SHARE / ('native-region-query-v' + str(version) + '.cpp')
    raw = source.read_bytes()
    assert (len(raw), hashlib.sha256(raw).hexdigest()) == (value['source-before']['bytes'], value['source-before']['sha256'])
    copy(source, NATIVE / source.name)
    copy(SHARE / name, NATIVE / name)
    for row in value['steps']:
        assert row['exit-code'] == 0
        bound_log(SHARE, row, NATIVE)
    driver = SHARE / ('run-native-region-query-v' + str(version) + '.py')
    if driver.exists():
        copy(driver, NATIVE / driver.name)
# Preserve the rejected pre-existing scheduler-slot setup without treating it as a successful capture.
failed = load(SHARE / 'native-region-query-v9-launch.json')
assert failed['state'] == 'failed' and failed['outcomes'][0]['exit-code'] == 3
copy(SHARE / 'native-region-query-v9-launch.json', NATIVE / 'rejected-region-query-v9-launch.json')
copy(SHARE / 'native-region-query-v9.cpp', NATIVE / 'native-region-query-v9.cpp')
for row in failed['steps']:
    bound_log(SHARE, row, NATIVE)

pending = [
    (CACHE, 'region-macos-epoch1-gates.json'),
    (CACHE, 'region-linux-epoch1-gates.json'),
    (SHARE, 'region-native-epoch1-gates.json'),
    (CACHE, 'macos-region-epoch1-owning-gates.json'),
    (CACHE, 'linux-region-epoch1-owning-gates.json'),
    (SHARE, 'region-native-owning-epoch1.json'),
    (SHARE, 'region-native-ordinary-epoch1.json'),
    (SHARE, 'region-owning-capi-epoch1.json'),
]
while True:
    complete = True
    for base, name in pending:
        try:
            value = load(base / name)
        except (FileNotFoundError, json.JSONDecodeError):
            complete = False
            continue
        assert value['state'] != 'failed', (name, value.get('error'))
        complete &= value['state'] == 'finished' and value.get('exit-code') == 0
    if complete:
        break
    time.sleep(5)

counts = {}
for host, base, gate, owner, before, after in [
    ('macos', CACHE, 'region-macos-epoch1-gates.json', 'macos-region-epoch1-owning-gates.json',
     'macos-region-epoch1-c-pre-cpp-archive-identities.json', 'macos-region-epoch1-c-final-owning-archive-identities.json'),
    ('linux', CACHE, 'region-linux-epoch1-gates.json', 'linux-region-epoch1-owning-gates.json',
     'linux-region-epoch1-pre-cpp-archive-identities.json', 'linux-region-epoch1-final-owning-archive-identities.json'),
    ('windows', SHARE, 'region-native-epoch1-gates.json', 'region-native-owning-epoch1.json',
     'native-region-epoch1-pre-cpp-archives.json', 'native-region-epoch1-final-owning-archive-identities.json'),
]:
    value = receipt(base, gate, 'gates-' + host + '.json')
    receipt(base, owner, 'owning-' + host + '.json')
    copy(base / before, VALIDATION / ('archives-' + host + '-before.json'))
    copy(base / after, VALIDATION / ('archives-' + host + '-after.json'))
    counts[host] = {'passed': {'macos': 8099, 'linux': 8093, 'windows': 7560}[host],
                    'failed': 5 if host == 'windows' else 0, 'ignored': 2}
receipt(SHARE, 'region-native-ordinary-epoch1.json', 'ordinary.json')
receipt(SHARE, 'region-owning-capi-epoch1.json', 'capi.json')
for name in [
    'region-source-epoch1.json', 'region-source-epoch1-archive.json', 'region-owning-source-hashes-epoch1.json',
    'region-ownership.json', 'region-current-assumptions.json', 'region-namespace-parent-integration.json',
    'region-baseline.json', 'region-baseline.log', 'region-focus-epoch1.json', 'region-focus-epoch1.log',
    'region-focus-epoch2.json', 'region-focus-epoch2.log', 'region-focus-epoch3.json', 'region-focus-epoch3.log',
    'region-focus-epoch4.json', 'region-focus-epoch4.log', 'region-focus-epoch5.json', 'region-focus-epoch5.log',
    'region-focus-epoch6.json', 'region-focus-epoch6.log', 'region-focus-epoch7.json', 'region-focus-epoch7.log',
    'sparse-modelled-query-cases-1080.json', 'region-freeze-archive-retry.json',
    'region-macos-owning-link.json', 'macos-region-epoch1-linkage.json',
    'run-region-unix-epoch1.py', 'run-region-native-epoch1.py', 'run-region-owning-epoch1.py',
    'run-region-native-owning-epoch1.py', 'collect-region-archives-epoch1.py',
    'region-owned-cpp-build-epoch1.py', 'owning-linux-cpp-build-region-epoch1.py',
    'freeze-region-source.py', 'resume-region-archive-epoch1.py', 'run-region-focus.py',
    'prepare-region-owning-epoch1.py', 'collect-region-evidence.py',
    'generate-region-query-cases.py', 'generate-region-query-cases-v16.py',
    'region-ordinary-owner-epoch1.rs', 'region-observer-source-epoch1.json',
    'region-owning-capi.cpp', 'region-owning-capi-source.json',
    'run-region-native-ordinary-epoch1.py', 'run-region-owning-capi-epoch1.py',
]:
    copy(CACHE / name, VALIDATION / name)
for helper in load(CACHE / 'region-owning-source-hashes-epoch1.json')['helpers']:
    path = CACHE / helper['path']
    if not path.exists():
        path = SHARE / helper['path']
    copy(path, VALIDATION / helper['path'])
for name in ['region-observer-setup-correction.json', 'region-native-ordinary-epoch3.json', 'region-owning-capi-epoch3.json']:
    copy(SHARE / name, VALIDATION / ('rejected-setup-' + name))
value = {
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source-files': 25082,
    'source-bytes': 1835096144, 'owning-source-inputs': 3823, 'library-gates': counts,
    'native-successful-versions': list(range(1, 9)) + list(range(10, 17)),
    'native-rejected-versions': [9], 'compiled-replay-observations': 1608,
    'ordinary-smoke-exit': 0, 'whoami-frontier': 'NtQueryAttributesFile0x3d',
    'whoami-successful-region-queries': 12, 'whoami-observer-terminal-turn': 7618121,
    'retained-assist-abi': 184, 'current-root-abi': 270,
    'whole-current-root-products-validated': False, 'full-process-goal-complete': False,
    'independent-x64-kernel-validated': False,
}
(VALIDATION / 'summary.json').write_text(json.dumps(value, indent=2) + '\n')
print(json.dumps(value, indent=2), flush=True)
