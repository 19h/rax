from pathlib import Path
import json,hashlib
b=Path(r'C:\Windows\Temp\assist-native-rax-20261010');s=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
data=(b/'assist/rust/assist-rs/.cargo/config.toml').read_bytes().replace(b'assist-dependencies',b'dependencies')
if hashlib.sha256(data).hexdigest()!='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c':raise ValueError('original adapter bytes did not reproduce')
(b/'src/.cargo/config.toml').write_bytes(data)
m=json.loads((s/'native-timer2-current-overlay-resolved-manifest.json').read_text());roots={'rax':b/'src','root':b/'assist'};errors=[]
for r in m['source-files']+m['environment-adapters']:
 p=roots[r['repo']]/r['path'];data=p.read_bytes()
 if len(data)!=r['bytes'] or hashlib.sha256(data).hexdigest()!=r['sha256']:errors.append(r['path'])
result={'source-files':len(m['source-files']),'patches':len(m['patches']),'root-head':m['root-head'],'rax-parent':m['rax-parent'],'environment-adapters':m['environment-adapters'],'errors':errors}
(s/'native-timer2-current-source-check.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
if errors:raise SystemExit(1)
