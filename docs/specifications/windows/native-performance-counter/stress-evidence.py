from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

BASE = Path(__file__).parent
CHECKER = BASE / 'verify-evidence.py'
output = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(tempfile.mkdtemp(prefix='performance-counter-evidence-'))
output.mkdir(parents=True, exist_ok=True)
subprocess.run([sys.executable, '-B', str(CHECKER)], check=True, capture_output=True)
results = []
for case in ['source-byte', 'executed-binary', 'kernel', 'subject', 'library', 'frequency', 'missing-row-rehashed', 'status-rehashed', 'monotonic-rehashed', 'optimized']:
    copied = output / case
    shutil.copytree(BASE, copied)
    identities = json.loads((copied / 'native-performance-counter-identities.json').read_text())
    if case == 'source-byte':
        path = copied / 'native-performance-counter.cpp'
        path.write_bytes(path.read_bytes() + b' ')
    elif case == 'executed-binary':
        identities['producers'][0]['executable']['sha256'] = '0' * 64
    elif case == 'kernel':
        identities['native-machine'] = 0x8664
    elif case == 'subject':
        identities['principal'] = 'S-1-5-21'
    elif case == 'library':
        identities['libraries'][0]['sha256'] = '0' * 64
    elif case == 'frequency':
        identities['producers'][0]['calibration']['frequency'] = '10000000'
    elif case in ['missing-row-rehashed', 'status-rehashed', 'monotonic-rehashed']:
        path = copied / 'native-performance-counter-arm64.log'
        lines = path.read_text().splitlines(keepends=True)
        if case == 'missing-row-rehashed':
            lines = [line for line in lines if not line.startswith('case=249 ')]
        elif case == 'status-rehashed':
            lines = [line.replace('status=00000000', 'status=80000002', 1) if line.startswith('case=0 ') else line for line in lines]
        else:
            lines = [line.replace('delta=', 'delta=999', 1) if line.startswith('monotonic case=0 ') else line for line in lines]
        path.write_text(''.join(lines))
        data = path.read_bytes()
        identities['producers'][0]['stdout'].update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    (copied / 'native-performance-counter-identities.json').write_text(json.dumps(identities, indent=2) + '\n')
    command = [sys.executable, '-O' if case == 'optimized' else '-B', str(CHECKER), str(copied)]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode == 0:
        raise ValueError('corrupted evidence accepted: ' + case)
    results.append({'case': case, 'rejected': True, 'exit-code': result.returncode, 'last-error': result.stderr.splitlines()[-1]})
(output / 'stress-results.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps({'rejected': len(results), 'output': str(output)}))
