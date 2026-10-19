from pathlib import Path
import datetime
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile

N = Path(__file__).resolve().parent
R = N / 'wait-source'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert not (N / 'wait-source-epoch3.json').exists()
tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=R).decode().split('\0')[:-1]
owned = json.loads((N / 'wait-owned-source-plan.json').read_text())['paths']
names = sorted(set(tracked) | {name for name in owned if name.endswith('.rs')} |
               {p.relative_to(R).as_posix() for p in (R / 'docs/specifications/windows/native-single-wait/replay').glob('*.tsv')})
rows = []
archive = N / 'wait-source-epoch3.tar.gz'
with tarfile.open(archive, 'w:gz', compresslevel=1) as tar:
    for name in names:
        path = R / name
        link = path.is_symlink()
        raw = os.readlink(path).encode() if link else path.read_bytes()
        row = {'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(), 'kind': 'symlink-text' if link else 'file'}
        rows.append(row)
        info = tarfile.TarInfo(name)
        info.size = len(raw)
        info.mode = 0o755 if not link and os.access(path, os.X_OK) else 0o644
        tar.addfile(info, io.BytesIO(raw))
manifest = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source-epoch': 3,
    'base': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=R).decode().strip(),
    'scope': 'Complete Git-tracked parent tree plus exact new compilation/test inputs, not a sparse override set. Symlinks are retained as link text for portable Windows extraction. Native Cargo source replacement is bound separately.',
    'files': rows}
for root in [N, S]:
    (root / 'wait-source-epoch3.json').write_text(json.dumps(manifest, indent=2) + '\n')
shutil.copy2(archive, S / archive.name)
record = {'archive': archive.name, 'bytes': archive.stat().st_size, 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
    'files': len(rows), 'source-bytes': sum(row['bytes'] for row in rows)}
for root in [N, S]:
    (root / 'wait-source-epoch3-archive.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
