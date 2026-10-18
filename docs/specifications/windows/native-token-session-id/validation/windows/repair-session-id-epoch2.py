from pathlib import Path,PurePosixPath
import hashlib,json,tarfile,shutil
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-token-session-id';old=B/'src-token-open';backup=B/'src-tid-e2-partial'
state=json.loads((S/'session-id-epoch2-launch.json').read_text())
if state['state']!='finished' or state['exit-code']!=101:raise RuntimeError('prior incomplete-workspace run not finished')
base=json.loads((S/'token-final-source.json').read_text());m=json.loads((S/'session-id-epoch2-complete-source.json').read_text());a=S/'session-id-epoch2-complete-overlay.tar.gz'
for row in base['files']:
 raw=(old/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('baseline changed '+row['path'])
if hashlib.sha256(a.read_bytes()).hexdigest()!=m['overlay-sha256']:raise RuntimeError('repair overlay changed')
if backup.exists():raise RuntimeError('previous repair backup exists')
R.rename(backup);shutil.copytree(old,R)
allowed={r['path']for r in m['overlay']}
with tarfile.open(a)as t:
 for member in t:
  path=PurePosixPath(member.name)
  if member.name not in allowed or path.is_absolute() or '..'in path.parts or not member.isfile():raise RuntimeError('invalid repair member')
  dst=R/Path(*path.parts);dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(t.extractfile(member).read())
for row in m['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('repaired source mismatch '+row['path'])
print('Verified complete native workspace:',len(m['files']),'selected inputs')
