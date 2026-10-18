from pathlib import Path,PurePosixPath
import hashlib,json,tarfile,shutil
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');old=B/'src-token-session-id';R=B/'src-token-scalars'
state=json.loads((S/'session-final-launch.json').read_text())
if state['state']!='finished' or state['exit-code']!=0:raise RuntimeError('preceding native validation is not complete')
base=json.loads((S/'scalar-baseline-source.json').read_text());m=json.loads((S/'scalar-epoch1-source.json').read_text());a=S/'scalar-epoch1-overlay.tar.gz'
if R.exists():raise RuntimeError('scalar source already exists')
for r in base['files']:
 raw=(old/r['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(r['bytes'],r['sha256']):raise RuntimeError('baseline drift '+r['path'])
if hashlib.sha256(a.read_bytes()).hexdigest()!=m['overlay-sha256']:raise RuntimeError('overlay drift')
shutil.copytree(old,R)
allowed={r['path']for r in m['overlay']}
with tarfile.open(a)as t:
 for member in t:
  p=PurePosixPath(member.name)
  if member.name not in allowed or p.is_absolute()or '..'in p.parts or not member.isfile():raise RuntimeError('invalid overlay member')
  dst=R/Path(*p.parts);dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(t.extractfile(member).read())
for r in m['files']:
 raw=(R/r['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(r['bytes'],r['sha256']):raise RuntimeError('scalar source drift '+r['path'])
raw=(R/'.cargo/config.toml').read_bytes()
if hashlib.sha256(raw).hexdigest()!='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':raise RuntimeError('native Cargo config drift')
print('Verified scalar epoch1 source inputs:',len(m['files']),flush=True)
