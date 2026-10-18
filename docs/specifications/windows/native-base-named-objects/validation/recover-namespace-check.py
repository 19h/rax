from pathlib import Path
import datetime
import json
import os
import shutil
import subprocess
import sys
import traceback

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = S / 'namespace-storage-recovery.json'
lock = B / 'namespace-storage-recovery.lock'
if record.exists():
    raise RuntimeError('storage recovery already attempted')
cleanup = json.loads((S / 'namespace-storage-cleanup.json').read_text())
if cleanup['free-after'] <= cleanup['free-before']:
    raise RuntimeError('no storage was recovered')
original = json.loads((S / 'base-final-launch.json').read_text())
if original['state'] != 'failed' or original['core-exit-code'] != 101:
    raise RuntimeError('unexpected original driver state')
fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(fd)
row = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state': 'running',
       'source-epoch': 2, 'original-driver': original, 'storage-cleanup': cleanup,
       'environment': {'CARGO_INCREMENTAL': '0'}, 'checks': []}


def save(path, value):
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    os.replace(temporary, path)


def run(args, name):
    with (S / name).open('wb') as log:
        result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
    row['checks'].append({'argv': args, 'log': name, 'exit-code': result.returncode})
    save(record, row)
    if result.returncode:
        raise RuntimeError('recovery command failed: ' + str(args))


save(record, row)
try:
    for name in ['base-final-launch.json', 'native-base-final-gates.json',
                 'native-base-final-all-targets.log']:
        shutil.copyfile(S / name, S / (name + '.storage-failure'))
    run([sys.executable, '-B', str(S / 'verify-base-final.py')],
        'namespace-recovery-source-before.log')
    run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
         str(S / 'recover-namespace-check.ps1')], 'namespace-recovery-driver.log')
    run([sys.executable, '-B', str(S / 'verify-base-final.py')],
        'namespace-recovery-source-after.log')
    gates = json.loads((S / 'native-base-final-gates.json').read_text(encoding='utf-8-sig'))
    if [r['gate'] for r in gates] != ['integration', 'capi', 'full', 'all-targets']:
        raise RuntimeError('unexpected recorded gate selection')
    gates[-1].update({'exit-code': 0, 'environment': {'CARGO_INCREMENTAL': '0'},
                      'recovery-record': record.name})
    save(S / 'native-base-final-gates.json', gates)
    row.update({'state': 'finished', 'exit-code': 0,
                'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
    save(record, row)
    save(S / 'base-final-launch.json', {
        'state': 'finished', 'exit-code': 0, 'source': 'src-base-named-objects',
        'source-inputs': 5728, 'source-epoch': 2, 'core-exit-code': 0,
        'original-driver-record': 'base-final-launch.json.storage-failure',
        'recovery-record': record.name, 'completed-utc': row['completed-utc'],
        'scope': 'Original integration/C API/full results plus the recovered all-target check.'})
except BaseException:
    row.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
    save(record, row)
finally:
    lock.unlink()
print(json.dumps(row), flush=True)
raise SystemExit(row['exit-code'])
