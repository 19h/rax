from pathlib import Path
import datetime
import hashlib
import json
import re
import shutil
import time

ROOT = Path('/Users/int/.assist-namespace-20261020')
CACHE = Path(__file__).parent
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
DOCS = ROOT / 'docs/specifications/windows/native-namespace-diagnostics'
VALIDATION = DOCS / 'validation'


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def copy(path, name=None):
    target = VALIDATION / (name or path.name)
    if target.exists():
        assert target.read_bytes() == path.read_bytes(), str(target)
    else:
        shutil.copy2(path, target)


def collect_receipt(base, name, target):
    value = load(base / name)
    assert value['state'] == 'finished' and value['exit-code'] == 0
    copy(base / name, target)
    for row in value['steps']:
        path = base / row['log']
        raw = path.read_bytes()
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256'])
        copy(path)
    return value


def main():
    items = [
        (CACHE, 'namespace-macos-epoch1-gates.json'),
        (CACHE, 'namespace-linux-epoch1b-gates.json'),
        (SHARE, 'namespace-native-epoch1-gates.json'),
        (CACHE, 'macos-namespace-epoch1-owning-gates.json'),
        (CACHE, 'linux-namespace-epoch1-owning-gates.json'),
        (SHARE, 'namespace-native-owning-epoch1.json'),
    ]
    while True:
        complete = True
        for base, name in items:
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
    for host, base, gate_name, owner_name, before, after in [
        ('macos', CACHE, 'namespace-macos-epoch1-gates.json', 'macos-namespace-epoch1-owning-gates.json',
         'macos-namespace-epoch1-c-pre-cpp-archive-identities.json', 'macos-namespace-epoch1-c-final-owning-archive-identities.json'),
        ('linux', CACHE, 'namespace-linux-epoch1b-gates.json', 'linux-namespace-epoch1-owning-gates.json',
         'linux-namespace-epoch1-pre-cpp-archive-identities.json', 'linux-namespace-epoch1-final-owning-archive-identities.json'),
        ('windows', SHARE, 'namespace-native-epoch1-gates.json', 'namespace-native-owning-epoch1.json',
         'native-namespace-epoch1-pre-cpp-archives.json', 'native-namespace-epoch1-final-owning-archive-identities.json'),
    ]:
        gates = collect_receipt(base, gate_name, 'gates-' + host + '.json')
        collect_receipt(base, owner_name, 'owning-' + host + '.json')
        copy(base / before, 'archives-' + host + '-before.json')
        copy(base / after, 'archives-' + host + '-after.json')
        full = next(row for row in gates['steps'] if row['name'] == 'full')
        match = re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored;', full['results'][0])
        assert match
        counts[host] = dict(zip(['passed', 'failed', 'ignored'], map(int, match.groups())))
    assert counts == {
        'macos': {'passed': 8083, 'failed': 0, 'ignored': 2},
        'linux': {'passed': 8077, 'failed': 0, 'ignored': 2},
        'windows': {'passed': 7544, 'failed': 5, 'ignored': 2},
    }
    for row in load(CACHE / 'namespace-owning-source-hashes-epoch1.json')['helpers']:
        copy(CACHE / row['path'] if row['path'] == 'namespace-owned-cpp-build-epoch1.py' else SHARE / row['path'])
    for name in [
        'macos-namespace-epoch1-linkage.json', 'namespace-macos-owning-link.json',
        'run-namespace-unix-epoch1.py', 'run-namespace-unix-epoch1b.py',
        'run-namespace-native-epoch1.py', 'run-namespace-owning-epoch1.py',
        'run-namespace-native-owning-epoch1.py', 'collect-namespace-archives-epoch1.py',
        'owning-linux-cpp-build-namespace-epoch1.py', 'collect-namespace-evidence.py',
    ]:
        copy(CACHE / name)
    failed = load(CACHE / 'namespace-linux-epoch1-gates.json')
    assert failed['state'] == 'failed' and failed['steps'][0]['exit-code'] == 125
    copy(CACHE / 'namespace-linux-epoch1-gates.json', 'rejected-linux-mount-gates.json')
    copy(CACHE / 'namespace-linux-epoch1-focus.log', 'rejected-linux-mount-focus.log')
    # Keep the rejected receipt's original log filename resolvable as well.
    copy(CACHE / 'namespace-linux-epoch1-focus.log')
    value = {
        'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'library-gates': counts, 'source-files': 25071, 'source-bytes': 1834294123,
        'owning-source-inputs': 3812, 'retained-assist-abi': 184,
        'namespace-cause-on-x64-runner': 'unknown', 'full-process-goal-complete': False,
        'whole-current-root-products-validated': False,
        'scope': 'Contextual rejection diagnostic with unchanged validation, bounded four-ABI canonical-name probe, complete declared3host gates and retained consumer boundary.',
    }
    (VALIDATION / 'summary.json').write_text(json.dumps(value, indent=2) + '\n')
    print(json.dumps(value, indent=2), flush=True)


if __name__ == '__main__':
    main()
