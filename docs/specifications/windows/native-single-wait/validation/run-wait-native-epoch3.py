"""Validate a complete, fixed source snapshot on native Windows ARM64."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import traceback

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = B / 'src-wait-epoch3'
RECORD = S / 'wait-native-epoch3-gates.json'
LOCK = B / 'wait-native-epoch3.lock'
CONFIG_SHA = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'

def save():
    RECORD.write_text(json.dumps(state, indent=2) + '\n')

def verify():
    for row in manifest['files']:
        raw = (R / row['path']).read_bytes()
        expected = CONFIG_SHA if row['path'] == '.cargo/config.toml' else row['sha256']
        assert hashlib.sha256(raw).hexdigest() == expected, row['path']
        assert row['path'] == '.cargo/config.toml' or len(raw) == row['bytes'], row['path']

def run(name, command, env):
    path = S / ('wait-native-epoch3-' + name + '.log')
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with path.open('wb') as log:
        child = subprocess.Popen(command, cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT)
        state.update({'current-step': name, 'child-pid': child.pid})
        save()
        code = child.wait()
    raw = path.read_bytes()
    output = raw.decode('utf-8', 'replace')
    state['steps'].append({'name': name, 'argv': command, 'cwd': str(R), 'started-utc': started,
        'exit-code': code, 'log': path.name, 'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw),
        'results': re.findall(r'test result: ([^\r\n]+)', output)})
    save()
    return code, output

assert not RECORD.exists(), 'native epoch3 already attempted'
fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(fd)
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'state': 'preparing', 'source-epoch': 3, 'steps': []}
save()
try:
    manifest = json.loads((S / 'wait-source-epoch3.json').read_text())
    archive = json.loads((S / 'wait-source-epoch3-archive.json').read_text())
    path = S / archive['archive']
    assert path.stat().st_size == archive['bytes']
    assert hashlib.sha256(path.read_bytes()).hexdigest() == archive['sha256']
    assert not R.exists(), 'destination already exists; inspect prior preparation'
    R.mkdir()
    with tarfile.open(path, 'r:gz') as tar:
        members = tar.getmembers()
        assert len(members) == len(manifest['files'])
        assert {member.name for member in members} == {row['path'] for row in manifest['files']}
        assert all(member.isfile() and not Path(member.name).is_absolute() and '..' not in Path(member.name).parts for member in members)
        tar.extractall(R, filter='data')
    original = R / '.cargo/config.toml'
    state['repository-cargo-config-sha256'] = hashlib.sha256(original.read_bytes()).hexdigest()
    config = B / 'src-mutant-epoch4/.cargo/config.toml'
    assert hashlib.sha256(config.read_bytes()).hexdigest() == CONFIG_SHA
    shutil.copy2(config, original)
    state['native-cargo-source-config'] = {'sha256': CONFIG_SHA, 'bytes': original.stat().st_size,
        'scope': 'Offline dependency source replacement only; no target/build flags. Full parent configuration remains in bound source snapshot.'}
    verify()
    state.update({'state': 'testing', 'prepared-source-inputs': len(manifest['files'])})
    save()
    env = dict(os.environ)
    for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']:
        env.pop(key, None)
    env.update({'CARGO_HOME': str(B / 'cargo-home'), 'CARGO_TARGET_DIR': str(B / 'target-bno-isolation-prototype'),
        'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0', 'RUSTC': str(B / 'toolchain/bin/rustc.exe')})
    env.update(json.loads((S / 'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
    cargo = str(B / 'toolchain/bin/cargo.exe')
    code, output = run('toolchain', [cargo, '--version'], env)
    assert code == 0 and 'cargo 1.95.0' in output
    baseline = set(json.loads((S / 'mutant-native-baseline-failures.json').read_text())['failures'])
    gates = [('focus', ['test', '--lib', 'native_single_wait']), ('full', ['test', '--lib']),
        ('capi', ['test', '-p', 'rax-capi']), ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
        ('all-targets', ['check', '--all-targets'])]
    for name, command in gates:
        args = [cargo, command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
        if command[0] == 'test':
            args += ['--', '--nocapture', '--test-threads=1']
        code, output = run(name, args, env)
        verify()
        if name == 'full':
            final = output[output.rfind('\nfailures:\n'):]
            failures = set(re.findall(r'^    (\S+)\r?$', final, re.M))
            assert code == 101 and failures == baseline, 'native full suite differs from exact five-name baseline'
            state['exact-existing-failures'] = sorted(failures)
        else:
            assert code == 0, 'native gate failed: ' + name
        if name == 'focus':
            assert '16 passed; 0 failed;' in output, 'native focus selection incomplete'
        save()
    state.update({'state': 'finished', 'exit-code': 0, 'post-source-inputs': len(manifest['files']),
        'qualification': 'Native full core retains exactly the five established lowerer failures.'})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    LOCK.unlink()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
