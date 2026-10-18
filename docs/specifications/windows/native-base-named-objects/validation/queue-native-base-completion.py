from pathlib import Path
import datetime
import json
import subprocess
import time

B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = B / 'native-base-completion-queue.json'
if record.exists():
    raise RuntimeError('native completion already queued')
row = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state': 'waiting'}
record.write_text(json.dumps(row, indent=2) + '\n')
deadline = time.monotonic() + 3600
while time.monotonic() < deadline:
    states = [json.loads((S / name).read_text()) for name in
              ['base-final-launch.json', 'base-final-owned-launch.json']]
    if any(r['state'] == 'failed' for r in states):
        raise RuntimeError('native validation failed before completion')
    if all(r['state'] == 'finished' and r['exit-code'] == 0 for r in states):
        break
    time.sleep(5)
else:
    raise RuntimeError('native completion wait exceeded 3600 s')
args = ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
        'C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe', '-B',
        'C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\finish-native-base-epoch2.py']
row.update(state='running', argv=args)
record.write_text(json.dumps(row, indent=2) + '\n')
with (B / 'native-base-completion-transport.log').open('wb') as log:
    result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
row.update({'state': 'finished', 'exit-code': result.returncode,
            'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(row, indent=2) + '\n')
print('Native namespace completion:', result.returncode, flush=True)
raise SystemExit(result.returncode)
