from pathlib import Path
import json
import subprocess
import sys
import time

N = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
while True:
    path = S / 'namespace-storage-recovery.json'
    if path.exists():
        row = json.loads(path.read_text())
        if row['state'] in ['finished', 'failed']:
            if row['state'] != 'finished' or row['exit-code']:
                raise RuntimeError('native storage recovery failed; inspect its record')
            break
    time.sleep(5)
old = N / 'native-base-completion-queue.json'
archived = N / 'epoch2-storage-failure/native-base-completion-queue.json'
if old.exists():
    if archived.exists():
        raise RuntimeError('completion recovery already attempted')
    old.rename(archived)
with (N / 'native-base-completion-after-storage.log').open('wb') as log:
    result = subprocess.run([sys.executable, '-B', str(N / 'queue-native-base-completion.py')],
                            stdout=log, stderr=subprocess.STDOUT)
print('Post-storage native completion:', result.returncode, flush=True)
raise SystemExit(result.returncode)
