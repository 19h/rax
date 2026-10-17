from pathlib import Path
import hashlib,json,sys
def require(ok, message):
 if not ok:raise ValueError(message)
require(not sys.flags.optimize, 'optimized validation is rejected')
root=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
artifacts=[json.loads(x) for x in (root/'native-access-check-final-after-artifacts.jsonl').read_text().splitlines()]
core=[x for x in artifacts if x.get('reason')=='compiler-artifact' and x['target']['name']=='rax' and not x['manifest_path'].replace('\\','/').endswith('/capi/Cargo.toml')]
require(len(core)==1 and core[0]['features']==[], 'selected empty-feature core')
paths=[root/'target-io-completion-owned/debug/assist_rs.lib',Path(next(p for p in core[0]['filenames'] if p.endswith('.rlib')))]
rows=[]
for path in paths:
 size=path.stat().st_size;h=hashlib.sha256()
 with path.open('rb') as f:
  for block in iter(lambda:f.read(1048576),b''):h.update(block)
 with path.open('rb') as f:
  require(f.read(8)==b'!<arch>\n', 'archive magic');count=0
  while f.tell()<size:
   header=f.read(60);require(len(header)==60 and header[58:60]==b'`\n', 'member header')
   member=int(header[48:58]);require(member>=0, 'nonnegative member extent');end=f.tell()+member+(member&1);require(end<=size, 'complete member extent')
   f.seek(end);count+=1
  require(f.tell()==size, 'complete archive walk')
 rows.append({'path':str(path),'bytes':size,'sha256':h.hexdigest(),'members':count,'structural-member-walk':'pass'})
manifest=json.loads(Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\access-check-final-owning-source-hashes.json').read_text())
sources=[]
for item in manifest['sources']:
 data=(root/'src'/item['path']).read_bytes();require(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'], 'native source '+item['path']);sources.append(item)
for item in manifest['helpers']:
 data=Path(item['native-path']).read_bytes();require(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'], 'owning actual helper '+item['path'])
print(json.dumps({'archives':rows,'core-features':core[0]['features'],'sources':sources,'helpers':manifest['helpers'],'observer-sha256':manifest['observer-sha256']}))
