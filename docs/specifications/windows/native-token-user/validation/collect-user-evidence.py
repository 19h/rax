from pathlib import Path
import hashlib
import json
import shutil

Q = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
T = Q.parent / 'native-timer2-set-20261019'
R = Q / 'user-source'
D = R / 'docs/specifications/windows/native-token-user'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (Q, 'user-macos-epoch1-gates.json'),
    (Q, 'user-linux-epoch1-gates.json'),
    (S, 'user-native-epoch1-gates.json'),
    (Q, 'macos-user-epoch1-owning-gates.json'),
    (Q, 'linux-user-epoch1-owning-gates.json'),
    (S, 'user-native-owning-epoch1.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

owner = json.loads((S / 'user-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'user-owner-epoch1-launch.json')
select(S, 'user-owner-epoch1-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

for name in ['user-candidate-epoch1.json', 'user-candidate-epoch1-archive.json', 'initial-ownership.json', 'user-plan.json', 'user-prototype-a.json', 'user-prototype-a.log', 'user-baseline-inputs.json', 'user-baseline-b.json', 'user-baseline-b.log', 'baseline-token_query_tests.rs', 'baseline-token_user_tests.rs', 'user-owning-source-hashes-epoch1.json', 'macos-user-epoch1-owning-commands.json', 'linux-user-epoch1-owning-commands.json', 'macos-user-epoch1-pre-cpp-archive-identities.json', 'macos-user-epoch1-final-owning-archive-identities.json', 'linux-user-epoch1-pre-cpp-archive-identities.json', 'linux-user-epoch1-final-owning-archive-identities.json', 'macos-user-epoch1-linkage.json', 'ordinary-program-summary.json', 'user-owner-epoch1.rs', 'user-owner-epoch1-build.ps1', 'run-user-owner-epoch1.py', 'wait-user-owner-epoch1.py', 'run-user-unix-epoch1.py', 'run-user-native-epoch1.py', 'run-user-owning-epoch1.py', 'run-user-native-owning-epoch1.py', 'collect-user-archives-epoch1.py', 'user-owned-cpp-build-epoch1.py', 'owning-linux-cpp-build-user-epoch1.py', 'check-user-validation.py', 'stress-user-validation.py', 'collect-user-evidence.py', 'finalize-user-records.py', 'build-user-proof.py', 'run-user-proof-checks.py', 'run-user-final-unix-check.py', 'run-user-final-windows-check.py', 'run-user-candidate-epoch1.py', 'user-candidate-owner-epoch1.rs']:
    select(Q, name)
for name in ['native-user-epoch1-pre-cpp-archives.json',
             'native-user-epoch1-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

# Candidate exploration is explicitly separate from final retained-owner proof.
select(Q, 'user-candidate-epoch1-run.json')
for row in json.loads((Q / 'user-candidate-epoch1-run.json').read_text())['steps']:
    select(Q, row['log'])

for path in sorted(Q.glob('native-token-user-teb-v1*')):
    if path.is_file(): select(Q, path.name)
select(Q, 'run-native-token-user-teb-v1.py')

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
(Q / 'user-collected-evidence.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('collected', len(rows), 'validation files')
