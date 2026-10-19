from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import traceback


P = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = P / 'idle-source-epoch2/docs/specifications/windows/native-worker-factory-idle-timeout'
record = P / 'idle-finalization-b.json'
assert not record.exists()
state = {'state': 'waiting-for-gates', 'pid': os.getpid(), 'steps': [],
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}


def save():
    record.write_text(json.dumps(state, indent=2) + '\n')


def run(name, argv):
    path = P / ('idle-finalization-b-' + name + '.log')
    with path.open('xb') as log:
        child = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
        state.update({'state': name, 'child-pid': child.pid})
        save()
        code = child.wait()
    raw = path.read_bytes()
    state['steps'].append({'name': name, 'argv': argv, 'exit-code': code,
                           'log': path.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    save()
    assert code == 0, name


save()
try:
    gates = [(P, 'idle-macos-epoch2-gates.json'), (P, 'idle-linux-epoch2-gates.json'),
             (S, 'idle-native-epoch2-gates.json'),
             (P, 'macos-idle-epoch2-owning-gates.json'),
             (P, 'linux-idle-epoch2-owning-gates.json'), (S, 'idle-native-owning-epoch2.json')]
    deadline = time.monotonic() + 1800
    while True:
        rows = []
        for base, name in gates:
            try:
                row = json.loads((base / name).read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                row = None
            rows.append(row)
        assert all(row is None or row['state'] != 'failed' for row in rows)
        if all(row is not None and row['state'] == 'finished' and row['exit-code'] == 0 for row in rows):
            break
        assert time.monotonic() < deadline, 'full gates did not finish'
        time.sleep(3)
    run('records', [sys.executable, '-B', str(P / 'finalize-idle-records.py')])
    controls = json.loads((P / 'idle-final-negative-controls.json').read_text())
    assert controls['all-rejected'] and len(controls['controls']) == 17
    for name in ['idle-final-negative-controls.json', 'README.pending.md', 'runtime-append.pending.md']:
        shutil.copy2(P / name, D / 'validation' / name)
    run('proof-archive', [sys.executable, '-B', str(P / 'build-idle-proof.py')])
    shutil.copy2(P / 'run-idle-final-windows-check.py', S / 'run-idle-final-windows-check.py')
    commands = [
        ('macos', [sys.executable, '-B', str(P / 'run-idle-final-unix-check.py'), 'macos']),
        ('linux', [sys.executable, '-B', str(P / 'run-idle-final-unix-check.py'), 'linux']),
        ('windows', ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
                     r'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe', '-B',
                     r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\run-idle-final-windows-check.py']),
    ]
    children = []
    for name, argv in commands:
        log = (P / ('idle-finalization-b-proof-' + name + '.log')).open('xb')
        child = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
        children.append((name, child, log))
    state.update({'state': 'three-host-proof', 'proof-pids': {name: child.pid for name, child, _ in children}})
    save()
    for name, child, log in children:
        code = child.wait()
        log.close()
        # A Parallels controller disconnect is not a failed native verification.
        # Inspect its native receipt before assigning a result.
        if name == 'windows' and code != 0:
            native = S / 'idle-final-check-windows.json'
            while native.exists():
                receipt = json.loads(native.read_text())
                if receipt['state'] in ['finished', 'failed']:
                    code = receipt['exit-code']
                    break
                assert time.monotonic() < deadline, 'native receipt did not finish'
                time.sleep(3)
        state['steps'].append({'name': 'proof-' + name, 'exit-code': code})
        save()
        assert code == 0, 'proof-' + name
    state.update({'state': 'finished', 'exit-code': 0})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
