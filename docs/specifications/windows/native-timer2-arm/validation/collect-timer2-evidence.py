from pathlib import Path
import hashlib
import json
import shutil

T = Path(__file__).resolve().parent
R = T / 'timer2-source'
D = R / 'docs/specifications/windows/native-timer2-arm'
V = D / 'validation'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')

for base, name in [(T, 'timer2-macos-epoch1-gates.json'), (T, 'timer2-linux-epoch1-gates.json'),
                   (S, 'timer2-native-epoch1-gates.json'), (T, 'macos-timer2-epoch1-owning-gates.json'),
                   (T, 'linux-timer2-epoch1-owning-gates.json'), (S, 'timer2-native-owning-epoch1.json')]:
    state = json.loads((base / name).read_text())
    assert state['state'] == 'finished' and state['exit-code'] == 0, name

V.mkdir(exist_ok=True)
files = {}
patterns = ['timer2-*.json', 'timer2-*.log', 'macos-timer2-epoch1*.json',
            'macos-timer2-epoch1*.log', 'linux-timer2-epoch1*.json', 'linux-timer2-epoch1*.log',
            'native-timer2-epoch1*.json', 'native-timer2-epoch1*.log']
for base in [T, S]:
    for pattern in patterns:
        for p in base.glob(pattern):
            if p.name == 'timer2-collected-evidence.json' or p.name.startswith('timer2-finalization'):
                continue
            if not p.is_file():
                continue
            if p.name in files:
                assert p.read_bytes() == files[p.name].read_bytes(), p.name
            files[p.name] = p

for name in ['ordinary-program-summary.json', 'collect-timer2-evidence.py', 'finalize-timer2-records.py', 'native-timer2-logical-check.json', 'timer2-owner-epoch1.rs',
             'timer2-owner-epoch1-build.ps1', 'freeze-timer2-epoch1.py',
             'prepare-timer2-native-epoch1.py', 'collect-timer2-archives-epoch1.py',
             'timer2-owned-cpp-build-epoch1.py', 'owning-linux-cpp-build-timer2-epoch1.py',
             'run-timer2-unix-epoch1.py', 'run-timer2-native-epoch1.py',
             'run-timer2-owning-epoch1.py', 'run-timer2-native-owning-epoch1.py',
             'run-timer2-owner-epoch1.py', 'wait-timer2-owner-epoch1.py',
             'capture-timer2-host-identity.py', 'stress-native-timer2.py',
             'check-timer2-validation.py', 'stress-timer2-validation.py',
             'build-timer2-proof.py', 'run-timer2-proof-checks.py',
             'run-timer2-final-unix-check.py', 'run-timer2-final-windows-check.py']:
    files[name] = T / name

rows = []
for name, path in sorted(files.items()):
    raw = path.read_bytes()
    target = V / name
    if target.exists():
        assert target.read_bytes() == raw, name
    else:
        shutil.copy2(path, target)
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
(T / 'timer2-collected-evidence.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('collected', len(rows), 'validation records and commands')
