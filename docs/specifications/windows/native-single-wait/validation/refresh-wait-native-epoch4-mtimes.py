from pathlib import Path
import datetime,hashlib,json,os,time
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-wait-epoch4';record=S/'wait-native-epoch4-freshness.json'
assert not record.exists(),'native freshness already recorded'
deadline=time.monotonic()+1800
while True:
 prior=json.loads((S/'wait-native-epoch3b-gates.json').read_text())
 if prior['state']=='finished' and (S/'wait-native-epoch4-preparation.json').exists():break
 assert prior['state']!='failed','preceding native epoch failed'
 assert time.monotonic()<deadline,'preceding native epoch timed out'
 time.sleep(1)
manifest=json.loads((S/'wait-source-epoch4.json').read_text());changes=set(manifest['changed-paths']);rows=[]
for item in manifest['files']:
 if item['path'] not in changes:continue
 path=R/item['path'];raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==item['sha256']
 before=path.stat().st_mtime_ns;os.utime(path,None)
 assert hashlib.sha256(path.read_bytes()).hexdigest()==item['sha256']
 rows.append({'path':item['path'],'sha256':item['sha256'],'mtime-before-ns':before,'mtime-after-ns':path.stat().st_mtime_ns})
record.write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'prior-gates-completed-utc':prior['completed-utc'],'files':rows,'scope':'Refresh only the three separate override mtimes after every prior Cargo invocation finishes. This forces current source into both normal-library and test/check profiles in the warmed target. Source bytes remain unchanged.'},indent=2)+'\n')
print('Refreshed epoch4 overrides after prior native gates',flush=True)
