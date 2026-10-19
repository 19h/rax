from pathlib import Path
import datetime,hashlib,io,json,os,shutil,tarfile,time

N=Path(__file__).resolve().parent;R=N/'wait-source';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert not (N/'wait-source-epoch4.json').exists()
old=json.loads((N/'wait-source-epoch3.json').read_text());rows=[];changed=[]
for row in old['files']:
 p=R/row['path'];raw=os.readlink(p).encode() if row['kind']=='symlink-text' else p.read_bytes()
 current=dict(row,bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());rows.append(current)
 if current['sha256']!=row['sha256']:changed.append(current['path'])
assert changed==['src/user/windows/dll/mod.rs','src/user/windows/dll/native.rs','src/user/windows/dll/threading/native_wait_tests.rs'],changed
archive=N/'wait-source-epoch4-overrides.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
 for name in changed:
  raw=(R/name).read_bytes();info=tarfile.TarInfo(name);info.size=len(raw);info.mode=0o644;info.mtime=int(time.time());tar.addfile(info,io.BytesIO(raw))
manifest=dict(old,**{'source-epoch':4,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows,
 'scope':'Complete22059-input snapshot derived from the exact epoch3 archive plus three explicit source overrides. Existing synthetic NTDLL ordinals/trap RVAs remain stable; native operations and timeout/APC semantics are unchanged.',
 'prior-manifest':'wait-source-epoch3.json','changed-paths':changed})
meta={'archive':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'source-files':len(rows),'source-bytes':sum(r['bytes'] for r in rows),'overrides':changed}
for root in [N,S]:
 (root/'wait-source-epoch4.json').write_text(json.dumps(manifest,indent=2)+'\n');(root/'wait-source-epoch4-archive.json').write_text(json.dumps(meta,indent=2)+'\n')
shutil.copy2(archive,S/archive.name)
print(json.dumps(meta))
