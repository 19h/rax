"""Native Windows validation with fixed inputs and explicit baseline failures."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import traceback

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = B / 'src-mutant-epoch4'
RECORD = S / 'mutant-native-epoch4b-gates.json'
LOCK = B / 'mutant-native-epoch4b.lock'


def save():
    RECORD.write_text(json.dumps(state, indent=2) + '\n')


def verify():
    for row in state.get('supplemental-package-files', []):
        raw = (R / row['path']).read_bytes()
        if hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('supplemental package metadata drift')
    if hashlib.sha256((R / '.cargo/config.toml').read_bytes()).hexdigest() != '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':
        raise RuntimeError('native Cargo source configuration drift')
    for row in manifest['files']:
        raw = (R / row['path']).read_bytes()
        if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('native source drift: ' + row['path'])


def run(name, args, cwd, env):
    path = S / ('mutant-native-epoch4b-' + name + '.log')
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with path.open('wb') as log:
        process = subprocess.Popen(args, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
        state['child-pid'] = process.pid
        state['current-step'] = name
        save()
        code = process.wait()
    raw = path.read_bytes()
    text = raw.decode('utf-8', 'replace')
    state['steps'].append({'name': name, 'argv': args, 'cwd': str(cwd),
        'started-utc': started, 'exit-code': code, 'log': path.name,
        'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw),
        'results': re.findall(r'test result: ([^\r\n]+)', text)})
    save()
    return code, text


if RECORD.exists():
    raise RuntimeError('native epoch4b already attempted')
prior = json.loads((S / 'mutant-native-epoch4-gates.json').read_text())
if prior['state'] != 'failed' or prior['steps'][-1]['name'] != 'focus' or prior['steps'][-1]['exit-code'] != 101:
    raise RuntimeError('native original failure changed; inspect before resuming')
failed = (S / prior['steps'][-1]['log']).read_text()
if 'vendor\\vm-memory\\Cargo.toml' not in failed:
    raise RuntimeError('native dependency metadata failure changed')
descriptor = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(descriptor)
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'state': 'resuming-after-compiler-environment', 'source-epoch': 4,
         'prior-record': 'mutant-native-epoch4-gates.json', 'steps': []}
save()
try:
    manifest = json.loads((S / 'mutant-source-epoch4.json').read_text())
    metadata = json.loads((S / 'mutant-extra-cargo-metadata.json').read_text())
    for row in metadata:
        source = S / 'mutant-extra-cargo-metadata' / row['path']
        raw = source.read_bytes()
        if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
            raise RuntimeError('supplemental package metadata changed')
        previous = B / 'src-object-reparse-epoch1' / row['path']
        if row['path'].endswith('/Cargo.toml') and (not previous.exists() or previous.read_bytes() != raw):
            raise RuntimeError('vendored dependency manifest differs from native baseline')
        target = R / row['path']
        if target.exists() and target.read_bytes() != raw:
            raise RuntimeError('do not overwrite changed native source')
        target.parent.mkdir(exist_ok=True, parents=True)
        shutil.copy2(source, target)
    state['supplemental-package-files'] = metadata
    verify()
    state['prepared-source-inputs'] = len(manifest['files'])
    env = dict(os.environ)
    for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']:
        env.pop(key, None)
    env.update({'CARGO_HOME': str(B / 'cargo-home'),
                'CARGO_TARGET_DIR': str(B / 'target-bno-isolation-prototype'),
                'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0',
                'RUSTC': str(B / 'toolchain/bin/rustc.exe')})
    selected = json.loads((S / 'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig'))
    env.update(selected)
    code, version = run('toolchain', [str(B / 'toolchain/bin/cargo.exe'), '--version'], R, env)
    if code or 'cargo 1.95.0' not in version:
        raise RuntimeError('native Cargo version differs')
    baseline = set(json.loads((S / 'mutant-native-baseline-failures.json').read_text())['failures'])
    gates = [('focus', ['test', '--lib', 'native_mutant']), ('full', ['test', '--lib']),
             ('capi', ['test', '-p', 'rax-capi']),
             ('integration', ['test', '--test', 'user_windows', '--test', 'user_windows_memory']),
             ('all-targets', ['check', '--all-targets'])]
    for name, command in gates:
        args = [str(B / 'toolchain/bin/cargo.exe'), command[0], '--locked', '--offline', '--no-default-features', *command[1:]]
        if command[0] == 'test':
            args += ['--', '--nocapture', '--test-threads=1']
        code, output = run(name, args, R, env)
        verify()
        if name == 'full':
            final = output[output.rfind('\nfailures:\n'):]
            failures = set(re.findall(r'^    (\S+)\r?$', final, re.M))
            if code != 101 or failures != baseline:
                raise RuntimeError('native full-suite failures differ from exact five-name baseline')
            state['exact-existing-failures'] = sorted(failures)
        elif code:
            raise RuntimeError('native gate failed: ' + name)
        if name == 'focus' and '11 passed; 0 failed;' not in output:
            raise RuntimeError('native focused selection was incomplete')
        save()
    state.update({'state': 'finished', 'exit-code': 0, 'post-source-inputs': len(manifest['files']),
                  'qualification': 'Full core retains exactly the five pre-existing lowerer failures.'})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    LOCK.unlink()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
