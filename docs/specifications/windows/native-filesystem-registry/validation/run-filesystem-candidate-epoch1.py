"""Candidate-only native continuation; this is not a final parity/consumer gate."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
import traceback

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
OLD = B / 'src-policy-candidate-epoch1'
R = B / 'src-filesystem-candidate-epoch1'
RECORD = S / 'filesystem-candidate-epoch1-run.json'
CONFIG = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
assert not RECORD.exists() and not R.exists()
state = {'state': 'preparing', 'pid': os.getpid(), 'source-epoch': 1,
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'steps': [],
         'scope': 'Candidate fixed FileSystem registry exploration: six shared tests passed; nine native focused tests and normal no-default core build/ordinary continuation run here. Full three-host, C API and retained owning validation remain pending.'}


def save():
    RECORD.write_text(json.dumps(state, indent=2) + '\n')


def identity(path):
    raw = path.read_bytes()
    return {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}


def native(path):
    result = subprocess.run([str(path)], capture_output=True, timeout=30)
    return {'exit-code': result.returncode, 'stdout-hex': result.stdout.hex(), 'stderr-hex': result.stderr.hex()}


def run(name, args, env):
    log = S / ('filesystem-candidate-epoch1-' + name + '.log')
    with log.open('wb') as output:
        child = subprocess.Popen(args, cwd=R, env=env, stdout=output, stderr=subprocess.STDOUT)
        state.update({'state': name, 'child-pid': child.pid})
        save()
        code = child.wait()
    state['steps'].append({'name': name, 'argv': args, 'exit-code': code, 'log': log.name,
                           'bytes': log.stat().st_size, 'sha256': identity(log)['sha256']})
    save()
    assert code == 0, name
    return log


save()
try:
    meta = json.loads((S / 'filesystem-candidate-epoch1-archive.json').read_text())
    manifest = json.loads((S / 'filesystem-candidate-epoch1.json').read_text())
    parent_raw = (S / meta['parent-manifest']).read_bytes()
    assert hashlib.sha256(parent_raw).hexdigest() == meta['parent-manifest-sha256']
    for row in json.loads(parent_raw)['files']:
        expected = CONFIG if row['path'] == '.cargo/config.toml' else row['sha256']
        assert identity(OLD / row['path'])['sha256'] == expected, row['path']
    changed = set(meta['separate-overrides'])
    R.mkdir()
    for row in manifest['files']:
        if row['path'] in changed:
            continue
        source, target = OLD / row['path'], R / row['path']
        target.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(source, target)
        except OSError:
            shutil.copy2(source, target)
    archive = S / meta['archive']
    assert identity(archive)['sha256'] == meta['sha256']
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        assert {m.name for m in members} == changed and len(members) == len(changed)
        assert all(m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts for m in members)
        tar.extractall(R, filter='data')
    for row in manifest['files']:
        expected = CONFIG if row['path'] == '.cargo/config.toml' else row['sha256']
        assert identity(R / row['path'])['sha256'] == expected, row['path']
    env = dict(os.environ)
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env.update(json.loads((S / 'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')))
    env.update({'CARGO_HOME': str(B / 'cargo-home'), 'CARGO_TARGET_DIR': str(B / 'target-wait-owned'),
                'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0', 'RUSTC': str(B / 'toolchain/bin/rustc.exe'),
                'RUSTFLAGS': '-C target-feature=+crt-static'})
    state['disk-free-before'] = shutil.disk_usage('C:/').free
    cargo = str(B / 'toolchain/bin/cargo.exe')
    focus_env = dict(env)
    focus_env.pop('RUSTFLAGS', None)
    focus_env['CARGO_TARGET_DIR'] = str(B / 'target-wait-epoch3')
    focus_log = run('focus', [cargo, 'test', '--locked', '--offline', '--no-default-features', '--lib',
        'filesystem_registry', '--', '--nocapture', '--test-threads=1'], focus_env)
    assert '9 passed; 0 failed;' in focus_log.read_text(errors='replace')
    messages = run('build', [cargo, 'build', '--locked', '--offline', '--no-default-features',
                             '--lib', '--message-format=json'], env)
    artifacts = []
    for line in messages.read_text(errors='replace').splitlines():
        if line.startswith('{'):
            row = json.loads(line)
            if row.get('reason') == 'compiler-artifact' and row['target']['name'] == 'rax':
                artifacts.append(row)
    assert len(artifacts) == 1 and artifacts[0]['features'] == []
    core = Path(next(p for p in artifacts[0]['filenames'] if p.endswith('.rlib')))
    state['core-before'] = identity(core)
    source = S / 'filesystem-candidate-owner-epoch1.rs'
    assert identity(source)['sha256'] == meta['observer-sha256']
    state['observer-before'] = identity(source)
    executable = B / 'filesystem-candidate-owner-epoch1.exe'
    run('observer-build', [str(B / 'toolchain/bin/rustc.exe'), '--edition=2024', '--crate-name',
        'filesystem_candidate_owner_epoch1', str(source), '-C', 'target-feature=+crt-static',
        '--extern', 'rax=' + str(core), '-L', 'dependency=' + str(B / 'target-wait-owned/debug/deps'), '-o', str(executable)], env)
    state['executable-before'] = identity(executable)
    module_rows = json.loads((S / 'native-filesystem-registry-v1-modules-before.json').read_bytes().decode('utf-16' if (S / 'native-filesystem-registry-v1-modules-before.json').read_bytes()[:2] in (b'\xff\xfe',b'\xfe\xff') else 'utf-8-sig'))
    state['runtime-before'] = [identity(Path(row['Path'])) for row in module_rows]
    assert [r['sha256'] for r in state['runtime-before']] == [r['Hash'].lower() for r in module_rows]
    state['outcomes'] = []
    for index, program in enumerate([R / 'tests/fixtures/user/windows/bin/arm64/smoke.exe', Path('C:/Windows/System32/whoami.exe')]):
        row = {'program-before': identity(program), 'native-before': native(program)}
        log = run('ordinary-' + str(index), [str(executable), str(program)], env)
        row.update({'program-after': identity(program), 'native-after': native(program),
                    'log': log.name, 'log-sha256': identity(log)['sha256']})
        assert row['program-before'] == row['program-after'] and row['native-before'] == row['native-after']
        state['outcomes'].append(row)
        save()
    assert state['core-before'] == identity(core)
    assert state['observer-before'] == identity(source)
    assert state['executable-before'] == identity(executable)
    assert state['runtime-before'] == [identity(Path(row['Path'])) for row in module_rows]
    for row in manifest['files']:
        expected = CONFIG if row['path'] == '.cargo/config.toml' else row['sha256']
        assert identity(R / row['path'])['sha256'] == expected, row['path']
    state.update({'state': 'finished', 'exit-code': 0})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
