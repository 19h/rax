from pathlib import Path
import json,hashlib
b=Path('C:/Windows/Temp/assist-native-rax-20261010')
r={}
p=b/'assist/rust/assist-rs/.cargo/config.toml'
r['adapter']={'text':p.read_text(),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
r['directories']={}
for folder in ['dependencies','assist-dependencies']:
 rows=[]
 for d in sorted((b/folder).iterdir()):
  p=d/'.cargo-checksum.json'
  if p.is_file():
   js=json.loads(p.read_text()); missing={n:h for n,h in js['files'].items() if not (d/n).is_file()}
   if missing: rows.append({'name':d.name,'missing':missing})
 r['directories'][folder]=rows
out=b/'cookie-dependency-missing.json'
out.write_text(json.dumps(r,indent=2))
Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/cookie-dependency-missing.json').write_bytes(out.read_bytes())
print(json.dumps({'adapter':r['adapter'],'missing-package-count':{k:len(v) for k,v in r['directories'].items()}}))
