from pathlib import Path
import hashlib
import json
import shutil

Q = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
T = Q.parent / 'native-timer2-set-20261019'
R = Q / 'loader-source'
D = R / 'docs/specifications/windows/native-loader-registry'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (Q, 'loader-macos-epoch3-gates.json'),
    (Q, 'loader-linux-epoch3-gates.json'),
    (S, 'loader-native-epoch3b-gates.json'),
    (Q, 'macos-loader-epoch3-owning-gates.json'),
    (Q, 'linux-loader-epoch3-owning-gates.json'),
    (S, 'loader-native-owning-epoch3.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

owner = json.loads((S / 'loader-owner-epoch3-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'loader-owner-epoch3-launch.json')
select(S, 'loader-owner-epoch3-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

for name in [
    'loader-candidate-epoch3.json', 'loader-candidate-epoch3-archive.json',
    'loader-candidate-epoch2.json', 'loader-candidate-epoch2-archive.json',
    'safeboot-source-epoch1.json', 'safeboot-source-epoch1-archive.json',
    'loader-source-ownership.json', 'loader-plan.json',
    'loader-macos-prototype-epoch3.json', 'loader-macos-prototype-epoch3.log',
    'loader-native-logical-check.json', 'loader-native-negative-controls.json',
    'safeboot-native-baseline-a.json', 'safeboot-native-baseline-a.log',
    'loader-native-epoch3-gates.json',
    'loader-owning-source-hashes-epoch3.json',
    'macos-loader-epoch3-owning-commands.json',
    'linux-loader-epoch3-owning-commands.json',
    'macos-loader-epoch3-pre-cpp-archive-identities.json',
    'macos-loader-epoch3-final-owning-archive-identities.json',
    'linux-loader-epoch3-pre-cpp-archive-identities.json',
    'linux-loader-epoch3-final-owning-archive-identities.json',
    'macos-loader-epoch3-linkage.json',
    'ordinary-program-summary.json',
    'loader-owner-epoch3.rs', 'loader-owner-epoch3-build.ps1',
    'run-loader-owner-epoch3.py', 'wait-loader-owner-epoch3.py',
    'run-loader-unix-epoch3.py', 'run-loader-native-epoch3.py',
    'run-loader-native-epoch3b.py',
    'run-loader-owning-epoch3.py', 'run-loader-native-owning-epoch3.py',
    'collect-loader-archives-epoch3.py', 'loader-owned-cpp-build-epoch3.py',
    'owning-linux-cpp-build-loader-epoch3.py',
    'check-loader-validation.py', 'stress-native-loader-registry.py',
    'stress-loader-validation.py', 'collect-loader-evidence.py',
    'finalize-loader-records.py', 'build-loader-proof.py',
    'run-loader-proof-checks.py', 'run-loader-final-unix-check.py',
    'run-loader-final-windows-check.py',
]:
    select(Q, name)
for name in ['native-loader-epoch3-pre-cpp-archives.json',
             'native-loader-epoch3-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

# Exploration records distinguish core-only candidates from the retained owner.
for stem in ['loader-candidate-epoch2-observer-b', 'loader-candidate-epoch3-run']:
    name = stem + '.json'
    select(Q, name)
    for row in json.loads((Q / name).read_text())['steps']:
        select(Q, row['log'])

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
(Q / 'loader-collected-evidence.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('collected', len(rows), 'validation files')
