from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys

B = Path(__file__).resolve().parent
R = B / 'prototype-source'
host = sys.argv[1]
record = B / ('links-' + host + '-focus-epoch1.json')
if record.exists():
    raise RuntimeError('epoch5 focus already recorded')
manifest = json.loads((B / 'links-source-epoch1.json').read_text())


def verify():
    for row in manifest['files']:
        raw = (R / row['path']).read_bytes()
        if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('source changed: ' + row['path'])


verify()
env = dict(os.environ, RUSTUP_TOOLCHAIN='1.95.0', CARGO_BUILD_JOBS='2',
           CARGO_TARGET_DIR=str(B.parent / 'native-bno-isolation-20261018/target-macos-prototype'))
prefix = []
if host == 'linux':
    prefix = ['docker', 'run', '--rm', '--platform', 'linux/amd64',
              '-v', '/Users/int/hexrays/kvasir:/work:ro',
              '-v', str(R) + ':/work/vendor/rax:ro',
              '-v', 'rax-cargo-registry:/usr/local/cargo/registry',
              '-v', 'rax-cargo-git:/usr/local/cargo/git',
              '-v', 'rax-user-target:/target', '-e', 'CARGO_TARGET_DIR=/target/bno-isolation-prototype',
              '-e', 'CARGO_BUILD_JOBS=2', '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu',
              '-w', '/work/vendor/rax', 'assist-rax-validation-rustfmt:1.95-bullseye-20261015']
elif host != 'macos':
    raise ValueError('unknown host')
args = prefix + ['cargo', 'test', '--locked', '--offline', '--no-default-features',
                 '--lib', 'native_object_reparse', '--', '--nocapture', '--test-threads=1']
row = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'source-epoch': 1, 'parent-rax': 'ce29b4a4bca74a668d7f029e8a69da6a2e0340e7', 'argv': args, 'state': 'running',
       'manifest-sha256': hashlib.sha256((B / 'links-source-epoch1.json').read_bytes()).hexdigest()}
record.write_text(json.dumps(row, indent=2) + '\n')
with (B / ('links-' + host + '-focus-epoch1.log')).open('wb') as log:
    result = subprocess.run(args, cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT)
verify()
row.update({'state': 'finished', 'exit-code': result.returncode,
            'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
record.write_text(json.dumps(row, indent=2) + '\n')
print(host, 'Object-manager link epoch1 focus:', result.returncode, flush=True)
raise SystemExit(result.returncode)
