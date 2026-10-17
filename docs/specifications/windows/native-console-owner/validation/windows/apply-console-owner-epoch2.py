from pathlib import Path
import json,hashlib,shutil,os,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=Path('C:/Windows/Temp/assist-native-rax-20261010/src');m=json.loads((s/'console-owner-epoch2-overlay.json').read_text());overlay=s/'console-owner-epoch2-overlay'
if hashlib.sha256((r/'.cargo/config.toml').read_bytes()).hexdigest()!=m['native-config-adapter-sha256']:raise ValueError('native Cargo adapter changed')
def identity(path):
 if not path.exists():return None
 data=path.read_bytes();return {'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
for row in m['files']:
 expected={k:row[k] for k in ['bytes','sha256']}
 if identity(overlay/row['path'])!=expected:raise ValueError('overlay mismatch '+row['path'])
 actual=identity(r/row['path'])
 if actual not in [row['before'],expected]:raise ValueError('unexpected native before image '+row['path'])
for row in m['files']:
 dest=r/row['path'];dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(overlay/row['path'],dest)
 # Deliberately use current native mtimes; preserving old Mac mtimes could
 # otherwise make Cargo retain an artifact built after those old timestamps.
 os.utime(dest,None)
 if identity(dest)!={k:row[k] for k in ['bytes','sha256']}:raise ValueError('native write mismatch '+row['path'])
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'published-base':m['published-base'],'epoch':2,'verified-count':len(m['files']),'files':m['files'],'native-cargo-adapter-preserved':True,'all-changed-inputs-have-fresh-native-mtimes':True,'scope':'exact13Rust and new primary corpus overlay;unchanged capture inputs retain their previous verified source evidence;no new full-tree hash claim'}
(s/'native-console-owner-epoch2-source-verification.json').write_text(json.dumps(record,indent=2)+'\n');print('Applied and verified',len(m['files']),'exact console-owner overlay inputs with fresh native mtimes',flush=True)
