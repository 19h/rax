from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time


O = Path(__file__).resolve().parent
R = Path('/Users/int/.assist-alert3-20261019')
N = O.parent / 'native-wait-20261019'
HOST = sys.argv[1]
assert HOST in ('macos', 'linux')
RECORD = O / ('alert-' + HOST + '-epoch3-gates.json')
assert not RECORD.exists()
manifest = json.loads((O / 'alert-source-epoch3.json').read_text())
if HOST == 'linux':
    deadline = time.monotonic() + 7200
    RECORD.write_text(json.dumps({'state':'waiting-for-prior-host-gates','host':HOST,'steps':[]})+'\n')
    while True:
        prior_path = O/'alert-linux-epoch2-gates.json'
        prior_state = json.loads(prior_path.read_text())
        assert prior_state['state'] != 'failed', prior_state
        if prior_state['state']=='finished':
            assert prior_state['exit-code']==0
            break
        assert time.monotonic()<deadline
        time.sleep(3)



def verify():
    for row in manifest['files']:
        p = R / row['path']
        raw = os.readlink(p).encode() if row['kind'] == 'symlink-text' else p.read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']


prefix = []
env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
           CARGO_TARGET_DIR=str(R / 'target'), TMPDIR=str(R / 't'))
cargo = ['cargo', '+1.95.0']
if HOST == 'linux':
    prior = json.loads((N / 'wait-linux-epoch4-gates.json').read_text())
    assert prior['state'] == 'finished' and prior['exit-code'] == 0
    prefix = ['docker', 'run', '--rm', '--name', 'assist-native-alert-epoch3', '--platform', 'linux/amd64',
              '-v', '/Users/int/hexrays/kvasir:/work:ro', '-v', str(R) + ':/work/vendor/rax:ro',
              '-v', 'rax-cargo-registry:/usr/local/cargo/registry', '-v', 'rax-cargo-git:/usr/local/cargo/git',
              '-v', 'rax-user-target:/work/vendor/rax/target/linux', '-e', 'CARGO_TARGET_DIR=/work/vendor/rax/target/linux/bno-isolation-prototype',
              '-e', 'CARGO_BUILD_JOBS=2', '-e', 'CARGO_INCREMENTAL=0',
              '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu', '-w', '/work/vendor/rax',
              'assist-rax-validation-rustfmt:1.95-bullseye-20261015', 'sh', '-c', 'mkdir -p /work/vendor/rax/target/linux/tmp && export TMPDIR=/work/vendor/rax/target/linux/tmp && exec "$@"', 'sh']
    cargo = ['cargo']
state = {'state': 'preparing', 'source-epoch': 2, 'host': HOST, 'driver-pid': os.getpid(), 'steps': []}
gates = [('focus', ['test', '--lib', 'native_test_alert']), ('full', ['test', '--lib']),
         ('capi', ['test', '-p', 'rax-capi']),
         ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
         ('all-targets', ['check', '--all-targets'])]
if HOST == 'macos':
    # The exact twelve shared tests already passed in the prototype on unchanged inputs.
    # The complete library target below includes them again as part of its broad gate.
    gates = [(name, command) for name, command in gates if name != 'focus']
for name, command in gates:
    verify()
    args = prefix + cargo + [command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
    if command[0] == 'test':
        args += ['--', '--nocapture', '--test-threads=1']
    path = O / ('alert-' + HOST + '-epoch3-' + name + '.log')
    start = time.monotonic()
    with path.open('wb') as log:
        child = subprocess.Popen(args, cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT)
        state.update({'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state': 'running',
                      'child-pid': child.pid, 'current-step': name})
        RECORD.write_text(json.dumps(state, indent=2) + '\n')
        code = child.wait()
    verify()
    raw = path.read_bytes()
    results = re.findall(r'test result: ([^\r\n]+)', raw.decode('utf-8', 'replace'))
    state['steps'].append({'name': name, 'argv': args, 'exit-code': code,
                           'seconds': time.monotonic() - start, 'log': path.name,
                           'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(), 'results': results})
    state.update({'state': 'failed' if code else 'running', 'last-exit-code': code})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    print(HOST, name, code, flush=True)
    assert code == 0, (HOST, name, code)
    if name == 'focus':
        assert any('8 passed; 0 failed;' in result for result in results), results
    if name == 'full':
        assert any('0 failed; 2 ignored; 0 measured; 0 filtered out;' in result for result in results), results
state.update({'state': 'finished', 'exit-code': 0, 'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
RECORD.write_text(json.dumps(state, indent=2) + '\n')
