from pathlib import Path
import hashlib,json,struct,platform
s=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
rows=json.loads((s/'native-etw-register-guids-producer-inputs.json').read_text())
def identity(p):
 d=p.read_bytes();r={'path':str(p),'bytes':len(d),'sha256':hashlib.sha256(d).hexdigest()}
 if d[:2]==b'MZ':
  at=struct.unpack_from('<I',d,0x3c)[0]
  if d[at:at+4]!=b'PE\0\0':raise RuntimeError('invalid PE signature')
  r['machine']=struct.unpack_from('<H',d,at+4)[0];r['characteristics']=struct.unpack_from('<H',d,at+22)[0]
 return r
out={'windows-version':platform.version(),'host-machine':platform.machine(),'producers':[],'native-libraries':[]}
for row in rows:
 p=Path(row['directory']);source=identity(p/row['source'])
 if source['sha256']!=row['source-sha256']:raise RuntimeError('source mismatch '+row['profile'])
 log=identity(p/row['log']);shared=identity(s/row['log'])
 if log['sha256']!=shared['sha256']:raise RuntimeError('log mismatch '+row['profile'])
 out['producers'].append(dict(row,source_identity=source,executable_identity=identity(p/row['executable']),log_identity=log))
for p in [Path(r'C:\Windows\System32\ntdll.dll'),Path(r'C:\Windows\SysWOW64\ntdll.dll')]:out['native-libraries'].append(identity(p))
(s/'native-etw-register-guids-producer-identities.json').write_text(json.dumps(out,indent=2)+'\n')
print('producers',len(out['producers']),'native-libraries',len(out['native-libraries']))
