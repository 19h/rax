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
record = S / 'bno-native-epoch5-launch.json'
lock = B / 'bno-native-epoch5.lock'
if record.exists():
    raise RuntimeError('epoch5 native attempt already recorded')
fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(fd)
row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'state': 'preparing', 'source-epoch': 5, 'namespace-epoch': 2}


def save():
    record.write_text(json.dumps(row, indent=2) + '\n')


def run(args, log):
    with (S / log).open('wb') as out:
        child = subprocess.Popen(args, stdout=out, stderr=subprocess.STDOUT)
        row['child-pid'] = child.pid
        save()
        return child.wait()


save()
try:
    code = run([sys.executable, '-B', str(S / 'prepare-bno-native-epoch5.py')],
               'bno-native-epoch5-prepare.log')
    if code:
        raise RuntimeError('native source preparation failed: ' + str(code))
    row['state'] = 'running-validation'
    save()
    code = run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                str(S / 'bno-native-epoch5-focus.ps1')], 'bno-native-epoch5-driver.log')
    manifest = json.loads((S / 'bno-epoch5-source.json').read_text())
    for item in manifest['files']:
        raw = (B / 'src-bno-isolation-prototype' / item['path']).read_bytes()
        if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
            raise RuntimeError('source changed during native focus: ' + item['path'])
    row.update({'state': 'finished', 'exit-code': code, 'post-source-inputs': len(manifest['files'])})
except BaseException:
    row.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    row['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    lock.unlink()
print(json.dumps(row), flush=True)
raise SystemExit(row['exit-code'])
