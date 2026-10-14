from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent
base = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(tempfile.mkdtemp(prefix='rax-section-map-evidence-'))
sandbox = base / 'evidence-falsification'
sandbox.mkdir(parents=True, exist_ok=True)
report = []


def run_case(name, mutate):
    directory = sandbox / name
    directory.mkdir(exist_ok=True)
    for source in root.rglob('*'):
        if source.is_file():
            target = directory / source.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
    mutate(directory)
    result = subprocess.run([sys.executable, '-B', str(root / 'verify-evidence.py'), str(directory)],
                            capture_output=True, text=True)
    if result.returncode == 0:
        raise ValueError('accepted falsification: ' + name)
    report.append({'case': name, 'rejected': True, 'exit-code': result.returncode,
                   'last-error': result.stderr.splitlines()[-1]})


def edit_json(path, change):
    value = json.loads(path.read_text(encoding='utf-8-sig'))
    change(value)
    path.write_text(json.dumps(value, indent=2) + '\n')


run_case('source-byte', lambda p: (p / 'native-section-map-access.cpp').write_bytes(
    (p / 'native-section-map-access.cpp').read_bytes() + b'\n'))
run_case('executed-binary', lambda p: edit_json(p / 'native-section-map-access-outcomes.json',
    lambda v: v[0].__setitem__('exe-sha256', '0' * 64)))
run_case('physical-kernel', lambda p: edit_json(p / 'native-section-map-access-identities.json',
    lambda v: v.__setitem__('native-machine', 0x8664)))
run_case('subject', lambda p: edit_json(p / 'native-section-map-access-identities.json',
    lambda v: v.__setitem__('principal', 'S-1-5-21-unmeasured')))
run_case('native-library', lambda p: edit_json(p / 'native-section-map-access-identities.json',
    lambda v: v['libraries'][0].__setitem__('sha256', '0' * 64)))


def missing_row(p):
    log = p / 'native-section-map-access-arm64.log'
    raw = log.read_bytes()
    lines = raw.splitlines(keepends=True)
    raw = b''.join(line for line in lines if not line.startswith(b'case=913 '))
    log.write_bytes(raw)
    edit_json(p / 'native-section-map-access-identities.json', lambda v:
              v['producers'][0].__setitem__('stdout', {'path': v['producers'][0]['stdout']['path'],
              'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}))


run_case('missing-row-rehashed', missing_row)
run_case('producer-failed-exit', lambda p: edit_json(p / 'native-section-map-access-outcomes.json',
    lambda v: v[0].__setitem__('exit-code', 21)))
run_case('laa', lambda p: edit_json(p / 'native-section-map-access-identities.json',
    lambda v: v['producers'][2]['executable'].__setitem__('characteristics', 0x20)))
result = subprocess.run([sys.executable, '-O', str(root / 'verify-evidence.py')], capture_output=True, text=True)
if result.returncode == 0:
    raise ValueError('optimized verifier accepted')
report.append({'case': 'optimized', 'rejected': True, 'exit-code': result.returncode})
(base / 'evidence-falsification-results.json').write_text(json.dumps(report, indent=2) + '\n')
print('rejected controlled falsifications', len(report))
