from pathlib import Path
import datetime,hashlib,io,json,os,shutil,subprocess,tarfile,time
Q=Path(__file__).resolve().parent;R=Q/'timer2-source'
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O=Q.parent/'native-registry-runtime-20261019'
assert not (Q/'timer2-source-epoch1.json').exists()
proto=json.loads((Q/'timer2-prototype-macos-epoch2.json').read_text());assert proto['state']=='finished' and proto['exit-code']==0
names=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=R).decode().split('\0')
rows=[]
for name in sorted(set(names)-{''}):
 p=R/name;assert p.is_file() or p.is_symlink(),name
 raw=os.readlink(p).encode() if p.is_symlink() else p.read_bytes()
 rows.append({'path':name,'kind':'symlink-text' if p.is_symlink() else 'file','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
parent_path=O/'registry-source-epoch1.json';parent_raw=parent_path.read_bytes();previous={x['path']:x for x in json.loads(parent_raw)['files']}
changed=[x['path'] for x in rows if previous.get(x['path'])!=x]
deleted=sorted(set(previous)-{x['path'] for x in rows});assert not deleted,deleted
archive=Q/'timer2-source-epoch1-overrides.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
 for row in rows:
  if row['path'] not in changed:continue
  p=R/row['path'];raw=os.readlink(p).encode() if p.is_symlink() else p.read_bytes()
  info=tarfile.TarInfo(row['path']);info.size=len(raw);info.mode=0o644;info.mtime=int(time.time());tar.addfile(info,io.BytesIO(raw))
manifest={'feature':'native-timer2-arm','source-epoch':1,'base-repository-commit':'0621926beb7d9f065ba2afddf9bcb95c74e26ba3','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows,'scope':'Complete repository inputs before final validation, with exact link-text representation on Windows and only the separately bound native Cargo source adapter.'}
meta={'archive':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'parent-manifest':'registry-source-epoch1.json','parent-manifest-sha256':hashlib.sha256(parent_raw).hexdigest(),'source-files':len(rows),'source-bytes':sum(x['bytes'] for x in rows),'separate-overrides':changed,'deleted':deleted}
for root in [Q,S]:
 (root/'timer2-source-epoch1.json').write_text(json.dumps(manifest,indent=2)+'\n')
 (root/'timer2-source-epoch1-archive.json').write_text(json.dumps(meta,indent=2)+'\n')
shutil.copy2(archive,S/archive.name)
print(json.dumps({k:v for k,v in meta.items() if k!='separate-overrides'},indent=2));print('separate overrides',len(changed))
