"""Collect an explicit allowlist only after all three native drivers finish."""
from pathlib import Path
import datetime
import hashlib
import json

BASE = Path(__file__).resolve().parent
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
SOURCE = BASE / 'rax-final-source'
DEST = SOURCE / 'docs/specifications/windows/native-console-owner/validation'


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def require(value, message):
    if not value:
        raise ValueError(message)


def main():
    require(__debug__, 'optimized verification rejected')
    copies = {}

    def add(path, name=None):
        require(path.is_file(), 'missing completed evidence: ' + str(path))
        name = name or path.name
        require(name not in copies, 'duplicate evidence destination: ' + name)
        require(path.stat().st_size < 16 * 1024 * 1024, 'unexpectedly large evidence: ' + name)
        copies[name] = path

    for host in ['macos', 'linux']:
        for epoch in [1, 2]:
            folder = BASE / f'{host}-epoch{epoch}'
            gates = load(folder / 'gates.json')
            require(len(gates) == 7, 'unfinished ' + str(folder))
            require(all(row['exit-code'] == (101 if epoch == 1 and row['gate'] in ['focused', 'lib'] else 0)
                        for row in gates), 'unexpected gate outcome: ' + str(folder))
            add(folder / 'gates.json', f'{host}-epoch{epoch}/gates.json')
            for row in gates:
                add(folder / row['log'], f'{host}-epoch{epoch}/' + row['log'])
        gates = load(BASE / f'{host}-owning-gates.json')
        require(len(gates) == (6 if host == 'macos' else 4), 'unfinished owning gates: ' + host)
        require(all(row['exit-code'] == 0 for row in gates), 'failed owning gate: ' + host)
        add(BASE / f'{host}-owning-gates.json')
        for row in gates:
            add(BASE / row['log'])
        for suffix in ['owning-commands.json', 'final-owning-archive-identities.json']:
            add(BASE / f'{host}-{suffix}')

    for epoch in [1, 2]:
        record = load(SHARE / f'console-owner-epoch{epoch}-launch.json')
        require(record.get('exit-code') == (101 if epoch == 1 else 0), 'unfinished native driver epoch' + str(epoch))
        gates_name = f'native-console-owner-epoch{epoch}-gates.json'
        handoff = SHARE / 'console-owner-native-handoff.json'
        transferred = load(handoff) if epoch == 1 and handoff.exists() else None
        superseded = bool(transferred and transferred.get('epoch1-native-focused', '').startswith('not run;'))
        if transferred is not None:
            require(transferred.get('exit-code') == 0 and transferred.get('state') == 'finished',
                    'unfinished native handoff')
            require(transferred.get('final-core-exit-code') == 0 and transferred.get('final-owning-exit-code') == 0,
                    'handoff did not finish corrected core and owning checks')
            add(handoff, 'windows/' + handoff.name)
            add(SHARE / 'run-console-owner-native-handoff.py', 'windows/run-console-owner-native-handoff.py')
            add(SHARE / 'console-owner-native-continuation.json', 'windows/console-owner-native-continuation.json')
            add(SHARE / 'run-console-owner-continuation-after-initial.py', 'windows/run-console-owner-continuation-after-initial.py')
            add(SHARE / 'loader-first-error-via-existing-driver.json', 'windows/loader-first-error-via-existing-driver.json')
        gates = [] if superseded else load(SHARE / gates_name)
        if isinstance(gates, dict):
            gates = [gates]
        require(len(gates) == (0 if superseded else 1 if epoch == 1 else 6), 'native gate count epoch' + str(epoch))
        require(all(row['exit-code'] == (101 if epoch == 1 or row['gate'] == 'full' else 0)
                    for row in gates), 'unexpected native gate epoch' + str(epoch))
        names = [f'console-owner-epoch{epoch}-launch.json', f'console-owner-epoch{epoch}-driver.log',
                 f'native-console-owner-epoch{epoch}-source-verification.json',
                 f'native-console-owner-epoch{epoch}-build-user.log',
                 f'native-console-owner-epoch{epoch}-observer-build.log',
                 f'native-console-owner-epoch{epoch}-gates.ps1',
                 f'run-console-owner-epoch{epoch}-atomic.py',
                 f'apply-console-owner-epoch{epoch}.py',
                 f'run-console-owner-epoch{epoch}-observations.py',
                 f'console-owner-epoch{epoch}-overlay.json']
        if not superseded:
            names.append(gates_name)
        names += [row['log'] for row in gates]
        ordinary = f'native-console-owner-epoch{epoch}-ordinary-outcomes.json'
        names.append(ordinary)
        names += [row['log'] for row in load(SHARE / ordinary) if 'log' in row]
        for name in names:
            add(SHARE / name, 'windows/' + name)

    record = load(SHARE / 'console-owner-owning-epoch2-atomic-v2-launch.json')
    require(record.get('exit-code') == 0, 'unfinished native owning driver')
    for name in ['console-owner-owning-epoch2-atomic-v2-launch.json',
                 'console-owner-owning-epoch2-atomic-v2-driver.log',
                 'assist-native-console-owner-epoch2-owned.ps1',
                 'assist-native-console-owner-epoch2-shipping.log',
                 'assist-native-console-owner-epoch2-cpp.log',
                 'native-console-owner-epoch2-owning-source-verification.json',
                 'native-console-owner-epoch2-owning-archive-hashes.json',
                 'console-owner-epoch2-owning-source-hashes.json',
                 'inspect-console-owner-epoch2-archives.py',
                 'verify-console-owner-epoch2-owning-sources.py',
                 'run-console-owner-owning-epoch2-atomic.py']:
        add(SHARE / name, 'windows/' + name)

    for epoch in [1, 2]:
        manifest = load(BASE / f'epoch{epoch}-console-owner-code.json')
        require(len(manifest['files']) == 13, 'source identity count')
        add(BASE / f'epoch{epoch}-console-owner-code.json')
        add(BASE / f'epoch{epoch}-owning-source-hashes.json')
        root = BASE / ('epoch1-code' if epoch == 1 else 'rax-final-source')
        for row in manifest['files']:
            data = (root / row['path']).read_bytes()
            require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']),
                    'frozen epoch input mismatch: ' + row['path'])
            add(root / row['path'], f'compiled-source/epoch{epoch}/' + row['path'])

    for name in ['run-console-owner-unix.py', 'run-console-owner-final-unix.py',
                 'run-console-owner-final-after-initial.py',
                 'run-console-owner-final-windows-after-initial.py',
                 'run-console-owner-owning-after-focus.py',
                 'run-console-owner-windows-owning-after-focus.py',
                 'run-owning-gates.py', 'collect-console-owner-archives.py',
                 'collect-console-owner-validation.py', 'owning-linux-cpp-build.py',
                 'current-cpp-consumer-identities.json',
                 'latest-published-cpp-consumer-identities.json',
                 'root-function-environment-source-comparison.json',
                 'owning-validation-boundary.json', 'final-code-integrity-check.json',
                 'evidence-stress-results.json', 'stress-console-owner-evidence.py',
                 'summarize-final-unix-console-owner.py', 'final-unix-core-summary.json',
                 'epoch1-observation-driver-original.py',
                 'epoch1-observation-driver-fallback-change.json',
                 'epoch1-observation-driver-detached-fallback.py',
                 'epoch2-native-launcher-original.py', 'native-owning-launcher-original.py',
                 'native-final-handoff-change.json', 'actual-native-continuation-provenance.json']:
        add(BASE / name)

    rows = []
    for name, path in sorted(copies.items()):
        data = path.read_bytes()
        target = DEST / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            require(target.read_bytes() == data, 'refusing to replace earlier evidence: ' + name)
        else:
            target.write_bytes(data)
        rows.append({'path': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'base': 'b0c3f8d374bc85068479f6b59f9ab24b0fe78827',
              'final-code-epoch': 2, 'selected-owning-input-count': 309,
              'scope': 'source epochs, completed core gates and retained Assist carrier checks; '
                       'no whole-current-root, GUI, package or ordinary guest success inference',
              'files': rows}
    (DEST / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    print('Collected', len(rows), 'explicit evidence files;', sum(row['bytes'] for row in rows), 'bytes')


if __name__ == '__main__':
    main()
