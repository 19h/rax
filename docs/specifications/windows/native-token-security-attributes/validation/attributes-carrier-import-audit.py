from pathlib import Path
import hashlib,json,sys,subprocess
b=Path('C:/Windows/Temp/assist-native-rax-20261010')
r={'python':sys.version,'paths':sys.path,'files':[]}
for rel in ['assist/scripts/static_text/pack_rust.py','assist/scripts/static_text/lzfse_lzvn.py','python/python314._pth','python/python._pth']:
 p=b/rel
 row={'path':str(p),'exists':p.exists()}
 if p.exists():
  raw=p.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest(),prefix=raw[:700].decode('utf8','replace'))
 r['files'].append(row)
print(json.dumps(r,indent=2),flush=True)
