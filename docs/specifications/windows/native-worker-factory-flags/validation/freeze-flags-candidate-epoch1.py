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
P = F.parent / 'native-filesystem-registry-20261019'
R = F / 'flags-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert json.loads((F / 'flags-prototype-a.json').read_text())['exit-code'] == 0
assert not (F / 'flags-candidate-epoch1.json').exists()
subprocess.run(['git', 'status', '--short', '--branch'], cwd=R, check=True)
parent_raw = (P / 'filesystem-candidate-epoch1.json').read_bytes()
parent = {row['path']: row for row in json.loads(parent_raw)['files']}
paths = set(subprocess.check_output(['git', 'ls-files', '-z'], cwd=R).decode().split('\0')) - {''}
paths.update(json.loads((F / 'flags-ownership.json').read_text())['source-owned-paths'])
paths.update(path.relative_to(R).as_posix() for path in
             (R / 'docs/specifications/windows/native-worker-factory-flags').rglob('*') if path.is_file())
rows = []
for name in sorted(paths):
    path = R / name
    raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
    rows.append({'path': name, 'kind': 'symlink-text' if path.is_symlink() else 'file',
                 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest = {'source-epoch': 1, 'base-repository-commit': '9e243946c5c4e8df0bcbc93d94d1337ec8d62171',
            'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'files': rows, 'total-bytes': sum(row['bytes'] for row in rows)}
raw = json.dumps(manifest, indent=2) + '\n'
for base in [F, S]:
    (base / 'flags-candidate-epoch1.json').write_text(raw)
changed = [row['path'] for row in rows if parent.get(row['path']) != row]
archive = F / 'flags-candidate-epoch1-overrides.tar.gz'
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
shutil.copy2(P / 'filesystem-candidate-epoch1.json', S / 'filesystem-candidate-epoch1.json')
observer = (F / 'flags-candidate-owner-epoch1.rs').read_bytes()
for base in [F, S]:
    (base / 'flags-candidate-owner-epoch1.rs').write_bytes(observer)
meta = {'archive': archive.name, 'bytes': archive.stat().st_size,
        'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
        'parent-manifest': 'filesystem-candidate-epoch1.json',
        'parent-manifest-sha256': hashlib.sha256(parent_raw).hexdigest(),
        'source-files': len(rows), 'source-bytes': manifest['total-bytes'],
        'separate-overrides': changed, 'deleted': sorted(set(parent) - paths),
        'observer-sha256': hashlib.sha256(observer).hexdigest()}
assert not meta['deleted']
for base in [F, S]:
    (base / 'flags-candidate-epoch1-archive.json').write_text(json.dumps(meta, indent=2) + '\n')
controller = (P / 'run-filesystem-candidate-epoch1.py').read_text()
controller = controller.replace('filesystem-candidate-epoch1', 'flags-candidate-epoch1')
controller = controller.replace("OLD = B / 'src-policy-candidate-epoch1'", "OLD = B / 'src-filesystem-candidate-epoch1'")
controller = controller.replace('filesystem-candidate-owner-epoch1', 'flags-candidate-owner-epoch1')
controller = controller.replace('filesystem_candidate_owner_epoch1', 'flags_candidate_owner_epoch1')
controller = controller.replace("'filesystem_registry'", "'native_worker_factory_flags'")
controller = controller.replace("'9 passed; 0 failed;'", "'7 passed; 0 failed;'")
controller = controller.replace('native-filesystem-registry-v1-modules-before.json', 'native-worker-factory-flags-v2-modules-before.json')
controller = controller.replace('Candidate fixed FileSystem registry exploration: six shared tests passed; nine native focused tests', 'Candidate dormant WorkerFactoryFlags exploration: six shared tests passed; seven native focused tests')
assert "OLD = B / 'src-filesystem-candidate-epoch1'" in controller
assert "R = B / 'src-flags-candidate-epoch1'" in controller
compile(controller, 'run-flags-candidate-epoch1.py', 'exec')
for base in [F, S]:
    (base / 'run-flags-candidate-epoch1.py').write_text(controller)
print(json.dumps({k: v for k, v in meta.items() if k != 'separate-overrides'}, indent=2))
print('separate overrides', len(changed))
