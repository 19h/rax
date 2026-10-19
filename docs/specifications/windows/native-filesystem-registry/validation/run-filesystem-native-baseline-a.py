from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
OLD = B / 'src-policy-candidate-epoch1'
R = B / 'src-filesystem-baseline-a'
record = S / 'filesystem-native-baseline-a.json'
assert not record.exists() and not R.exists()
meta = json.loads((S / 'filesystem-baseline-inputs.json').read_text())
parent = json.loads((S / meta['parent-manifest']).read_text())
changed = {row['path']: row for row in meta['test-only-overrides']}
config = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
state = {'state': 'preparing', 'pid': os.getpid(), 'scope': meta['scope']}
record.write_text(json.dumps(state, indent=2) + '\n')
R.mkdir()
for row in parent['files']:
    source = OLD / row['path']
    raw = source.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == (config if row['path'] == '.cargo/config.toml' else row['sha256'])
    if row['path'] in changed:
        continue
    target = R / row['path']
    target.parent.mkdir(parents=True, exist_ok=True)
    os.link(source, target)
for name, row in changed.items():
    source = S / row['transfer']
    raw = source.read_bytes()
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    target = R / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(raw)
env = dict(os.environ)
for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']:
    env.pop(key, None)
env.update(json.loads((S / 'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
env.update(CARGO_HOME=str(B / 'cargo-home'), CARGO_TARGET_DIR=str(B / 'target-wait-epoch3'),
           CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0', RUSTC=str(B / 'toolchain/bin/rustc.exe'))
args = [str(B / 'toolchain/bin/cargo.exe'), 'test', '--locked', '--offline', '--no-default-features',
        '--lib', 'installed_filesystem_snapshot_matches_independent_native_presence_and_views',
        '--', '--nocapture', '--test-threads=1']
log = S / 'filesystem-native-baseline-a.log'
with log.open('xb') as output:
    child = subprocess.Popen(args, cwd=R, env=env, stdout=output, stderr=subprocess.STDOUT)
    state.update(state='running', child=child.pid, argv=args)
    record.write_text(json.dumps(state, indent=2) + '\n')
    code = child.wait()
raw = log.read_bytes()
text = raw.decode(errors='replace')
assert code == 101 and '0 passed; 1 failed;' in text and 'assertion failed: matches!' in text
for name, row in changed.items():
    assert hashlib.sha256((R / name).read_bytes()).hexdigest() == row['sha256']
state.update(state='expected-regression-failed', **{'exit-code': code, 'passed': 0, 'failed': 1,
    'log': log.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
record.write_text(json.dumps(state, indent=2) + '\n')
print(json.dumps(state, indent=2))
