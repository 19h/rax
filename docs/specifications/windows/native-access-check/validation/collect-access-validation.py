"""Collect completed source-bound access-check validation without replacing proof."""
from pathlib import Path
import datetime
import hashlib
import json

B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = B / 'rax-source/docs/specifications/windows/native-access-check/validation'

def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

def need(value, message):
    if not value:
        raise ValueError(message)

def main():
    need(__debug__, 'optimized collection rejected')
    native = load(S / 'access-check-final-launch.json')
    need(native.get('state') == 'finished' and native.get('exit-code') == 0, 'native driver incomplete')
    files = {}
    def add(path, name=None):
        name = name or path.name
        need(path.is_file() and name not in files, 'missing or duplicate ' + str(path))
        files[name] = path
    for host in ['macos', 'linux']:
        for label, count, status in [('initial', 1, 101), ('epoch2', 3, 0), ('final', 4, 0)]:
            path = B / f'{host}-access-{label}-gates.json'
            gates = load(path)
            need(len(gates) == count and all(g['exit-code'] == status for g in gates), 'incomplete ' + path.name)
            add(path)
            for gate in gates:
                add(B / gate['log'])
        gates = load(B / (host + '-owning-gates.json'))
        need(len(gates) == (6 if host == 'macos' else 4) and all(g['exit-code'] == 0 for g in gates), 'incomplete owning ' + host)
        for name in [host + '-owning-gates.json', host + '-owning-commands.json',
                     host + '-final-owning-archive-identities.json', *[g['log'] for g in gates]]:
            add(B / name)
    for name in ['source-baseline.json', 'owned-source-plan.json', 'initial-access-source.json',
                 'initial-overlay-identities.json', 'initial-owning-source-hashes.json', 'final-source-inputs.json',
                 'epoch2-change.json', 'epoch2-owning-source-hashes.json',
                 'initial-fixture-debugger.log', 'initial-fixture-debugger-command.json',
                 'initial-review-notes.json', 'run-access-initial-unix.py', 'run-access-epoch2-unix.py',
                 'run-access-final-unix.py', 'run-owning-gates.py', 'owning-linux-cpp-build.py',
                 'collect-access-archives.py', 'collect-access-validation.py']:
        add(B / name)
    for row in load(B / 'initial-overlay-identities.json')['files']:
        add(B / 'source-epochs/initial' / row['path'], 'compiled-source/initial/' + row['path'])
        add(B / 'rax-source' / row['path'], 'compiled-source/final/' + row['path'])
    names = ['access-check-initial-launch.json', 'access-check-initial-driver.log',
             'apply-access-check-initial.py', 'run-access-check-initial.py',
             'native-access-check-initial-gates.ps1', 'native-access-check-initial-gates.json',
             'native-access-check-initial-source-verification.json',
             'access-check-final-launch.json', 'access-check-final-core-driver.log',
             'access-check-final-owning-driver.log', 'access-check-epoch2-delta.json',
             'access-check-final-owning-source-hashes.json', 'apply-access-check-epoch2.py',
             'run-access-check-final-atomic.py', 'native-access-check-final-gates.ps1',
             'native-access-check-final-gates.json', 'native-access-check-final-verified-gates.json',
             'native-access-check-epoch2-source-verification.json',
             'native-access-check-final-build-user.log', 'native-access-check-final-build-user-artifacts.jsonl',
             'native-access-check-final-observer-build.log', 'run-access-check-final-observations.py',
             'native-access-check-final-ordinary-outcomes.json',
             'assist-native-access-check-final-owned.ps1', 'verify-access-check-final-owning-sources.py',
             'inspect-access-check-final-archives.py', 'assist-native-access-check-final-shipping.log',
             'assist-native-access-check-final-cpp.log', 'native-access-check-final-after-artifacts.jsonl',
             'native-access-check-final-after-artifacts-stderr.log',
             'native-access-check-final-owning-source-identities.json',
             'native-access-check-final-owning-source-verification.json',
             'native-access-check-final-owning-archive-hashes.json', 'current-cpp-consumers-reverified.json',
             'native-secure-capture-current-cpp-verification.json']
    for prefix in ['initial', 'final']:
        names.extend(g['log'] for g in load(S / f'native-access-check-{prefix}-gates.json'))
    names.extend(g['log'] for g in load(S / 'native-access-check-final-ordinary-outcomes.json') if 'log' in g)
    for name in names:
        add(S / name, 'windows/' + name)
    for name in ['owning-validation-boundary.json', 'latest-published-cpp-consumer-identities.json']:
        add(B.parent / 'native-create-file-20261015' / name)
    rows = []
    for name, source in sorted(files.items()):
        target = D / name
        target.parent.mkdir(parents=True, exist_ok=True)
        raw = source.read_bytes()
        if target.exists():
            need(target.read_bytes() == raw, 'refuse evidence replacement ' + name)
        else:
            target.write_bytes(raw)
        rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    (D / 'results.json').write_text(json.dumps({'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'parent': '23ac34c5d0d0f6afff762431442d2fab52cda7f9', 'selected-owning-inputs': 319,
        'files': rows}, indent=2) + '\n')
    print('Collected', len(rows), 'records;', sum(r['bytes'] for r in rows), 'bytes')

if __name__ == '__main__':
    main()
