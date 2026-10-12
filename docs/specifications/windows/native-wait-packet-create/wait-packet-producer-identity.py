from pathlib import Path
import json,hashlib,struct
root=Path(r'C:\Windows\Temp\assist-native-rax-20261010');share=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
rows=[]
for family in ['create','lifetime','parity']:
 for profile in ['arm64','x64','x86','x86-laa']:
  folder=root/f'{"wait-parity" if family=="parity" else "wait-packet-"+family}-probe-{profile}';source=folder/f'native-{"wait-parity" if family=="parity" else "wait-packet-"+family}-probe.cpp';exe=folder/f'native-{"wait-parity" if family=="parity" else "wait-packet-"+family}-probe-{profile}.exe';data=exe.read_bytes();at=struct.unpack_from('<I',data,60)[0]
  if data[:2]!=b'MZ' or data[at:at+4]!=b'PE\0\0':raise ValueError('invalid actual PE')
  machine,sections=struct.unpack_from('<HH',data,at+4);optional=at+24;magic=struct.unpack_from('<H',data,optional)[0];wide=magic==0x20b
  if magic not in (0x10b,0x20b):raise ValueError('optional header')
  stack=struct.unpack_from('<QQ' if wide else '<II',data,optional+72)
  rows.append({'family':family,'profile':profile,'source-path':str(source),'source-bytes':source.stat().st_size,'source-sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'executable-path':str(exe),'executable-bytes':len(data),'executable-sha256':hashlib.sha256(data).hexdigest(),'machine':hex(machine),'optional-magic':hex(magic),'PE-stack-reserve':stack[0],'PE-stack-commit':stack[1]})
(share/'wait-packet-producer-identity.json').write_text(json.dumps(rows,indent=2)+'\n');print('actual executable/source identities',len(rows))
