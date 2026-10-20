"""Collect, adversarially check, and verify the immutable ledger on three hosts."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import traceback

CACHE = Path(__file__).parent
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
ROOT = Path('/Users/int/.assist-namespace-20261020')
DOCS = ROOT / 'docs/specifications/windows/native-namespace-diagnostics'
VALIDATION = DOCS / 'validation'
RECORD = CACHE / 'namespace-proof-controller-b.json'
assert not RECORD.exists()
state = {'state': 'waiting-for-gates', 'pid': os.getpid(), 'steps': []}


def load(path):
    try:
        return json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def save():
    state['utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    RECORD.write_text(json.dumps(state, indent=2) + '\n')


def envelope():
    rows = []
    for path in sorted(DOCS.rglob('*')):
        if path.is_file() and path.name != 'evidence-manifest.json':
            raw = path.read_bytes()
            rows.append({'path': path.relative_to(DOCS).as_posix(), 'bytes': len(raw),
                         'sha256': hashlib.sha256(raw).hexdigest()})
    (DOCS / 'evidence-manifest.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')


def run(name, argv):
    state.update({'state': 'running', 'step': name})
    save()
    log = CACHE / ('namespace-proof-' + name + '.log')
    with log.open('wb') as output:
        code = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT).returncode
    state['steps'].append({'name': name, 'exit-code': code, 'log': log.name})
    save()
    assert code == 0, log.read_text(errors='replace')


save()
try:
    while not (VALIDATION / 'summary.json').exists():
        gates = load(SHARE / 'namespace-native-epoch1-gates.json')
        assert gates.get('state') != 'failed', gates
        time.sleep(5)
    shutil.copy2(Path(__file__), VALIDATION / Path(__file__).name)
    envelope()
    run('negative-controls', [sys.executable, '-B', str(CACHE / 'stress-namespace-proof.py')])
    for number in [1, 2]:
        envelope()
        run('package-round' + str(number), [sys.executable, '-B',
            str(CACHE / 'package-namespace-verification-docs-epoch1.py'), str(number)])
        children = []
        for host in ['macos', 'linux']:
            child = subprocess.Popen([sys.executable, '-B',
                str(CACHE / 'run-namespace-host-verification-epoch1.py'), host, str(number)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            children.append((host, child))
        children.append(('windows', subprocess.Popen([
            'prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
            'C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe', '-B',
            'C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\run-namespace-native-verification-epoch1.py',
            str(number)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)))
        for host, child in children:
            code = child.wait()
            state['steps'].append({'name': host + '-round' + str(number) + '-transport', 'exit-code': code})
            save()
            if host != 'windows':
                assert code == 0, host
        for host in ['macos', 'linux', 'windows']:
            base = SHARE if host == 'windows' else CACHE
            name = 'namespace-epoch1-final-check-' + host + '-round' + str(number) + '.json'
            while load(base / name).get('state') not in ['finished', 'failed']:
                time.sleep(5)
            receipt = load(base / name)
            assert receipt['state'] == 'finished' and receipt['exit-code'] == 0, receipt
            if number == 1:
                shutil.copy2(base / name, VALIDATION / name)
                for row in receipt['steps']:
                    shutil.copy2(base / row['log'], VALIDATION / row['log'])
        if number == 1:
            summary = load(VALIDATION / 'summary.json')
            summary['portable-proof-round1'] = {
                host: {'exit-code': 0, 'envelope': load(VALIDATION /
                    ('namespace-epoch1-final-check-' + host + '-round1.json'))['envelope']}
                for host in ['macos', 'linux', 'windows']}
            (VALIDATION / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    state.update({'state': 'finished', 'exit-code': 0})
except BaseException:
    state.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
finally:
    save()
print(json.dumps(state, indent=2), flush=True)
raise SystemExit(state['exit-code'])
