from pathlib import Path
import datetime
import json
import os
import subprocess
import time

BASE = Path(__file__).resolve().parent
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = BASE / 'windows-epoch2-host-driver.json'
if record.exists():
    raise RuntimeError('Native final driver already requested; inspect the existing record')
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'state': 'waiting-for-native-epoch1-terminal'}
record.write_text(json.dumps(state, indent=2) + '\n')
while True:
    try:
        initial = json.loads((SHARE / 'console-owner-epoch1-launch.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        initial = {}
    if 'exit-code' in initial:
        if initial['exit-code'] != 101:
            raise RuntimeError('Unexpected first native outcome; inspect before applying final overlay')
        break
    time.sleep(5)
argv = ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
        r'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe', '-B',
        r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\run-console-owner-epoch2-atomic.py']
state.update({'state': 'requested-native-final-epoch2', 'argv': argv,
              'started-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'epoch1-terminal': initial})
record.write_text(json.dumps(state, indent=2) + '\n')
with (BASE / 'windows-epoch2-host-driver.log').open('wb') as output:
    result = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT)
state.update({'state': 'host-request-finished', 'host-exit-code': result.returncode,
              'finished-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'scope': 'Parallels host return is not native completion; inspect native atomic record and gates'})
record.write_text(json.dumps(state, indent=2) + '\n')
raise SystemExit(result.returncode)
