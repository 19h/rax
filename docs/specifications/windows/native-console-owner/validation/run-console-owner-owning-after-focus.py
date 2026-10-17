from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys
import time

base = Path(__file__).resolve().parent
host = sys.argv[1]
record = base / (host + '-epoch2-owning-driver.json')
if record.exists():
    raise RuntimeError('Owning build already requested; inspect its existing process/outcome')
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'state': 'waiting-for-final-semantic-and-compile-gates', 'host': host}
record.write_text(json.dumps(state, indent=2) + '\n')
required = {'focused', 'prior-services', 'all-targets', 'windows'}
while True:
    try:
        rows = json.loads((base / (host + '-epoch2/gates.json')).read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        rows = []
    if any(row['exit-code'] for row in rows):
        raise RuntimeError('A final core gate failed; inspect before owning build')
    if required <= {row['gate'] for row in rows}:
        break
    time.sleep(5)
manifest = json.loads((base / 'epoch2-owning-source-hashes.json').read_text())
for row in manifest['sources']:
    data = (base / 'rax-final-source' / row['path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('Final owning source changed ' + row['path'])
state.update({'state': 'running-final-owning-gates', 'verified-inputs': len(manifest['sources']),
              'started-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(state, indent=2) + '\n')
with (base / (host + '-epoch2-owning-driver.log')).open('wb') as output:
    child = subprocess.Popen([sys.executable, str(base / 'run-owning-gates.py'), host], stdout=output, stderr=subprocess.STDOUT)
    state['child-pid'] = child.pid
    record.write_text(json.dumps(state, indent=2) + '\n')
    state['exit-code'] = child.wait()
state.update({'state': 'finished', 'finished-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(state, indent=2) + '\n')
raise SystemExit(state['exit-code'])
