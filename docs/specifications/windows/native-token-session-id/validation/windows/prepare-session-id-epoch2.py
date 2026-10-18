from pathlib import Path,PurePosixPath
import hashlib,json,tarfile
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-token-session-id'
old=json.loads((S/'session-id-epoch1-source.json').read_text());new=json.loads((S/'session-id-epoch2-source.json').read_text());archive=S/'session-id-epoch2-overlay.tar.gz'
if hashlib.sha256(archive.read_bytes()).hexdigest()!=new['overlay-sha256']:raise RuntimeError('overlay changed')
for row in old['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('previous source drift '+row['path'])
allowed={r['path']for r in new['overlay']}
with tarfile.open(archive)as src:
 for member in src:
  path=PurePosixPath(member.name)
  if member.name not in allowed or path.is_absolute() or '..'in path.parts or not member.isfile():raise RuntimeError('invalid overlay member')
  dst=R/Path(*path.parts);dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(src.extractfile(member).read())
for row in new['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('new source mismatch '+row['path'])
print('Verified',len(new['files']),'native session-id epoch2 inputs')
