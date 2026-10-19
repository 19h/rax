from pathlib import Path
import hashlib
import json
import shutil


P = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = P / 'policy-source'
D = R / 'docs/specifications/windows/native-user-policy'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (P, 'policy-macos-epoch1-gates.json'), (P, 'policy-linux-epoch1-gates.json'),
    (S, 'policy-native-epoch1-gates.json'),
    (P, 'macos-policy-epoch1-owning-gates.json'),
    (P, 'linux-policy-epoch1-owning-gates.json'), (S, 'policy-native-owning-epoch1.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

owner = json.loads((S / 'policy-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'policy-owner-epoch1-launch.json')
select(S, 'policy-owner-epoch1-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

names = [
    'policy-candidate-epoch1.json', 'policy-candidate-epoch1-archive.json',
    'policy-ownership.json', 'policy-plan.json', 'policy-prototype-a.json', 'policy-prototype-a.log',
    'policy-baseline-inputs.json', 'baseline-installed.rs', 'baseline-user_policy_tests.rs',
    'run-policy-native-baseline-a.py', 'policy-owning-source-hashes-epoch1.json',
    'macos-policy-epoch1-owning-commands.json', 'linux-policy-epoch1-owning-commands.json',
    'macos-policy-epoch1-pre-cpp-archive-identities.json',
    'macos-policy-epoch1-final-owning-archive-identities.json',
    'linux-policy-epoch1-pre-cpp-archive-identities.json',
    'linux-policy-epoch1-final-owning-archive-identities.json', 'macos-policy-epoch1-linkage.json',
    'ordinary-program-summary.json', 'policy-owner-epoch1.rs', 'policy-owner-epoch1-build.ps1',
    'run-policy-owner-epoch1.py', 'wait-policy-owner-epoch1.py', 'run-policy-unix-epoch1.py',
    'run-policy-native-epoch1.py', 'run-policy-owning-epoch1.py', 'run-policy-native-owning-epoch1.py',
    'collect-policy-archives-epoch1.py', 'policy-owned-cpp-build-epoch1.py',
    'owning-linux-cpp-build-policy-epoch1.py', 'check-policy-validation.py',
    'stress-policy-validation.py', 'collect-policy-evidence.py', 'finalize-policy-records.py',
    'build-policy-proof.py', 'run-policy-proof-checks.py', 'run-policy-final-unix-check.py',
    'run-policy-final-windows-check.py', 'run-policy-candidate-epoch1.py',
    'policy-candidate-owner-epoch1.rs', 'stress-policy-native.py',
    'policy-owning-driver-preflight.json', 'policy-owning-junction.json',
    'run-policy-native-pipeline.py',
]
for name in names:
    select(P, name)
for name in ['policy-native-baseline-a.json', 'policy-native-baseline-a.log',
             'native-policy-epoch1-pre-cpp-archives.json',
             'native-policy-epoch1-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

candidate = json.loads((P / 'policy-candidate-epoch1-run.json').read_text())
assert candidate['state'] == 'finished' and candidate['exit-code'] == 0
select(P, 'policy-candidate-epoch1-run.json')
for row in candidate['steps']:
    select(P, row['log'])

V.mkdir(exist_ok=True)
rows = []
for name, path in sorted(files.items()):
    raw = path.read_bytes()
    target = V / name
    if target.exists():
        assert target.read_bytes() == raw, name
    else:
        shutil.copy2(path, target)
    rows.append({'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
(P / 'policy-collected-evidence.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('collected', len(rows), 'validation files')
