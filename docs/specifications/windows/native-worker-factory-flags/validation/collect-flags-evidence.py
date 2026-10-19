from pathlib import Path
import hashlib
import json
import shutil


P = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = P / 'flags-source'
D = R / 'docs/specifications/windows/native-worker-factory-flags'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (P, 'flags-macos-epoch1-gates.json'), (P, 'flags-linux-epoch1-gates.json'),
    (S, 'flags-native-epoch1-gates.json'),
    (P, 'macos-flags-epoch1-owning-gates.json'),
    (P, 'linux-flags-epoch1-owning-gates.json'), (S, 'flags-native-owning-epoch1.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

owner = json.loads((S / 'flags-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'flags-owner-epoch1-launch.json')
select(S, 'flags-owner-epoch1-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

names = [
    'flags-candidate-epoch1.json', 'flags-candidate-epoch1-archive.json',
    'flags-ownership.json', 'flags-plan.json', 'flags-prototype-a.json', 'flags-prototype-a.log',
    'flags-baseline-proof.json', 'flags-baseline-a.json', 'flags-baseline-a.log',
    'flags-baseline-b.json', 'flags-baseline-b.log', 'baseline-a-worker_factory_flags_tests.rs',
    'baseline-b-worker_factory_flags_tests.rs', 'baseline-b-services_tests.rs',
    'run-flags-baseline-a.py', 'run-flags-baseline-b.py', 'flags-owning-source-hashes-epoch1.json',
    'macos-flags-epoch1-owning-commands.json', 'linux-flags-epoch1-owning-commands.json',
    'macos-flags-epoch1-pre-cpp-archive-identities.json',
    'macos-flags-epoch1-final-owning-archive-identities.json',
    'linux-flags-epoch1-pre-cpp-archive-identities.json',
    'linux-flags-epoch1-final-owning-archive-identities.json', 'macos-flags-epoch1-linkage.json',
    'ordinary-program-summary.json', 'flags-owner-epoch1.rs', 'flags-owner-epoch1-build.ps1',
    'run-flags-owner-epoch1.py', 'wait-flags-owner-epoch1.py', 'run-flags-unix-epoch1.py',
    'run-flags-native-epoch1.py', 'run-flags-owning-epoch1.py', 'run-flags-native-owning-epoch1.py',
    'collect-flags-archives-epoch1.py', 'flags-owned-cpp-build-epoch1.py',
    'owning-linux-cpp-build-flags-epoch1.py', 'check-flags-validation.py',
    'stress-flags-validation.py', 'collect-flags-evidence.py', 'finalize-flags-records.py',
    'build-flags-proof.py', 'run-flags-proof-checks.py', 'run-flags-final-unix-check.py',
    'run-flags-final-windows-check.py', 'run-flags-candidate-epoch1.py',
    'flags-candidate-owner-epoch1.rs', 'stress-flags-native.py',
    'flags-owning-junction.json',
    'freeze-flags-candidate-epoch1.py',
    'run-flags-native-pipeline.py',
]
for name in names:
    select(P, name)
for name in ['native-flags-epoch1-pre-cpp-archives.json',
             'native-flags-epoch1-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

candidate = json.loads((P / 'flags-candidate-epoch1-run.json').read_text())
assert candidate['state'] == 'finished' and candidate['exit-code'] == 0
select(P, 'flags-candidate-epoch1-run.json')
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
