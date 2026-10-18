from pathlib import Path
import hashlib,json,struct
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');rows=[]
for profile in ['arm64','x64','x86','x86-laa']:
 root=b/('token-session-private-'+profile);src=root/'native-token-session-private.cpp';exe=root/'probe.exe';raw=src.read_bytes()
 if hashlib.sha256(raw).hexdigest()!='71625c09e29065c8496d6e0f7d42017fad9cd65085d4ac67db07525c8a3cf774':raise RuntimeError('source changed '+profile)
 image=exe.read_bytes();pe=struct.unpack_from('<I',image,0x3c)[0];machine=struct.unpack_from('<H',image,pe+4)[0];flags=struct.unpack_from('<H',image,pe+22)[0]
 rows.append({'profile':profile,'source':{'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()},'executable':{'bytes':len(image),'sha256':hashlib.sha256(image).hexdigest(),'machine':machine,'large-address-aware':bool(flags&0x20)}})
(s/'token-session-private-post-run.json').write_text(json.dumps(rows,indent=2)+'\n');print('Verified four post-run session-id sources and PEs')
