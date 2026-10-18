from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys
import traceback

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = S / 'links-native-focus-epoch1.json'
lock = B / 'links-native-focus-epoch1.lock'
if record.exists():
    raise RuntimeError('native link focus already attempted')
fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY); os.close(fd)
row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'state': 'preparing', 'source-epoch': 1, 'checks': []}


def save():
    record.write_text(json.dumps(row, indent=2) + '\n')


def run(args, log):
    with (S / log).open('wb') as output:
        child = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT)
        row['child-pid'] = child.pid; save()
        code = child.wait()
    row['checks'].append({'argv': args, 'exit-code': code, 'log': log}); save()
    if code:
        raise RuntimeError('native focus step failed ' + repr(args))


save()
try:
    run([sys.executable, '-B', str(S / 'prepare-links-native-epoch1.py')], 'links-native-preparation-epoch1.log')
    row['state'] = 'testing'; save()
    run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
         str(S / 'links-native-focus-epoch1.ps1')], 'links-native-driver-epoch1.log')
    manifest = json.loads((S / 'links-source-epoch1.json').read_text())
    for item in manifest['files']:
        raw = (B / 'src-object-reparse-epoch1' / item['path']).read_bytes()
        if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
            raise RuntimeError('post-run source changed ' + item['path'])
    row.update({'state': 'finished', 'exit-code': 0, 'post-source-inputs': len(manifest['files'])})
except BaseException:
    row.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    row['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat(); save(); lock.unlink()
print(json.dumps(row), flush=True)
raise SystemExit(row['exit-code'])
