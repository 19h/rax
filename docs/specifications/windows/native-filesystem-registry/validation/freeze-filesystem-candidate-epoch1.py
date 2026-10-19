from pathlib import Path
import datetime
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile


F = Path(__file__).resolve().parent
P = F.parent / 'native-user-policy-20261019'
R = F / 'filesystem-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert json.loads((F / 'filesystem-prototype-a.json').read_text())['exit-code'] == 0
assert not (F / 'filesystem-candidate-epoch1.json').exists()
subprocess.run(['git', 'status', '--short', '--branch'], cwd=R, check=True)
parent_raw = (P / 'policy-candidate-epoch1.json').read_bytes()
parent = {row['path']: row for row in json.loads(parent_raw)['files']}
paths = set(subprocess.check_output(['git', 'ls-files', '-z'], cwd=R).decode().split('\0')) - {''}
paths.update(json.loads((F / 'filesystem-ownership.json').read_text())['source-owned-paths'])
paths.update(path.relative_to(R).as_posix() for path in
             (R / 'docs/specifications/windows/native-filesystem-registry').rglob('*') if path.is_file())
rows = []
for name in sorted(paths):
    path = R / name
    raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
    rows.append({'path': name, 'kind': 'symlink-text' if path.is_symlink() else 'file',
                 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest = {'source-epoch': 1, 'base-repository-commit': '399506af43fa9988140ba9a4d8c0d189af59b05f',
            'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'files': rows, 'total-bytes': sum(row['bytes'] for row in rows)}
raw = json.dumps(manifest, indent=2) + '\n'
for base in [F, S]:
    (base / 'filesystem-candidate-epoch1.json').write_text(raw)
changed = [row['path'] for row in rows if parent.get(row['path']) != row]
archive = F / 'filesystem-candidate-epoch1-overrides.tar.gz'
with tarfile.open(archive, 'w:gz', dereference=True) as tar:
    for name in changed:
        path = R / name
        if path.is_symlink():
            raw = os.readlink(path).encode()
            entry = tarfile.TarInfo(name)
            entry.size = len(raw)
            tar.addfile(entry, io.BytesIO(raw))
        else:
            tar.add(path, arcname=name, recursive=False)
shutil.copy2(archive, S / archive.name)
shutil.copy2(P / 'policy-candidate-epoch1.json', S / 'policy-candidate-epoch1.json')
observer = (P / 'policy-candidate-owner-epoch1.rs').read_bytes()
for base in [F, S]:
    (base / 'filesystem-candidate-owner-epoch1.rs').write_bytes(observer)
meta = {'archive': archive.name, 'bytes': archive.stat().st_size,
        'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
        'parent-manifest': 'policy-candidate-epoch1.json',
        'parent-manifest-sha256': hashlib.sha256(parent_raw).hexdigest(),
        'source-files': len(rows), 'source-bytes': manifest['total-bytes'],
        'separate-overrides': changed, 'deleted': sorted(set(parent) - paths),
        'observer-sha256': hashlib.sha256(observer).hexdigest()}
assert not meta['deleted']
for base in [F, S]:
    (base / 'filesystem-candidate-epoch1-archive.json').write_text(json.dumps(meta, indent=2) + '\n')
controller = (P / 'run-policy-candidate-epoch1.py').read_text()
controller = controller.replace('policy-candidate-epoch1', 'filesystem-candidate-epoch1')
controller = controller.replace("OLD = B / 'src-user-candidate-epoch1'", "OLD = B / 'src-policy-candidate-epoch1'")
controller = controller.replace('policy-candidate-owner-epoch1', 'filesystem-candidate-owner-epoch1')
controller = controller.replace('policy_candidate_owner_epoch1', 'filesystem_candidate_owner_epoch1')
controller = controller.replace("'user_policy'", "'filesystem_registry'")
controller = controller.replace('native-user-policy-v3-modules-before.json', 'native-filesystem-registry-v1-modules-before.json')
controller = controller.replace('Candidate fixed per-user policy exploration', 'Candidate fixed FileSystem registry exploration')
assert "OLD = B / 'src-policy-candidate-epoch1'" in controller
assert "R = B / 'src-filesystem-candidate-epoch1'" in controller
compile(controller, 'run-filesystem-candidate-epoch1.py', 'exec')
for base in [F, S]:
    (base / 'run-filesystem-candidate-epoch1.py').write_text(controller)
print(json.dumps({k: v for k, v in meta.items() if k != 'separate-overrides'}, indent=2))
print('separate overrides', len(changed))
