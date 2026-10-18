from pathlib import Path
import datetime,hashlib,json,shutil,subprocess,tarfile
B=Path(__file__).resolve().parent;R=B/'rax-source';U=B.parent/'native-token-session-id-20261017';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');W=B/'windows';W.mkdir(exist_ok=True)
plan=json.loads((B/'owned-source-plan.json').read_text());owned=[p for p in plan['paths']if p.endswith('.rs')]
subprocess.run(['rustup','run','1.95.0','rustfmt','--edition','2024','--config','skip_children=true',*[str(R/p)for p in owned]],check=True)
base=json.loads((U/'session-final-source.json').read_text());paths={r['path']for r in base['files']};paths.update(owned);paths.add('docs/specifications/windows/native-token-scalars/cases.json')
rows=[]
for name in sorted(paths):
 raw=(R/name).read_bytes();rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
overlay=owned+['docs/specifications/windows/native-token-scalars/cases.json'];a=S/'scalar-epoch1-overlay.tar.gz'
with tarfile.open(a,'w:gz')as t:
 for name in overlay:t.add(R/name,arcname=name,recursive=False)
m={'base':plan['base'],'epoch':1,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows,'overlay':[r for r in rows if r['path']in overlay],'overlay-sha256':hashlib.sha256(a.read_bytes()).hexdigest()}
for p in [B/'scalar-epoch1-source.json',S/'scalar-epoch1-source.json']:p.write_text(json.dumps(m,indent=2)+'\n')
shutil.copyfile(U/'session-final-source.json',S/'scalar-baseline-source.json')
s=(U/'run-session-id-epoch2-focus.py').read_text().replace('-session-id-epoch2-','-scalar-epoch1-');(B/'run-scalar-epoch1-focus.py').write_text(s)
script='''from pathlib import Path,PurePosixPath
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
'''
(W/'prepare-scalar-epoch1-native.py').write_text(script)
s=(U/'windows/native-session-id-epoch2-complete.ps1').read_text().replace('repair-session-id-epoch2.py','prepare-scalar-epoch1-native.py').replace('src-token-session-id','src-token-scalars').replace('native-session-id-epoch2-complete-','native-scalar-epoch1-')
(W/'native-scalar-epoch1.ps1').write_text(s)
s=(U/'windows/run-session-id-epoch2-complete.py').read_text().replace('session-id-epoch2-complete','scalar-epoch1');(W/'run-scalar-epoch1.py').write_text(s)
for p in W.iterdir():
 if p.is_file():shutil.copyfile(p,S/p.name)
print('Frozen scalar epoch1:',len(rows),'source inputs;',len(owned),'Rust files')
