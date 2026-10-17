from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

BASE = Path(__file__).resolve().parent
CORPUS = BASE / 'rax-final-source/docs/specifications/windows/native-console-owner'


def update_stdout_identity(root, name):
    path = root / 'native-console-owner-identities.json'
    record = json.loads(path.read_text())
    data = (root / name).read_bytes()
    for row in record['producers']:
        if row['stdout']['path'].endswith(name):
            row['stdout']['bytes'] = len(data)
            row['stdout']['sha256'] = hashlib.sha256(data).hexdigest()
    path.write_text(json.dumps(record, indent=2) + '\n')


def mutate(root, case):
    identities = root / 'native-console-owner-identities.json'
    if case == 'raw-byte-digest':
        path = root / 'native-console-owner-arm64.log'
        path.write_bytes(path.read_bytes() + b'changed\n')
    elif case == 'executable-identity-mismatch':
        path = root / 'native-console-owner-outcomes.json'
        rows = json.loads(path.read_text(encoding='utf-8-sig'))
        rows[0]['exe-sha256'] = '0' * 64
        path.write_text(json.dumps(rows, indent=2) + '\n')
    elif case == 'native-pe-machine':
        record = json.loads(identities.read_text())
        record['producers'][0]['machine'] = 0x8664
        identities.write_text(json.dumps(record, indent=2) + '\n')
    elif case == 'changed-kernel-profile':
        record = json.loads(identities.read_text())
        record['kernel'][2] += 1
        identities.write_text(json.dumps(record, indent=2) + '\n')
    elif case == 'wow-query-status-with-updated-digest':
        name = 'native-console-owner-x86.log'
        path = root / name
        text = path.read_text().replace('result case=48 status=C0000003', 'result case=48 status=C0000005')
        path.write_text(text)
        update_stdout_identity(root, name)
    elif case == 'detached-sentinel-with-updated-digest':
        name = 'native-detached-process-parameters-arm64.log'
        path = root / name
        lines = path.read_text().splitlines()
        for index, line in enumerate(lines):
            if line.startswith('parameters bytes='):
                prefix, data = line.split('hex=')
                raw = bytearray.fromhex(data)
                raw[16:24] = bytes(8)
                lines[index] = prefix + 'hex=' + raw.hex().upper()
        path.write_text('\n'.join(lines) + '\n')
        update_stdout_identity(root, name)
    else:
        raise ValueError(case)


def main():
    baseline = subprocess.run([sys.executable, str(CORPUS / 'check.py')], capture_output=True, text=True)
    if baseline.returncode:
        raise RuntimeError(baseline.stderr)
    cases = ['raw-byte-digest', 'executable-identity-mismatch', 'native-pe-machine',
             'changed-kernel-profile', 'wow-query-status-with-updated-digest',
             'detached-sentinel-with-updated-digest']
    results = []
    for case in cases:
        with tempfile.TemporaryDirectory(prefix='console-owner-evidence-') as temporary:
            root = Path(temporary)
            for source in CORPUS.iterdir():
                if source.is_file():
                    shutil.copyfile(source, root / source.name)
            mutate(root, case)
            result = subprocess.run([sys.executable, str(root / 'check.py')], capture_output=True, text=True)
            if result.returncode == 0:
                raise RuntimeError('Evidence checker accepted ' + case)
            results.append({'case': case, 'rejected': True, 'exit-code': result.returncode,
                            'diagnostic': result.stderr.splitlines()[-1]})
    record = {'baseline': json.loads(baseline.stdout), 'adversarial-cases': results,
              'scope': 'temporary owned copies only; repository native/source bytes were preserved'}
    (BASE / 'evidence-stress-results.json').write_text(json.dumps(record, indent=2) + '\n')
    print('Native evidence baseline passes; all6tampering probes rejected')


if __name__ == '__main__':
    main()
