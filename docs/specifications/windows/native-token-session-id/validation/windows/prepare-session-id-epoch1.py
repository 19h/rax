from pathlib import Path,PurePosixPath
import hashlib,json,tarfile
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-token-session-id'
m=json.loads((S/'session-id-epoch1-source.json').read_text());archive=S/'session-id-epoch1-source.tar.gz'
if hashlib.sha256(archive.read_bytes()).hexdigest()!=m['archive-sha256']:raise RuntimeError('source archive changed')
if R.exists():raise RuntimeError('session source directory already exists')
R.mkdir()
with tarfile.open(archive)as src:
 for member in src:
  name=PurePosixPath(member.name)
  if name.is_absolute() or '..'in name.parts or not member.isfile():raise RuntimeError('unsafe source member')
  dst=R/Path(*name.parts);dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(src.extractfile(member).read())
config=(B/'src-token-open/.cargo/config.toml').read_bytes()
if hashlib.sha256(config).hexdigest()!='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':raise RuntimeError('native Cargo config changed')
(R/'.cargo').mkdir(exist_ok=True);(R/'.cargo/config.toml').write_bytes(config)
for row in m['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('source mismatch '+row['path'])
print('Verified',len(m['files']),'native session-id source inputs')
