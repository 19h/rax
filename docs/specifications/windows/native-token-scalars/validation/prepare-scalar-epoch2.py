from pathlib import Path
import datetime,hashlib,json,shutil,tarfile
B=Path(__file__).resolve().parent;R=B/'rax-source';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');W=B/'windows'
if not(B/'alignment-fix-applied.json').exists():raise RuntimeError('apply reviewed fixes after first focus outcomes')
old=json.loads((B/'scalar-epoch1-source.json').read_text());allowed=set(json.loads((B/'owned-source-plan.json').read_text())['paths']);rows=[];changed=[]
for row in old['files']:
 raw=(R/row['path']).read_bytes();new={'path':row['path'],'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()};rows.append(new)
 if new!=row:
  if row['path']not in allowed:raise RuntimeError('unowned source drift '+row['path'])
  changed.append(new)
if not changed:raise RuntimeError('no corrected source')
a=S/'scalar-epoch2-overlay.tar.gz'
with tarfile.open(a,'w:gz')as t:
 for row in changed:t.add(R/row['path'],arcname=row['path'],recursive=False)
m={'base':old['base'],'epoch':2,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows,'overlay':changed,'overlay-sha256':hashlib.sha256(a.read_bytes()).hexdigest()}
for p in [B/'scalar-epoch2-source.json',S/'scalar-epoch2-source.json']:p.write_text(json.dumps(m,indent=2)+'\n')
s='''from pathlib import Path,PurePosixPath
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
'''
(W/'prepare-scalar-epoch2-native.py').write_text(s)
for oldname,newname in [('native-scalar-epoch1.ps1','native-scalar-epoch2.ps1'),('run-scalar-epoch1.py','run-scalar-epoch2.py')]:
 s=(W/oldname).read_text().replace('scalar-epoch1','scalar-epoch2');(W/newname).write_text(s)
for p in W.iterdir():
 if p.is_file()and'epoch2'in p.name:shutil.copyfile(p,S/p.name)
s=(B/'run-scalar-epoch1-focus.py').read_text().replace('-scalar-epoch1-','-scalar-epoch2-');(B/'run-scalar-epoch2-focus.py').write_text(s)
print('Prepared scalar epoch2:',len(rows),'inputs;',len(changed),'corrected files')
