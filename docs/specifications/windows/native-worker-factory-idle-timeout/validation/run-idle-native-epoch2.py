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
import time
import sys

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = B / 'src-idle-candidate-epoch2'
RECORD = S / 'idle-native-epoch2-gates.json'
LOCK = B / 'idle-native-epoch2.lock'
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
    path = S / ('idle-native-epoch2-' + name + '.log')
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
    'state': 'preparing', 'source-epoch': 2, 'steps': []}
save()
try:
    manifest = json.loads((S / 'idle-candidate-epoch2.json').read_text())
    regression = json.loads((S / 'idle-baseline-proof.json').read_text())
    assert regression['state'] == 'expected-regression-failed' and regression['passed'] == 0 and regression['failed'] == 3
    candidate = json.loads((S / 'idle-candidate-epoch2-run.json').read_text())
    assert candidate['state'] == 'finished' and candidate['exit-code'] == 0
    state['materialization'] = {'candidate-record': 'idle-candidate-epoch2-run.json', 'complete-inputs': len(manifest['files']), 'scope': 'Previously materialized immutable candidate, all inputs reverified by this full gate.'}
    state['native-cargo-source-config'] = {'sha256': CONFIG_SHA, 'bytes': 374}
    prior = json.loads((S / 'flags-native-epoch1-gates.json').read_text())
    assert prior['state'] == 'finished' and prior['exit-code'] == 0
    state['prior-record'] = 'flags-native-epoch1-gates.json'
    state['qualification'] = 'Parent gates completed; complete immutable source snapshot with separate current-mtime overrides, six shared and one installed native/WoW64 worker idle leaf,8944 native idle timeout observations including clamp/access/alignment/cross-page controls and immutable captured identity.'
    verify()
    state.update({'state': 'testing', 'prepared-source-inputs': len(manifest['files'])})
    save()
    env = dict(os.environ)
    for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']:
        env.pop(key, None)
    env.update({'CARGO_HOME': str(B / 'cargo-home'), 'CARGO_TARGET_DIR': str(B / 'target-wait-epoch3'),
        'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0', 'RUSTC': str(B / 'toolchain/bin/rustc.exe')})
    env.update(json.loads((S / 'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
    cargo = str(B / 'toolchain/bin/cargo.exe')
    code, output = run('toolchain', [cargo, '--version'], env)
    assert code == 0 and 'cargo 1.95.0' in output
    baseline = set(json.loads((S / 'mutant-native-baseline-failures.json').read_text())['failures'])
    gates = [('focus', ['test', '--lib', 'native_worker_factory_idle']), ('full', ['test', '--lib']), ('capi', ['test', '-p', 'rax-capi']), ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']), ('all-targets', ['check', '--all-targets'])]
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
            assert '7499 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;' in output, 'native complete selection/count differs'
            state['exact-existing-failures'] = sorted(failures)
        else:
            assert code == 0, 'native gate failed: ' + name
        if name == 'focus':
            assert '7 passed; 0 failed;' in output, 'native focus selection incomplete'
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
