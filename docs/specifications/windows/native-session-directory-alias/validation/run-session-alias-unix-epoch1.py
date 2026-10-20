"""Complete affected Cargo binaries for one immutable source epoch."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-session-alias-20261020')
HOST = sys.argv[1]
assert HOST in ['macos', 'linux']
RECORD = CACHE / ('session-alias-' + HOST + '-epoch1-gates.json')
assert not RECORD.exists()
manifest = json.loads((CACHE / 'session-alias-source-epoch1.json').read_text())
while True:
    focus = json.loads((CACHE / 'session-alias-focus-epoch1.json').read_text())
    if focus['state'] == 'finished':
        assert focus['exit-code'] == 0 and focus['inputs-unchanged']
        break
    time.sleep(3)


def verify():
    for row in manifest['files']:
        path = ROOT / row['path']
        raw = os.readlink(path).encode() if row['kind'] == 'symlink-text' else path.read_bytes()
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256']), row['path']


prefix = []
environment = os.environ.copy()
environment.update(CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
                   CARGO_TARGET_DIR='/Users/int/.assist-continue-20261019/target',
                   TMPDIR='/Users/int/.assist-continue-20261019/t')
cargo = ['cargo', '+1.95.0']
if HOST == 'linux':
    # This ignored mount point must exist below the read-only bind before Docker starts.
    (ROOT / 'target/linux').mkdir(parents=True, exist_ok=True)
    prefix = ['docker', 'run', '--rm', '--name', 'assist-native-session-alias-epoch1', '--platform', 'linux/amd64',
              '-v', '/Users/int/hexrays/kvasir:/work:ro', '-v', str(ROOT) + ':/work/vendor/rax:ro',
              '-v', 'rax-cargo-registry:/usr/local/cargo/registry', '-v', 'rax-cargo-git:/usr/local/cargo/git',
              '-v', 'rax-user-target:/work/vendor/rax/target/linux',
              '-e', 'CARGO_TARGET_DIR=/work/vendor/rax/target/linux/bno-isolation-prototype',
              '-e', 'CARGO_BUILD_JOBS=2', '-e', 'CARGO_INCREMENTAL=0',
              '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu', '-w', '/work/vendor/rax',
              'assist-rax-validation-rustfmt:1.95-bullseye-20261015', 'sh', '-c',
              'mkdir -p /work/vendor/rax/target/linux/tmp && export TMPDIR=/work/vendor/rax/target/linux/tmp && exec "$@"', 'sh']
    cargo = ['cargo']
state = {'state': 'preparing', 'source-epoch': 1, 'host': HOST, 'driver-pid': os.getpid(), 'steps': []}
for name, command in [
    ('focus', ['test', '--lib', 'session_directory_alias']),
    ('full', ['test', '--lib']),
    ('capi', ['test', '-p', 'rax-capi']),
    ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
    ('all-targets', ['check', '--all-targets']),
]:
    if HOST == 'macos' and name == 'focus':
        continue  # exact current frozen focus already finished above
    verify()
    argv = prefix + cargo + [command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
    if command[0] == 'test':
        argv += ['--', '--nocapture', '--test-threads=1']
    log = CACHE / ('session-alias-' + HOST + '-epoch1-' + name + '.log')
    start = time.monotonic()
    with log.open('wb') as output:
        child = subprocess.Popen(argv, cwd=ROOT, env=environment, stdout=output, stderr=subprocess.STDOUT)
        state.update({'state': 'running', 'current-step': name, 'child-pid': child.pid})
        RECORD.write_text(json.dumps(state, indent=2) + '\n')
        code = child.wait()
    verify()
    raw = log.read_bytes()
    results = re.findall(r'test result: ([^\r\n]+)', raw.decode(errors='replace'))
    state['steps'].append({'name': name, 'argv': argv, 'exit-code': code, 'seconds': time.monotonic() - start,
                           'log': log.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(), 'results': results})
    state.update({'state': 'failed' if code else 'running', 'last-exit-code': code})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    print(HOST, name, code, results, flush=True)
    assert code == 0
    expected = {'focus': 3, 'capi': 183, 'integration': 544,
                'full': 8102 if HOST == 'macos' else 8096}
    if name in expected:
        assert any(f'{expected[name]} passed; 0 failed;' in row for row in results), (name, results)
    if name == 'full':
        assert any('2 ignored; 0 measured; 0 filtered out;' in row for row in results)
state.update({'state': 'finished', 'exit-code': 0,
              'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
RECORD.write_text(json.dumps(state, indent=2) + '\n')
