from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
HOST = sys.argv[1]
SOURCE = BASE / 'rax-final-source'
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
deadline = time.monotonic() + 10800
while True:
    previous = BASE / (HOST + '-token-final-gates.json')
    if previous.exists():
        prior = json.loads(previous.read_text())
        if len(prior) == 7:
            if any(row['exit-code'] != 0 for row in prior):
                raise RuntimeError('prior complete Unix core has failures; inspect before final checks')
            break
    if time.monotonic() > deadline:
        raise TimeoutError('prior Unix core did not finish')
    time.sleep(2)
common = ['--locked', '--offline', '--no-default-features']
gates = [('focused', ['cargo','test',*common,'--lib','token_query','--','--nocapture']),
         ('subject', ['cargo','test',*common,'--lib','primary_token_property_is_retained','--','--nocapture']),
         ('cookie', ['cargo','test',*common,'--lib','user::windows::loader::cookie_tests','--','--nocapture']),
         ('all-targets', ['cargo','check',*common,'--all-targets'])]
rows = []
for name, command in gates:
    log = BASE / (HOST + '-token-epoch2-' + name + '.log')
    start = time.monotonic()
    with log.open('wb') as output:
        result = subprocess.run(prefix + command, cwd=SOURCE, env=env, stdout=output, stderr=subprocess.STDOUT)
    rows.append({'gate': name, 'argv': prefix + command, 'exit-code': result.returncode,
                 'seconds': time.monotonic() - start, 'log': log.name,
                 'sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
    (BASE / (HOST + '-token-epoch2-gates.json')).write_text(json.dumps(rows, indent=2) + '\n')
    print(HOST, name, result.returncode, flush=True)
    if result.returncode != 0: break
sys.exit(int(any(row['exit-code'] != 0 for row in rows)))
