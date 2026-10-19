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
P = F.parent / 'native-worker-factory-flags-20261019'
R = F / 'idle-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert json.loads((F / 'idle-prototype-b.json').read_text())['exit-code'] == 0
assert not (F / 'idle-candidate-epoch1.json').exists()
subprocess.run(['git', 'status', '--short', '--branch'], cwd=R, check=True)
parent_raw = (P / 'flags-candidate-epoch1.json').read_bytes()
parent = {row['path']: row for row in json.loads(parent_raw)['files']}
paths = set(subprocess.check_output(['git', 'ls-files', '-z'], cwd=R).decode().split('\0')) - {''}
paths.update(json.loads((F / 'idle-ownership.json').read_text())['source-owned-paths'])
paths.update(path.relative_to(R).as_posix() for path in
             (R / 'docs/specifications/windows/native-worker-factory-idle-timeout').rglob('*') if path.is_file())
rows = []
for name in sorted(paths):
    path = R / name
    raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
    rows.append({'path': name, 'kind': 'symlink-text' if path.is_symlink() else 'file',
                 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
manifest = {'source-epoch': 1, 'base-repository-commit': 'b325144aea85b2ab0bc90a5c69be6e3c0300a652',
            'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'files': rows, 'total-bytes': sum(row['bytes'] for row in rows)}
raw = json.dumps(manifest, indent=2) + '\n'
for base in [F, S]:
    (base / 'idle-candidate-epoch1.json').write_text(raw)
changed = [row['path'] for row in rows if parent.get(row['path']) != row]
archive = F / 'idle-candidate-epoch1-overrides.tar.gz'
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
shutil.copy2(P / 'flags-candidate-epoch1.json', S / 'flags-candidate-epoch1.json')
observer = (F / 'idle-candidate-owner-epoch1.rs').read_bytes()
for base in [F, S]:
    (base / 'idle-candidate-owner-epoch1.rs').write_bytes(observer)
meta = {'archive': archive.name, 'bytes': archive.stat().st_size,
        'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
        'parent-manifest': 'flags-candidate-epoch1.json',
        'parent-manifest-sha256': hashlib.sha256(parent_raw).hexdigest(),
        'source-files': len(rows), 'source-bytes': manifest['total-bytes'],
        'separate-overrides': changed, 'deleted': sorted(set(parent) - paths),
        'observer-sha256': hashlib.sha256(observer).hexdigest()}
assert not meta['deleted']
for base in [F, S]:
    (base / 'idle-candidate-epoch1-archive.json').write_text(json.dumps(meta, indent=2) + '\n')
controller = (P / 'run-flags-candidate-epoch1.py').read_text()
controller = controller.replace('flags-candidate', 'idle-candidate')
controller = controller.replace('flags_candidate', 'idle_candidate')
controller = controller.replace("OLD = B / 'src-filesystem-candidate-epoch1'", "OLD = B / 'src-flags-candidate-epoch1'")
controller = controller.replace('native_worker_factory_flags', 'native_worker_factory_idle')
controller = controller.replace('native-worker-factory-flags-v2-modules-before.json', 'native-worker-factory-idle-v3-modules-before.json')
controller = controller.replace('WorkerFactoryFlags', 'WorkerFactoryIdleTimeout')
assert "OLD = B / 'src-flags-candidate-epoch1'" in controller
assert "R = B / 'src-idle-candidate-epoch1'" in controller
compile(controller, 'run-idle-candidate-epoch1.py', 'exec')
for base in [F, S]:
    (base / 'run-idle-candidate-epoch1.py').write_text(controller)
print(json.dumps({k: v for k, v in meta.items() if k != 'separate-overrides'}, indent=2))
print('separate overrides', len(changed))
