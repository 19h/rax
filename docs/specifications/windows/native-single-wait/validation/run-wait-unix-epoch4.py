from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time

N = Path(__file__).resolve().parent
R = N / 'wait-source'
HOST = sys.argv[1]
RECORD = N / ('wait-' + HOST + '-epoch4-gates.json')
assert HOST in ['macos', 'linux'] and not RECORD.exists()
manifest = json.loads((N / 'wait-source-epoch4.json').read_text())

def verify():
    for row in manifest['files']:
        p = R / row['path']
        raw = os.readlink(p).encode() if row['kind'] == 'symlink-text' else p.read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']

prefix = []
env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
    CARGO_TARGET_DIR=str(N / 'target-macos-wait-epoch3'))
cargo = ['cargo', '+1.95.0']
if HOST == 'linux':
    prefix = ['docker', 'run', '--rm', '--name', 'assist-native-wait-epoch4', '--platform', 'linux/amd64',
        '-v', '/Users/int/hexrays/kvasir:/work:ro', '-v', str(R) + ':/work/vendor/rax:ro',
        '-v', 'rax-cargo-registry:/usr/local/cargo/registry', '-v', 'rax-cargo-git:/usr/local/cargo/git',
        '-v', 'rax-user-target:/target', '-e', 'CARGO_TARGET_DIR=/target/bno-isolation-prototype',
        '-e', 'CARGO_BUILD_JOBS=2', '-e', 'CARGO_INCREMENTAL=0',
        '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu', '-w', '/work/vendor/rax',
        'assist-rax-validation-rustfmt:1.95-bullseye-20261015']
    cargo = ['cargo']
rows = []
gates = [('focus', ['test', '--lib', 'native_single_wait']), ('full', ['test', '--lib']),
    ('capi', ['test', '-p', 'rax-capi']), ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
    ('all-targets', ['check', '--all-targets'])]
state = {'state': 'preparing', 'source-epoch': 4, 'host': HOST, 'driver-pid': os.getpid(), 'steps': rows}
for name, command in gates:
    verify()
    args = prefix + cargo + [command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
    if command[0] == 'test':
        args += ['--', '--nocapture', '--test-threads=1']
    path = N / ('wait-' + HOST + '-epoch4-' + name + '.log')
    started = time.monotonic()
    with path.open('wb') as log:
        process = subprocess.Popen(args, cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT)
        state.update({'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state': 'running',
            'child-pid': process.pid, 'current-step': name})
        RECORD.write_text(json.dumps(state, indent=2) + '\n')
        code = process.wait()
    verify()
    raw = path.read_bytes()
    output = raw.decode('utf-8', 'replace')
    rows.append({'name': name, 'argv': args, 'exit-code': code, 'seconds': time.monotonic() - started,
        'log': path.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
        'results': re.findall(r'test result: ([^\r\n]+)', output)})
    state.update({'state': 'failed' if code else 'running', 'last-exit-code': code})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    print(HOST, name, code, flush=True)
    if code or name == 'focus' and '16 passed; 0 failed;' not in output:
        raise SystemExit(code or 1)
state.update({'state': 'finished', 'exit-code': 0, 'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
RECORD.write_text(json.dumps(state, indent=2) + '\n')
