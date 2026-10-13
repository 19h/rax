from pathlib import Path
import json,hashlib
b=Path(r"C:\Windows\Temp\assist-native-rax-20261010")
p=b/'native-scheduler-shared-after-trace.rs'
raw=p.read_bytes();digest=hashlib.sha256(raw).hexdigest()
if len(raw)!=11177 or digest!='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e':raise ValueError('actual observer source changed')
r={'path':str(p),'bytes':len(raw),'sha256':digest,'compiled-executable-path':str(b/'native-scheduler-shared-final-after-production-trace.exe')}
x=Path(r['compiled-executable-path']).read_bytes();r['executable-bytes']=len(x);r['executable-sha256']=hashlib.sha256(x).hexdigest()
Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-scheduler-shared-final-observer-identity.json").write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r))
