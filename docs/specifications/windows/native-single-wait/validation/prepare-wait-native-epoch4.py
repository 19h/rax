from pathlib import Path
import datetime,hashlib,json,os,shutil,tarfile

B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
OLD=B/'src-wait-epoch3';R=B/'src-wait-epoch4'
M=json.loads((S/'wait-source-epoch4.json').read_text());P=json.loads((S/'wait-source-epoch3.json').read_text())
META=json.loads((S/'wait-source-epoch4-archive.json').read_text())
CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
assert not R.exists(),'epoch4 source already materialized; inspect prior preparation'
for row in P['files']:
 raw=(OLD/row['path']).read_bytes();expected=CONFIG if row['path']=='.cargo/config.toml' else row['sha256']
 assert hashlib.sha256(raw).hexdigest()==expected,row['path']
archive=S/META['archive'];assert archive.stat().st_size==META['bytes'] and hashlib.sha256(archive.read_bytes()).hexdigest()==META['sha256']
R.mkdir();changed=set(META['overrides']);linked=0;copied=[]
for row in M['files']:
 if row['path'] in changed:continue
 source=OLD/row['path'];target=R/row['path'];target.parent.mkdir(exist_ok=True,parents=True)
 try:os.link(source,target);linked+=1
 except OSError:shutil.copy2(source,target);copied.append(row['path'])
with tarfile.open(archive,'r:gz')as tar:
 members=tar.getmembers();assert len(members)==3 and {m.name for m in members}==changed
 assert all(m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts for m in members)
 tar.extractall(R,filter='data')
for name in changed:os.utime(R/name,None)
for row in M['files']:
 raw=(R/row['path']).read_bytes();expected=CONFIG if row['path']=='.cargo/config.toml' else row['sha256']
 assert hashlib.sha256(raw).hexdigest()==expected,row['path']
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':4,'complete-inputs':len(M['files']),'immutable-linked-inputs':linked,'fallback-copies':copied,'separate-overrides':sorted(changed),'source-bytes':META['source-bytes'],'scope':'Complete verified epoch3 snapshot plus three separately materialized overrides. Unchanged hardlinked inputs are never written; overrides receive current mtimes to force rebuild. Full hashes are checked before and after the final gates.'}
(S/'wait-native-epoch4-preparation.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
