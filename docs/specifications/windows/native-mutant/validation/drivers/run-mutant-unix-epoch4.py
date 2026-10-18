"""Complete affected Unix gates for the fixed fourth source epoch."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time

B = Path(__file__).resolve().parent
R = B / 'mutant-source'
HOST = sys.argv[1]
RECORD = B / ('mutant-' + HOST + '-epoch4-gates.json')
if HOST not in ['macos', 'linux'] or RECORD.exists():
    raise RuntimeError('host invalid or gates already attempted')
manifest = json.loads((B / 'mutant-source-epoch4.json').read_text())


def verify():
    for row in manifest['files']:
        raw = (R / row['path']).read_bytes()
        if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('Unix source drift: ' + row['path'])


prefix = []
env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
           CARGO_TARGET_DIR=str(B / 'target-macos-mutant'))
cargo = ['cargo', '+1.95.0']
if HOST == 'linux':
    prefix = ['docker', 'run', '--rm', '--name', 'assist-native-mutant-epoch4', '--platform', 'linux/amd64',
        '-v', '/Users/int/hexrays/kvasir:/work:ro', '-v', str(R) + ':/work/vendor/rax:ro',
        '-v', 'rax-cargo-registry:/usr/local/cargo/registry', '-v', 'rax-cargo-git:/usr/local/cargo/git',
        '-v', 'rax-user-target:/target', '-e', 'CARGO_TARGET_DIR=/target/bno-isolation-prototype',
        '-e', 'CARGO_BUILD_JOBS=2', '-e', 'CARGO_INCREMENTAL=0',
        '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu', '-w', '/work/vendor/rax',
        'assist-rax-validation-rustfmt:1.95-bullseye-20261015']
    cargo = ['cargo']
gates = [('focus', ['test', '--lib', 'native_mutant']), ('full', ['test', '--lib']),
         ('capi', ['test', '-p', 'rax-capi']),
         ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
         ('all-targets', ['check', '--all-targets'])]
if HOST == 'macos':
    focus = (B / 'mutant-focus-epoch4.log').read_text()
    if '11 passed; 0 failed;' not in focus:
        raise RuntimeError('macOS focused run has not passed')
    gates = gates[1:]
rows = []
for name, command in gates:
    verify()
    args = prefix + cargo + [command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
    if command[0] == 'test':
        args += ['--', '--nocapture', '--test-threads=1']
    path = B / ('mutant-' + HOST + '-epoch4-' + name + '.log')
    started = time.monotonic()
    with path.open('wb') as output:
        process = subprocess.Popen(args, cwd=R, env=env, stdout=output, stderr=subprocess.STDOUT)
        state = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state': 'running',
                 'source-epoch': 4, 'host': HOST, 'driver-pid': os.getpid(),
                 'child-pid': process.pid, 'current-step': name, 'steps': rows}
        RECORD.write_text(json.dumps(state, indent=2) + '\n')
        code = process.wait()
    verify()
    raw = path.read_bytes()
    text = raw.decode('utf-8', 'replace')
    rows.append({'name': name, 'argv': args, 'exit-code': code, 'seconds': time.monotonic() - started,
                 'log': path.name, 'sha256': hashlib.sha256(raw).hexdigest(),
                 'results': re.findall(r'test result: ([^\r\n]+)', text)})
    state.update({'state': 'failed' if code else 'running', 'steps': rows, 'last-exit-code': code})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    print(HOST, name, code, flush=True)
    if code or name == 'focus' and '11 passed; 0 failed;' not in text:
        raise SystemExit(code or 1)
state.update({'state': 'finished', 'exit-code': 0,
              'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
RECORD.write_text(json.dumps(state, indent=2) + '\n')
