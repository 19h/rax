from pathlib import Path
import datetime, hashlib, json
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
paths=[]
for profile in ['arm64','x64','x86','x86-laa']:
 for name in ['probe.exe','native-bno-copy-boundaries.cpp']:
  paths.append(B/('bno-copy-'+profile)/name)
for folder,name in [('bno-copy-controller','native-bno-copy-child.cpp'),('bno-copy-curve-controller','native-bno-copy-curve-child.cpp')]:
 paths += [B/folder/'controller.exe',B/folder/name]
paths += [S/name for name in ['native-bno-copy-boundaries.cpp','native-bno-copy-child.cpp','native-bno-copy-curve-child.cpp']]
rows=[]
for path in paths:
 raw=path.read_bytes();rows.append({'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
out=S/'bno-copy-native-post-identities.json'
if out.exists():raise RuntimeError('post-run identities already recorded')
out.write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows},indent=2)+'\n')
print('Native copy post-run identities',len(rows),flush=True)
