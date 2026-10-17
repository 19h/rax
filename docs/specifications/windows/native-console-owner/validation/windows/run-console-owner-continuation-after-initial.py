from pathlib import Path
import datetime
import hashlib
import importlib.util
import json
import os
import sys
import time

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = SHARE / 'console-owner-native-continuation.json'
lock = BASE / 'console-owner-native-continuation.lock'
if record.exists():
    raise RuntimeError('Native continuation already requested; inspect before retry')
descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.write(descriptor, str(os.getpid()).encode())
os.close(descriptor)
spec = importlib.util.spec_from_file_location('native_handoff', SHARE / 'run-console-owner-native-handoff.py')
handoff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(handoff)
row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'state': 'waiting-for-initial-native-terminal',
       'scope': 'preserve current compile/tests; await actual initial101 and inspect old focused results; then run unchanged final native gates directly',
       'handoff-source-sha256': hashlib.sha256((SHARE / 'run-console-owner-native-handoff.py').read_bytes()).hexdigest()}
record.write_text(json.dumps(row, indent=2) + '\n')
try:
    while True:
        initial = handoff.load(SHARE / 'console-owner-epoch1-launch.json')
        if 'exit-code' in initial:
            if initial['exit-code'] != 101:
                raise RuntimeError('Unexpected initial terminal code; inspect actual native result')
            break
        if not handoff.alive(initial['pid']):
            raise RuntimeError('Initial native process is no longer live and has no terminal record')
        row['last-verified-live-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        row['verified-initial-pid'] = initial['pid']
        record.write_text(json.dumps(row, indent=2) + '\n')
        time.sleep(5)
    row['state'] = 'running-final-native-handoff'
    record.write_text(json.dumps(row, indent=2) + '\n')
    row['exit-code'] = handoff.main()
    row['state'] = 'finished'
except BaseException as error:
    row.update({'state': 'failed', 'exit-code': 1, 'error': repr(error)})
    raise
finally:
    row['finished-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    record.write_text(json.dumps(row, indent=2) + '\n')
    lock.unlink()
sys.exit(row['exit-code'])
