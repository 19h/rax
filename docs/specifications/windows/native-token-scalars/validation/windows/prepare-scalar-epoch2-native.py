from pathlib import Path,PurePosixPath
import hashlib,json,tarfile
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-token-scalars'
state=json.loads((S/'scalar-epoch1-launch.json').read_text())
if state['state']!='finished':raise RuntimeError('first native focus is not finished')
old=json.loads((S/'scalar-epoch1-source.json').read_text());m=json.loads((S/'scalar-epoch2-source.json').read_text());a=S/'scalar-epoch2-overlay.tar.gz'
for row in old['files']:
 raw=(R/row['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('first source changed '+row['path'])
if hashlib.sha256(a.read_bytes()).hexdigest()!=m['overlay-sha256']:raise RuntimeError('corrected overlay drift')
allowed={r['path']for r in m['overlay']}
with tarfile.open(a)as t:
 for member in t:
  p=PurePosixPath(member.name)
  if member.name not in allowed or p.is_absolute()or '..'in p.parts or not member.isfile():raise RuntimeError('invalid overlay member')
  dst=R/Path(*p.parts);dst.write_bytes(t.extractfile(member).read())
for row in m['files']:
 raw=(R/row['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('corrected source mismatch '+row['path'])
if hashlib.sha256((R/'.cargo/config.toml').read_bytes()).hexdigest()!='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':raise RuntimeError('native config drift')
print('Verified scalar epoch2 source inputs:',len(m['files']),flush=True)
