from pathlib import Path
import hashlib,json
root=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
share=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
def require(condition,message):
 if not condition:raise ValueError(message)
source=json.loads((share/'symbolic-link-open-owning-source-hashes.json').read_text())
for item in source['sources']:
 data=(root/'src'/item['path']).read_bytes();require(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'],'current owning Rust source '+item['path'])
scope=json.loads((share/'symbolic-link-open-owning-root-scope.json').read_text())
consumers=[]
for item in scope['consumers']:
 data=(root/'assist'/item['path']).read_bytes();digest=hashlib.sha256(data).hexdigest();require(digest==item['actual-sha256'],'current owning C++ consumer '+item['path']);consumers.append({'path':item['path'],'bytes':len(data),'sha256':digest})
print(json.dumps({'rust-source-units':len(source['sources']),'cpp-consumers':consumers,'owning-windows-root-head':scope['owning-windows-root-head']}))
