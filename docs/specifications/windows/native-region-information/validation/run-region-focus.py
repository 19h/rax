from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import sys
import time

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-region-20261020')
epoch = int(sys.argv[1])
record = CACHE / ('region-focus-epoch' + str(epoch) + '.json')
assert not record.exists()


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).splitlines()


paths = sorted(set(git('diff', '--name-only') + git('ls-files', '--others', '--exclude-standard')))


def identities():
    rows = []
    for name in paths:
        raw = (ROOT / name).read_bytes()
        rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    return rows


before = identities()
argv = ['cargo', '+1.95.0', 'test', '--locked', '--offline', '--no-default-features',
        '--lib', sys.argv[2] if len(sys.argv) > 2 else 'region_', '--', '--nocapture', '--test-threads=1']
state = {'state': 'running', 'pid': os.getpid(), 'argv': argv, 'inputs-before': before}
record.write_text(json.dumps(state, indent=2) + '\n')
environment = os.environ.copy()
environment.update(CARGO_TARGET_DIR='/Users/int/.assist-continue-20261019/target',
                   CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0')
log = CACHE / ('region-focus-epoch' + str(epoch) + '.log')
start = time.monotonic()
with log.open('wb') as output:
    code = subprocess.run(argv, cwd=ROOT, env=environment, stdout=output, stderr=subprocess.STDOUT).returncode
raw = log.read_bytes()
after = identities()
state.update({'state': 'finished', 'exit-code': code, 'seconds': time.monotonic() - start,
              'inputs-after': after, 'inputs-unchanged': before == after,
              'log': log.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
              'results': re.findall(r'test result: ([^\r\n]+)', raw.decode(errors='replace'))})
record.write_text(json.dumps(state, indent=2) + '\n')
print(code, state['results'], flush=True)
assert before == after
raise SystemExit(code)
