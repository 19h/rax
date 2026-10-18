from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys
import time

B = Path(__file__).resolve().parent
R = B / 'prototype-source'
host = sys.argv[1]
record = B / (host + '-bno-epoch5-gates.json')
if record.exists():
    raise RuntimeError('full BNO validation already recorded')
focus = B / ('prototype-' + host + '-focus-epoch5.json')
while True:
    state = json.loads(focus.read_text())
    if state['state'] == 'finished':
        if state['exit-code']:
            raise RuntimeError('focused BNO validation failed')
        break
    time.sleep(5)
manifest = json.loads((B / 'bno-epoch5-source.json').read_text())


def verify():
    for row in manifest['files']:
        raw = (R / row['path']).read_bytes()
        if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('source drift: ' + row['path'])


verify()
env = dict(os.environ, CARGO_BUILD_JOBS='2', RUSTUP_TOOLCHAIN='1.95.0',
           CARGO_TARGET_DIR=str(B / 'target-macos-prototype'))
prefix = []
if host == 'linux':
    prefix = ['docker', 'run', '--rm', '--platform', 'linux/amd64',
              '-v', '/Users/int/hexrays/kvasir:/work:ro',
              '-v', str(R) + ':/work/vendor/rax:ro',
              '-v', 'rax-cargo-registry:/usr/local/cargo/registry',
              '-v', 'rax-cargo-git:/usr/local/cargo/git', '-v', 'rax-user-target:/target',
              '-e', 'CARGO_TARGET_DIR=/target/bno-isolation-prototype',
              '-e', 'CARGO_BUILD_JOBS=2', '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu',
              '-w', '/work/vendor/rax', 'assist-rax-validation-rustfmt:1.95-bullseye-20261015']
elif host != 'macos':
    raise ValueError('unknown host')
rows = []
for name, command in [('full', ['test', '--lib']), ('capi', ['test', '-p', 'rax-capi']),
                      ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
                      ('all-targets', ['check', '--all-targets'])]:
    args = prefix + ['cargo', command[0], '--locked', '--offline', '--no-default-features',
                     *command[1:]]
    if command[0] == 'test':
        args += ['--', '--nocapture', '--test-threads=1']
    path = B / (host + '-bno-epoch5-' + name + '.log')
    start = time.monotonic()
    with path.open('wb') as log:
        result = subprocess.run(args, cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT)
    verify()
    rows.append({'gate': name, 'argv': args, 'source-epoch': 5, 'namespace-epoch': 2,
                 'exit-code': result.returncode, 'seconds': time.monotonic() - start,
                 'log': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    record.write_text(json.dumps(rows, indent=2) + '\n')
    print(host, 'BNO', name, result.returncode, flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
