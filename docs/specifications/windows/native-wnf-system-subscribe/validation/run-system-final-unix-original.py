from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
HOST = sys.argv[1]
SOURCE = BASE / 'rax-source'
env = os.environ.copy()
env['CARGO_BUILD_JOBS'] = '2'
if HOST == 'macos':
    env['RUSTUP_TOOLCHAIN'] = '1.95.0'
    env['CARGO_TARGET_DIR'] = str(BASE.parent / 'native-cookie-owner-20261016/target-macos')
    prefix = []
else:
    prefix = ['docker', 'run', '--rm', '--platform', 'linux/amd64',
              '-v', '/Users/int/hexrays/kvasir:/work:ro', '-v', str(SOURCE) + ':/work/vendor/rax:ro',
              '-v', 'rax-cargo-registry:/usr/local/cargo/registry', '-v', 'rax-cargo-git:/usr/local/cargo/git',
              '-v', 'rax-user-target:/target', '-e', 'CARGO_TARGET_DIR=/target', '-e', 'CARGO_BUILD_JOBS=2',
              '-e', 'RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu', '-w', '/work/vendor/rax',
              'assist-rax-validation-rustfmt:1.95-bullseye-20261015']
common = ['--locked', '--offline', '--no-default-features']
while True:
    try:
        focused = json.loads((BASE / (HOST + '-wnf-epoch4-gates.json')).read_text())
        if len(focused) == 4:
            if any(row['exit-code'] != 0 for row in focused):
                raise RuntimeError('focused checks failed')
            break
    except (FileNotFoundError, json.JSONDecodeError):
        pass
    time.sleep(5)
manifest = json.loads((BASE / 'epoch4-owning-source-hashes.json').read_text())
for row in manifest['sources']:
    data = (SOURCE / row['path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise RuntimeError('source changed before full checks ' + row['path'])
gates = [('all-targets', ['cargo', 'check', *common, '--all-targets']),
         ('windows', ['cargo', 'test', *common, '--test', 'user_windows', '--test', 'user_windows_memory', '--', '--test-threads=1','--nocapture']),
         ('capi', ['cargo','test','--manifest-path','capi/Cargo.toml',*common,'--','--test-threads=1']),
         ('lib', ['cargo','test',*common,'--lib','--','--test-threads=1'])]
rows = []
for name, command in gates:
    log = BASE / (HOST + '-wnf-system-final-' + name + '.log')
    start = time.monotonic()
    with log.open('wb') as output:
        result = subprocess.run(prefix + command, cwd=SOURCE, env=env, stdout=output, stderr=subprocess.STDOUT)
    rows.append({'gate': name, 'argv': prefix + command, 'exit-code': result.returncode,
                 'seconds': time.monotonic() - start, 'log': log.name,
                 'sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
    (BASE / (HOST + '-wnf-system-final-gates.json')).write_text(json.dumps(rows, indent=2) + '\n')
    print(HOST, name, result.returncode, flush=True)
    if result.returncode != 0: break
sys.exit(int(any(row['exit-code'] != 0 for row in rows)))
