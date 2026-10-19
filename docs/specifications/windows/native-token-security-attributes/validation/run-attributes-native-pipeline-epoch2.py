"""Sequence native gates after candidate exploration; keep build ownership separate."""
from pathlib import Path
import datetime
import json
import os
import subprocess
import sys
import time
import traceback


S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = S / 'attributes-native-pipeline-epoch2.json'
assert not record.exists()
state = {'state': 'waiting-for-candidate', 'pid': os.getpid(),
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'children': []}


def save():
    record.write_text(json.dumps(state, indent=2) + '\n')


def read(name):
    path = S / name
    return json.loads(path.read_text()) if path.exists() else None


children = []
save()
try:
    deadline = time.monotonic() + 3600
    while True:
        candidate = read('attributes-candidate-epoch2-run.json')
        assert candidate is None or candidate['state'] != 'failed', candidate
        if candidate and candidate['state'] == 'finished':
            assert candidate['exit-code'] == 0
            break
        assert time.monotonic() < deadline, 'candidate wait expired'
        time.sleep(3)
    log = (S / 'attributes-native-pipeline-epoch2-full.log').open('wb')
    full = subprocess.Popen([sys.executable, '-B', str(S / 'run-attributes-native-epoch2.py')], stdout=log, stderr=subprocess.STDOUT)
    children.append(('full', full, log))
    state.update({'state': 'running-full', 'children': [{'name': 'full', 'pid': full.pid}]})
    save()
    while True:
        native = read('attributes-native-epoch2-gates.json')
        assert native is None or native['state'] != 'failed', native
        assert full.poll() is None or full.returncode == 0
        if native and any(row['name'] == 'focus' and row['exit-code'] == 0 for row in native['steps']):
            break
        assert time.monotonic() < deadline, 'focus wait expired'
        time.sleep(3)
    for name, child, log in children:
        code = child.wait()
        log.close()
        next(row for row in state['children'] if row['name'] == name)['exit-code'] = code
        save()
        assert code == 0, name
    state.update({'state': 'finished', 'exit-code': 0})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
