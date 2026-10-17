from pathlib import Path
import datetime
import json
import os
import subprocess
import sys
import time

base = Path(__file__).resolve().parent
host = sys.argv[1]
record = base / (host + '-epoch2-driver.json')
if record.exists():
    raise RuntimeError('Final validation already requested; inspect its recorded process/outcome')
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'host': host, 'state': 'waiting-for-retained-epoch1-gates'}
record.write_text(json.dumps(state, indent=2) + '\n')
initial = base / (host + '-epoch1') / 'gates.json'
while True:
    if initial.exists():
        rows = json.loads(initial.read_text())
        if len(rows) == 7 and rows[-1]['gate'] == 'build-user':
            break
    time.sleep(5)
state['state'] = 'running-final-epoch2'
state['started-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
record.write_text(json.dumps(state, indent=2) + '\n')
with (base / (host + '-epoch2-driver.log')).open('wb') as output:
    child = subprocess.Popen([sys.executable, str(base / 'run-console-owner-final-unix.py'), host], stdout=output, stderr=subprocess.STDOUT)
    state['child-pid'] = child.pid
    record.write_text(json.dumps(state, indent=2) + '\n')
    state['exit-code'] = child.wait()
state['state'] = 'finished'
state['finished-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
record.write_text(json.dumps(state, indent=2) + '\n')
raise SystemExit(state['exit-code'])
