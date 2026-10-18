from pathlib import Path
import json,hashlib,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=Path('C:/Windows/Temp/assist-native-rax-20261010/assist');m=json.loads((s/'current-cpp-consumers-scalar-root.json').read_text());rows=[]
for item in m['files']:
 data=(r/item['path']).read_bytes()
 if len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('frozen Windows/current CPP source mismatch '+item['path'])
 rows.append(item)
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'current-root-head':m['current-root-head'],'frozen-windows-root-head':'f386ed2aeac29a701d32f28eb73c14003eb55f7a','verified-count':len(rows),'files':rows,'scope':'exact9C++inputs;headers/generated resources/dependencies and whole-root parity are not established by this comparison'}
(s/'native-scalar-final-current-cpp-verification.json').write_text(json.dumps(result,indent=2)+'\n');print('Verified9 actual frozen Windows C++ sources against current root',flush=True)
