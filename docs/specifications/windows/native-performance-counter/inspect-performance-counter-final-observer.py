from pathlib import Path
import json,hashlib,struct
b=Path(r"C:\Windows\Temp\assist-native-rax-20261010")
p=b/'native-performance-counter-after-trace.rs'
raw=p.read_bytes();digest=hashlib.sha256(raw).hexdigest()
if len(raw)!=11177 or digest!='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e':raise ValueError('actual observer source changed')
r={'path':str(p),'bytes':len(raw),'sha256':digest,'compiled-executable-path':str(b/'native-performance-counter-final-after-production-trace.exe')}
x=Path(r['compiled-executable-path']).read_bytes();r['executable-bytes']=len(x);r['executable-sha256']=hashlib.sha256(x).hexdigest()
if x[:2]!=b'MZ':raise ValueError('actual observer DOS signature')
at=struct.unpack_from('<I',x,0x3c)[0]
if x[at:at+4]!=b'PE\0\0':raise ValueError('actual observer PE signature')
r['executable-machine']=struct.unpack_from('<H',x,at+4)[0]
if r['executable-machine']!=0xAA64:raise ValueError('actual observer machine')
Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-performance-counter-final-observer-identity.json").write_text(json.dumps(r,indent=2)+'\n')
extended=b/'native-performance-counter-extended-trace.rs'
e=extended.read_bytes()
if e.replace(b'0..3000000',b'0..1000000').replace(b'turn == 2999999',b'turn == 999999')!=raw:raise ValueError('extended observer changes more than two budget literals')
y=(b/'native-performance-counter-extended-trace.exe').read_bytes()
r['extended-observer']={'path':str(extended),'bytes':len(e),'sha256':hashlib.sha256(e).hexdigest(),'budget-turns':3000000,'original-budget-turns':1000000,'only-budget-literals-changed':True,'executable-bytes':len(y),'executable-sha256':hashlib.sha256(y).hexdigest()}
Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-performance-counter-final-observer-identity.json").write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r))
