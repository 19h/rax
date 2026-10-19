from pathlib import Path
import hashlib
import json
import shutil


P = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = P / 'filesystem-source'
D = R / 'docs/specifications/windows/native-filesystem-registry'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (P, 'filesystem-macos-epoch1-gates.json'), (P, 'filesystem-linux-epoch1-gates.json'),
    (S, 'filesystem-native-epoch1-gates.json'),
    (P, 'macos-filesystem-epoch1-owning-gates.json'),
    (P, 'linux-filesystem-epoch1-owning-gates.json'), (S, 'filesystem-native-owning-epoch1.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

# Preserve the raw executed Unix records/logs alongside traced basename aliases.
for host in ['macos', 'linux']:
    original = 'policy-' + host + '-epoch1-gates.json'
    raw = json.loads((P / original).read_text())
    assert raw['state'] == 'finished' and raw['exit-code'] == 0
    select(P, original)
    for row in raw['steps']:
        select(P, row['log'])

owner = json.loads((S / 'filesystem-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'filesystem-owner-epoch1-launch.json')
select(S, 'filesystem-owner-epoch1-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

names = [
    'filesystem-candidate-epoch1.json', 'filesystem-candidate-epoch1-archive.json',
    'filesystem-ownership.json', 'filesystem-plan.json', 'filesystem-prototype-a.json', 'filesystem-prototype-a.log',
    'filesystem-baseline-inputs.json', 'baseline-installed.rs', 'baseline-filesystem_tests.rs',
    'run-filesystem-native-baseline-a.py', 'filesystem-owning-source-hashes-epoch1.json',
    'macos-filesystem-epoch1-owning-commands.json', 'linux-filesystem-epoch1-owning-commands.json',
    'macos-filesystem-epoch1-pre-cpp-archive-identities.json',
    'macos-filesystem-epoch1-final-owning-archive-identities.json',
    'linux-filesystem-epoch1-pre-cpp-archive-identities.json',
    'linux-filesystem-epoch1-final-owning-archive-identities.json', 'macos-filesystem-epoch1-linkage.json',
    'ordinary-program-summary.json', 'filesystem-owner-epoch1.rs', 'filesystem-owner-epoch1-build.ps1',
    'run-filesystem-owner-epoch1.py', 'wait-filesystem-owner-epoch1.py', 'run-filesystem-unix-epoch1.py',
    'run-filesystem-native-epoch1.py', 'run-filesystem-owning-epoch1.py', 'run-filesystem-native-owning-epoch1.py',
    'collect-filesystem-archives-epoch1.py', 'filesystem-owned-cpp-build-epoch1.py',
    'owning-linux-cpp-build-filesystem-epoch1.py', 'check-filesystem-validation.py',
    'stress-filesystem-validation.py', 'collect-filesystem-evidence.py', 'finalize-filesystem-records.py',
    'build-filesystem-proof.py', 'run-filesystem-proof-checks.py', 'run-filesystem-final-unix-check.py',
    'run-filesystem-final-windows-check.py', 'run-filesystem-candidate-epoch1.py',
    'filesystem-candidate-owner-epoch1.rs', 'stress-filesystem-native.py',
    'filesystem-baseline-test-name-map.json', 'filesystem-owning-junction.json',
    'filesystem-unix-record-promotion.json', 'promote-filesystem-unix-records.py',
    'freeze-filesystem-candidate-epoch1.py',
    'run-filesystem-native-pipeline.py',
]
for name in names:
    select(P, name)
for name in ['filesystem-native-baseline-a.json', 'filesystem-native-baseline-a.log',
             'native-filesystem-epoch1-pre-cpp-archives.json',
             'native-filesystem-epoch1-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

candidate = json.loads((P / 'filesystem-candidate-epoch1-run.json').read_text())
assert candidate['state'] == 'finished' and candidate['exit-code'] == 0
select(P, 'filesystem-candidate-epoch1-run.json')
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
