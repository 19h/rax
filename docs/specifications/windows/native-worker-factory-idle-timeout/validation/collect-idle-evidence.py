from pathlib import Path
import hashlib
import json
import shutil


P = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = P / 'idle-source-epoch2'
D = R / 'docs/specifications/windows/native-worker-factory-idle-timeout'
V = D / 'validation'
files = {}


def select(base, name):
    path = base / name
    assert path.is_file(), str(path)
    if name in files:
        assert path.read_bytes() == files[name].read_bytes(), name
    files[name] = path


for base, name in [
    (P, 'idle-macos-epoch2-gates.json'), (P, 'idle-linux-epoch2-gates.json'),
    (S, 'idle-native-epoch2-gates.json'),
    (P, 'macos-idle-epoch2-owning-gates.json'),
    (P, 'linux-idle-epoch2-owning-gates.json'), (S, 'idle-native-owning-epoch2.json'),
]:
    record = json.loads((base / name).read_text())
    assert record['state'] == 'finished' and record['exit-code'] == 0, name
    select(base, name)
    for row in record['steps']:
        select(base, row['log'])

owner = json.loads((S / 'idle-owner-epoch2-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
select(S, 'idle-owner-epoch2-launch.json')
select(S, 'idle-owner-epoch2-build.log')
for row in owner['outcomes']:
    select(S, row['log'])

names = ['idle-candidate-epoch1.json', 'idle-candidate-epoch1-archive.json', 'idle-candidate-epoch2.json', 'idle-candidate-epoch2-archive.json', 'idle-ownership.json', 'idle-plan.json', 'idle-prototype-a.json', 'idle-prototype-a.log', 'idle-prototype-b.json', 'idle-prototype-b.log', 'idle-baseline-proof.json', 'idle-baseline-a.json', 'idle-baseline-a.log', 'run-idle-baseline-a.py', 'idle-owning-source-hashes-epoch2.json', 'macos-idle-epoch2-owning-commands.json', 'linux-idle-epoch2-owning-commands.json', 'macos-idle-epoch2-pre-cpp-archive-identities.json', 'macos-idle-epoch2-final-owning-archive-identities.json', 'linux-idle-epoch2-pre-cpp-archive-identities.json', 'linux-idle-epoch2-final-owning-archive-identities.json', 'macos-idle-epoch2-linkage.json', 'ordinary-program-summary.json', 'idle-owner-epoch2.rs', 'idle-owner-epoch2-build.ps1', 'run-idle-owner-epoch2.py', 'wait-idle-owner-epoch2.py', 'run-idle-unix-epoch2.py', 'run-idle-native-epoch2.py', 'run-idle-owning-epoch2.py', 'run-idle-native-owning-epoch2.py', 'collect-idle-archives-epoch2.py', 'idle-owned-cpp-build-epoch2.py', 'owning-linux-cpp-build-idle-epoch2.py', 'check-idle-validation.py', 'stress-idle-validation.py', 'collect-idle-evidence.py', 'finalize-idle-records.py', 'build-idle-proof.py', 'run-idle-proof-checks.py', 'run-idle-final-unix-check.py', 'run-idle-final-windows-check.py', 'run-idle-candidate-epoch1.py', 'run-idle-candidate-epoch2.py', 'idle-candidate-owner-epoch2.rs', 'run-idle-native-negatives.py', 'idle-retained-link-preparation.json', 'idle-retained-link-preparation-epoch2.json', 'idle-source-preflight-corrections.json', 'idle-final-observer-plan.json', 'idle-epoch2-correction.json', 'freeze-idle-candidate-epoch1.py', 'freeze-idle-candidate-epoch2.py', 'run-idle-native-pipeline-epoch2.py']
names += ['idle-proof-preflight-corrections.json', 'check-idle-validation-initial.py', 'idle-finalization.json', 'idle-finalization-records.log', 'run-idle-finalization-b.py', 'collect-idle-evidence-initial.py', 'idle-workspace-format.json', 'idle-workspace-format.log']
names += ['idle-proof-preflight-corrections-b.json', 'stress-idle-validation-initial.py', 'idle-finalization-b.json', 'idle-finalization-b-records.log', 'run-idle-finalization-c.py', 'finalize-idle-records-resume.py']
for name in names:
    select(P, name)
for name in ['native-idle-epoch2-pre-cpp-archives.json',
             'native-idle-epoch2-final-owning-archive-identities.json']:
    select(S, name)
for name in ['timer2-native-host-identity.json', 'capture-timer2-host-identity.py']:
    select(R / 'docs/specifications/windows/native-timer2-arm/validation', name)

candidate = json.loads((S / 'idle-candidate-epoch2-run.json').read_text())
assert candidate['state'] == 'finished' and candidate['exit-code'] == 0
select(S, 'idle-candidate-epoch2-run.json')
for row in candidate['steps']:
    select(S, row['log'])

prior_leaf = P / 'epoch1-worker_factory_idle_leaf_tests.rs'
if not prior_leaf.exists():
    prior_leaf.write_bytes((P / 'idle-source/src/user/windows/process/sched/tests/services_tests/worker_factory_idle_leaf_tests.rs').read_bytes())
select(P, prior_leaf.name)
for name in ['idle-candidate-epoch1-run.json', 'idle-candidate-epoch1-focus.log', 'idle-native-pipeline.json']:
    select(S, name)

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
(P / 'idle-collected-evidence.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('collected', len(rows), 'validation files')
