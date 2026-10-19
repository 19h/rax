from pathlib import Path
import datetime
import hashlib
import json
import subprocess
import sys

Q = Path(__file__).parent
R = Q / 'flags-source'
host = sys.argv[1]
assert host in ['macos', 'linux']
record = Q / ('flags-final-check-' + host + '.json')
assert not record.exists(), 'Inspect an earlier check; do not silently repeat it.'
meta = json.loads((Q / 'flags-final-proof-archive.json').read_text())
if host == 'macos':
    argv = [sys.executable, '-B', str(Q / 'run-flags-proof-checks.py'), str(R)]
else:
    argv = ['docker', 'run', '--rm', '--platform', 'linux/amd64',
            '-v', str(R) + ':/source:ro', '-v', str(Q) + ':/validation:ro',
            '-w', '/source', 'assist-rax-validation-rustfmt:1.95-bullseye-20261015',
            'python3', '-B', '/validation/run-flags-proof-checks.py', '/source']
state = {'state': 'checking', 'host': host, 'argv': argv,
         'checked-envelope-files': meta['bound-evidence-files'],
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
record.write_text(json.dumps(state, indent=2) + '\n')
log = Q / ('flags-final-check-' + host + '.log')
with log.open('wb') as output:
    code = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT).returncode
raw = log.read_bytes()
state.update({'state': 'finished' if code == 0 else 'failed', 'exit-code': code,
              'log': log.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
              'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(state, indent=2) + '\n')
print(json.dumps(state), flush=True)
raise SystemExit(code)
