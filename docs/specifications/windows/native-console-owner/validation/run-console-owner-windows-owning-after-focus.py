from pathlib import Path
import datetime
import json
import os
import subprocess
import time

base = Path(__file__).resolve().parent
share = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = base / 'windows-epoch2-owning-host-driver.json'
if record.exists():
    raise RuntimeError('Native owning build already requested; inspect the existing record')
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'state': 'waiting-for-final-native-semantic-and-compile-gates'}
record.write_text(json.dumps(state, indent=2) + '\n')
required = {'focused', 'prior-services', 'all-targets', 'integration'}
while True:
    try:
        rows = json.loads((share / 'native-console-owner-epoch2-gates.json').read_text(encoding='utf-8-sig'))
        if isinstance(rows, dict):
            rows = [rows]
    except (FileNotFoundError, json.JSONDecodeError):
        rows = []
    if any(row['exit-code'] and row['gate'] != 'full' for row in rows):
        raise RuntimeError('A final native core gate failed; inspect before owning build')
    if required <= {row['gate'] for row in rows}:
        break
    time.sleep(5)
argv = ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
        r'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe', '-B',
        r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\run-console-owner-owning-epoch2-atomic.py']
state.update({'state': 'requested-native-owning-epoch2', 'argv': argv,
              'started-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(state, indent=2) + '\n')
with (base / 'windows-epoch2-owning-host-driver.log').open('wb') as output:
    result = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT)
state.update({'state': 'host-request-finished', 'host-exit-code': result.returncode,
              'finished-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'scope': 'Parallels return is not native completion; inspect the native atomic record'})
record.write_text(json.dumps(state, indent=2) + '\n')
raise SystemExit(result.returncode)
