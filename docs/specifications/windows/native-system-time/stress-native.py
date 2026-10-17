"""Reject incomplete and semantically altered native proof sets."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D = Path(__file__).resolve().parent


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def bind(root):
    identities = load(root / 'native/system-time-v2-final-producer-identities.json')
    for row in identities:
        row['stdout-sha256'] = hashlib.sha256(
            (root / f"native/native-system-time-v2-probe-{row['profile']}.log").read_bytes()).hexdigest()
    save(root / 'native/system-time-v2-final-producer-identities.json', identities)
    for name in ['native-inputs.json', 'primary/sources.json']:
        manifest = load(root / name)
        for row in manifest['files']:
            data = (root / row['path']).read_bytes()
            row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
        save(root / name, manifest)


def change_log(root, transform):
    path = root / 'native/native-system-time-v2-probe-arm64.log'
    path.write_text(transform(path.read_text()))


def missing_profile(root):
    path = root / 'native/system-time-v2-final-producer-identities.json'
    save(path, load(path)[:-1])


def failed_producer(root):
    path = root / 'native/native-system-time-v2-probe-outcomes.json'
    rows = load(path)
    rows[0]['exit-code'] = 1
    save(path, rows)


def wrong_status(root):
    change_log(root, lambda text: text.replace('status=00000000', 'status=C0000005', 1))


def wrong_guard(root):
    change_log(root, lambda text: text.replace('after0=4 after1=4', 'after0=104 after1=4', 1))


def wrong_clock(root):
    import re
    change_log(root, lambda text: re.sub(r'value=[0-9A-Fa-f]+', 'value=0', text, count=1))


def missing_case(root):
    change_log(root, lambda text: '\n'.join(line for line in text.splitlines()
                                           if not line.startswith('case ordinal=0 ')) + '\n')


results = []
for name, mutate in [('missing-profile', missing_profile), ('failed-producer', failed_producer),
                     ('wrong-status', wrong_status), ('wrong-guard', wrong_guard),
                     ('wrong-clock', wrong_clock), ('missing-case', missing_case)]:
    with tempfile.TemporaryDirectory(prefix='system-time-native-stress-') as temp:
        root = Path(temp) / 'proof'
        shutil.copytree(D, root, ignore=shutil.ignore_patterns('validation', '__pycache__'))
        mutate(root)
        bind(root)
        result = subprocess.run([sys.executable, '-B', str(root / 'check-native.py')],
                                capture_output=True, text=True)
        if result.returncode == 0:
            raise RuntimeError('accepted invalid proof: ' + name)
        results.append({'case': name, 'rejected': True, 'error': result.stderr.splitlines()[-1]})
print(json.dumps(results, indent=2))
