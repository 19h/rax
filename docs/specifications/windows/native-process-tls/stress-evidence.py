from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--corpus', type=Path, default=Path(__file__).parent)
parser.add_argument('--output-directory', type=Path)
args = parser.parse_args()
temporary = tempfile.TemporaryDirectory(prefix='rax-tls-evidence-') if args.output_directory is None else None
BASE = args.output_directory or Path(temporary.name)
BASE.mkdir(parents=True, exist_ok=True)
CORPUS = args.corpus.resolve()
RESULTS = []


def run_case(name, mutate, optimized=False):
    target = BASE / 'evidence-stress' / name
    shutil.copytree(CORPUS, target, dirs_exist_ok=True)
    mutate(target)
    command = [sys.executable, '-O' if optimized else '-B', str(target / 'verify-evidence.py')]
    result = subprocess.run(command, capture_output=True, text=True)
    (target / 'validator.log').write_text(result.stdout + result.stderr)
    if result.returncode == 0:
        raise ValueError('mutation was accepted: ' + name)
    RESULTS.append({'case': name, 'exit-code': result.returncode, 'rejected': True})


def change_json(directory, name, mutate):
    path = directory / name
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + '\n')


run_case('executed-source-byte', lambda d: (d / 'native-process-tls-control.cpp').write_bytes((d / 'native-process-tls-control.cpp').read_bytes() + b'\n'))
run_case('actual-executable-hash', lambda d: change_json(d, 'native-process-tls-alias-identities.json', lambda x: x['producers'][0]['executable'].__setitem__('sha256', '0' * 64)))
run_case('physical-kernel', lambda d: change_json(d, 'native-process-tls-extended-identities.json', lambda x: x['kernel'].__setitem__(2, 1)))
run_case('principal', lambda d: change_json(d, 'native-process-tls-alias-identities.json', lambda x: x.__setitem__('principal', 'unknown')))
run_case('library-identity', lambda d: change_json(d, 'native-process-tls-length-order-identities.json', lambda x: x['libraries'][0].__setitem__('sha256', '0' * 64)))
run_case('aggregate-count', lambda d: change_json(d, 'native-process-tls-length-order-identities.json', lambda x: x.__setitem__('accepted-total-control-count', 720)))


def change_status(directory):
    path = directory / 'native-process-tls-control-arm64.log'
    text = path.read_text().replace('status=00000000', 'status=C0000005', 1)
    path.write_text(text)
    data = path.read_bytes()
    change_json(directory, 'native-process-tls-initial-identities.json',
                lambda x: x['producers'][0]['stdout'].update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))


run_case('status-with-refreshed-log-hash', change_status)
run_case('optimized-validation', lambda d: None, True)
(BASE / 'evidence-stress-results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')
print('Rejected', len(RESULTS), 'evidence falsification probes.')
